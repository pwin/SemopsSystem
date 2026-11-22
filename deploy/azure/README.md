# Running SemOps System on Azure

The same image runs on Azure as everywhere else: `ghcr.io/pwin/semops`, holding
`holos-server`, `holos` and `semops`. There is no Azure-specific build. This directory holds
the templates for two hosts, the identity policy for API Management, and this guide to
choosing between them, with particular attention to NFS storage. NFS can hold a HOLOS store
safely, but only some NFS services and only some deployment patterns.

| Path | What it is |
| --- | --- |
| [`aks/holos/`](aks/holos/) | Helm chart for AKS: one writer, optional read replicas, scheduled checkpoints |
| [`containerapps/main.bicep`](containerapps/main.bicep) | Bicep for Azure Container Apps, with the data on an Azure Files NFS share |
| [`apim/holos-identity.xml`](apim/holos-identity.xml) | API Management policy: validates the Entra ID token and sets HOLOS's identity headers |

## Which option

| You need | Use | Storage |
| --- | --- | --- |
| A writable store in production | AKS, the chart's default | Premium SSD managed disk |
| A writable store that survives a node or zone failure without a disk detach | AKS, `store.storageClassName` set to an Azure NetApp Files class | NetApp Files, NFS 4.1 (or NFS 3 with NLM) |
| A small or read-mostly writable store, with no cluster | Container Apps, `mode=writer` | Azure Files premium, NFS 4.1 |
| More query throughput than one server gives | Read replicas from a checkpoint: AKS `replicas.enabled`, or Container Apps `mode=replica` | Any NFS share, read-only, for the checkpoints |
| A published read-only endpoint that scales to zero | Container Apps, `mode=snapshot` | An N-Quads snapshot on Azure Files |
| Batch loads and `semops check` in CI | Container Apps Jobs, or an AKS Job, with the in-memory store | Whatever holds the input |

## NFS on Azure, and what HOLOS can do with each

HOLOS keeps its store in RocksDB, which admits one writer by taking a POSIX (`fcntl`) lock
on the store's `LOCK` file. Whether a network filesystem can hold a writable store comes
down to whether that lock reaches the server, where every client sees it, or stays on the
client, where it keeps out nobody else.

| Service | Protocol | Do locks reach the server? | Writable store | Checkpoints and read replicas | Snapshots and input files |
| --- | --- | --- | --- | --- | --- |
| Azure NetApp Files | NFS 4.1 | Yes, in the protocol | **Yes.** The best NFS home for a writer: sub-millisecond latency | Yes | Yes |
| Azure NetApp Files | NFS 3 | Yes, through NLM | **Yes** | Yes | Yes |
| Azure Files premium | NFS 4.1 | Yes, in the protocol | **With care.** Safe, since a second writer is refused, but each synchronous write costs a network round trip. Small or read-mostly stores | Yes | Yes |
| Azure Blob Storage | NFS 3.0 | **No.** Mounts require `nolock` | **No. HOLOS refuses to open one** | Yes: nothing opens a checkpoint writable | Yes |
| Azure Files | SMB | SMB's own locks | Not supported: SMB's POSIX semantics on Linux are partial, and RocksDB is not tested on it | Possible, untested | Yes |

### What enforces this

- **HOLOS refuses a writable store on a mount whose locks stay on the client.** On Linux it
  reads the mount table, and a store directory on NFS mounted with `nolock`
  (`local_lock=all` or `local_lock=posix`) is refused with an error naming the mount and the
  option. `HOLOS_ALLOW_LOCAL_LOCKS=1` overrides this, for a deployment that guarantees a
  single writer some other way and accepts what happens if it does not.
- **A store opened read-only takes no lock and writes nothing**
  (`holos-server --store-read-only`). Any number of servers on any number of machines can
  open one store this way at once. This is what makes checkpoints on a shared volume work
  as read replicas, on every NFS service in the table, Blob NFS included.
- **The deployments never run two writers.** The chart's writer is a StatefulSet fixed at
  one replica with ordered pod management, so a replacement starts only after the old pod
  has gone. The Container Apps writer runs one replica in single-revision mode. During a
  revision change the new replica is refused the lock until the old one lets go, and
  restarts until it can open the store.
- **A server on a network filesystem says so at start-up**, naming the filesystem and the
  mount, because the slowdown is easier to diagnose when it is in the log.

### Checkpoints and read replicas

The writer takes consistent checkpoints while it runs (`POST /backup`, with `--backup-dir`
on the shared volume). RocksDB builds each checkpoint under `<name>.tmp` and renames it when
it is complete. A checkpoint never changes afterwards, which is why replicas can share it.

Each replica opens the newest finished checkpoint read-only and serves queries from it. In
the chart, each replica looks for a newer checkpoint every `replicas.refreshSeconds` and,
when one appears, restarts onto it after a random delay of up to `replicas.staggerSeconds`.
The delay stops the replicas restarting all at once, and readiness probes keep traffic on
those that are up. Replicas are as fresh as the last checkpoint. Writes and
read-your-writes queries go to the writer's service.

When the store is on a managed disk and the checkpoints are on NFS, RocksDB copies the files
instead of hard-linking them, so a checkpoint costs a full copy. Size `backup.schedule` to
the store.

## Deploying

### AKS (Helm)

```sh
# The default: one writer on a Premium SSD managed disk.
helm install holos deploy/azure/aks/holos -n semops --create-namespace

# Writer on Azure NetApp Files (a Trident or static-provisioning StorageClass you have made).
helm install holos deploy/azure/aks/holos -n semops \
  --set store.storageClassName=azure-netapp-files-premium

# Writer on a managed disk, nightly checkpoints on Azure Files NFS, three read replicas.
# Checkpoints need an identity that can carry the backup role, so identity is forwarded,
# which needs an edge that strips and sets the identity headers (API Management, below).
helm install holos deploy/azure/aks/holos -n semops \
  --set shared.enabled=true \
  --set backup.enabled=true --set identity.mode=forwarded \
  --set replicas.enabled=true --set replicas.count=3
kubectl -n semops create job --from=cronjob/holos-backup first-checkpoint
```

The chart refuses combinations that cannot work: replicas or backups without the shared
volume, and backups without forwarded identity. Its NetworkPolicy admits only the ingress
namespace and the release's backup job. With forwarded identity, whoever reaches the
server's port can claim any role, so keep that policy, or one as strict, in place.

### Container Apps (Bicep)

```sh
az group create -n semops -l westeurope
az deployment group create -g semops -f deploy/azure/containerapps/main.bicep \
  -p namePrefix=semops mode=replica entraClientId=<app registration client id>
```

With the Azure Developer CLI, the repository root is an `azd` template for the same
deployment. `azd up` asks for a subscription, a region and a resource group, and provisions
the Bicep with the values below:

```sh
azd init -e semopsdev        # the environment name prefixes every resource: letters and digits, 3 to 12
azd env set SEMOPS_MODE replica
azd env set ENTRA_CLIENT_ID <app registration client id>
azd up
```

| Variable | Bicep parameter | Default |
| --- | --- | --- |
| `AZURE_ENV_NAME` | `namePrefix` | (the environment name) |
| `AZURE_LOCATION` | `location` | (asked for) |
| `SEMOPS_IMAGE` | `image` | `ghcr.io/pwin/semops:0.29.0` |
| `SEMOPS_MODE` | `mode` | `replica` |
| `SEMOPS_SNAPSHOT_FILE` | `snapshotFile` | `snapshots/latest.nq.gz` |
| `ENTRA_CLIENT_ID` | `entraClientId` | empty: no sign-in |

Until a release publishes the image, set `SEMOPS_IMAGE` to one you have pushed to a registry
the environment can pull from. `azd env get-values` shows the deployment's outputs,
`ENDPOINT` among them.

The template builds a virtual network, a premium FileStorage account with an NFS 4.1 share
reachable only from the Container Apps subnet (no public access, account keys off), a
VNet-integrated environment with the share mounted, and the app. Ingress is HTTPS-only.
With `entraClientId` set, built-in authentication refuses any request without a valid Entra
ID token for that application.

The share starts empty, and its root belongs to root, while the server runs as uid 10001.
Before the first `writer` or `replica` deployment, mount the share once from a VM in the
virtual network and prepare it:

```sh
sudo mount -t nfs -o vers=4,minorversion=1,sec=sys,nconnect=4 \
  <account>.file.core.windows.net:/<account>/holos /mnt/holos
sudo mkdir -p /mnt/holos/store /mnt/holos/checkpoints /mnt/holos/snapshots
sudo chown -R 10001:10001 /mnt/holos
```

For `snapshot` mode, put an N-Quads dump there (`snapshots/latest.nq.gz`, or set
`snapshotFile`). `holos dump`, or `POST /backup` on an in-memory server, makes one.

## Identity

HOLOS authenticates nobody itself. It applies policy to claims the edge has verified
(`holos/ACCESS-CONTROL.md`). On Azure the edge is one of two things:

- **Container Apps or App Service built-in authentication.** The platform validates the
  Entra ID token and passes the claims in `X-MS-CLIENT-PRINCIPAL`. Start the server with
  `--trust-azure-easy-auth` and it takes the object id as the principal, and app roles and
  `groups` as roles. Set authentication to required, so nothing unauthenticated reaches the
  container. Do not give the container any other way in, such as internal ingress from
  other apps in the environment: a caller that bypasses the platform's authentication could
  write that header itself.
- **API Management**, in front of AKS or anything else. `apim/holos-identity.xml` removes
  whatever identity headers the client sent, validates the token, and sets
  `X-Holos-Principal`, `X-Holos-Roles` and `X-Holos-Clearance` from its claims. Run the
  server with `--trust-forwarded-identity`, and let only APIM reach it (the chart's
  NetworkPolicy, or a private backend).

## Security baseline

What the templates and the server do by default:

| | |
| --- | --- |
| Process | Non-root (uid 10001), read-only root filesystem, every capability dropped, no privilege escalation, seccomp `RuntimeDefault`, no Kubernetes service account token |
| Network | NetworkPolicy on the server port. The NFS share reachable only from its subnet, no public access, no account keys. HTTPS-only ingress on Container Apps |
| Requests | Bodies over 256 MiB refused (`--max-body`). Queries abandoned after 300 s (`--timeout`) and refused past the memory ceiling (`--max-query-memory`) |
| Files | SPARQL `LOAD` refused unless `--load-dir` names a directory, and then confined to regular files inside it, after resolving symbolic links |
| Parsers | RDF/XML with more than 256 attributes on an element, or more than 4,096 namespace declarations, refused before parsing. This works around two quick-xml denial-of-service defects (RUSTSEC-2026-0194 and RUSTSEC-2026-0195) until the RDF/XML parser moves to a fixed quick-xml |
| Identity | Never read from headers unless a trust flag says so, and the templates turn one on only alongside an edge that sets the headers |

What is left to the operator:

- **Pin the image by digest**, and mirror it into Azure Container Registry. The proposal's
  pipeline does that.
- **NFS traffic is not encrypted in transit by default.** It stays inside the virtual
  network. Where Azure Files offers encryption in transit for NFS (the AZNFS mount helper),
  turn it on. NetApp Files supports Kerberos (`krb5p`) on NFS 4.1.
- Turn on Defender for Containers or an equivalent image scanner, and send the container
  logs to Log Analytics. The Bicep does the latter for Container Apps.

## What has been checked, and what has not

- The Helm chart is linted, rendered in each supported shape and validated against the
  Kubernetes 1.31 schemas (strict mode, kubeconform), here and by the `deploy-templates`
  CI job. The replica's checkpoint-switching script was run against a fake server in the
  runtime base image.
- The Bicep is compiled only by the CI job. It was not compiled on the machine these
  templates were written on.
- **Neither template has been deployed to an Azure subscription yet.** The first deployment
  is where resource API versions and regional availability get confirmed, Container Apps'
  NFS volume support in your region among them.
- The server's NFS behaviour (the lock check, read-only open, two servers sharing one
  checkpoint) is covered by the workspace's tests and was exercised on a local disk. It has
  not been run against an Azure NFS share.
