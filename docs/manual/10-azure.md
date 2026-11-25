# Running on Azure {#azure}

On Azure, SemOps System runs as the same container image it runs as anywhere else. What
Azure adds is a choice of host, a choice of storage, and an identity service at the edge.
`deploy/azure/` holds the templates; this chapter helps you choose.

## Choosing

| You need | Host | Storage |
|---|---|---|
| A writable store in production | AKS, the Helm chart's default | Premium SSD managed disk |
| A writable store that survives a node or zone failure without a disk detach | AKS | Azure NetApp Files, NFS 4.1 |
| A small or read-mostly writable store, with no cluster | Container Apps, `mode=writer` | Azure Files premium, NFS 4.1 |
| More query throughput than one server gives | AKS with `replicas.enabled`, or Container Apps `mode=replica` | Checkpoints on an NFS share, opened read-only |
| A published read-only endpoint that scales to zero | Container Apps, `mode=snapshot` | An N-Quads snapshot on Azure Files |

## NFS, and what each service can hold

The rule from [chapter 8](08-storage.md) decides it: a writable store needs a filesystem
whose locks reach the server.

| Service | Locks reach the server | Writable store | Checkpoints and replicas |
|---|---|---|---|
| Azure NetApp Files, NFS 4.1 or NFS 3 | Yes | Yes, the best NFS choice: sub-millisecond latency | Yes |
| Azure Files premium, NFS 4.1 | Yes | Yes, for small or read-mostly stores: each write is a round trip | Yes |
| Azure Blob Storage, NFS 3.0 | No: it is mounted `nolock` | Refused by the server | Yes |
| Azure Files, SMB | (SMB's own) | Not supported | Untested |

## AKS

```sh
# One writer on a Premium SSD managed disk.
helm install holos deploy/azure/aks/holos -n semops --create-namespace

# The writer on Azure NetApp Files, through a StorageClass you have made for it.
helm install holos deploy/azure/aks/holos -n semops \
  --set store.storageClassName=azure-netapp-files-premium

# A writer, nightly checkpoints on Azure Files NFS, three read replicas.
helm install holos deploy/azure/aks/holos -n semops \
  --set shared.enabled=true \
  --set backup.enabled=true --set identity.mode=forwarded \
  --set replicas.enabled=true --set replicas.count=3
```

The writer is a StatefulSet fixed at one replica, so two writers never hold the store.
Each replica opens the newest checkpoint read-only, looks for a newer one every five
minutes, and restarts onto it after a random delay so the replicas do not all restart at
once. Pods run as a non-root user with a read-only root filesystem and no Linux
capabilities, and a NetworkPolicy admits only the ingress controller and the release's
own backup job.

## Container Apps

```sh
az group create -n semops -l westeurope
az deployment group create -g semops -f deploy/azure/containerapps/main.bicep \
  -p namePrefix=semops mode=replica entraClientId=<application id>
```

The repository is also an `azd` template for the same deployment: `azd up` from its root
asks for a subscription, a region and a resource group, then provisions it.
`deploy/azure/README.md` lists the environment values it reads, such as `SEMOPS_MODE` and
`ENTRA_CLIENT_ID`.

The template builds a virtual network, a premium storage account with an NFS 4.1 share
that only the Container Apps subnet can reach (no public access, account keys turned off),
the environment, and the app with HTTPS-only ingress. The share starts empty and owned by
root, and the server runs as uid 10001, so prepare it once from a VM in the network:

```sh
sudo mount -t nfs -o vers=4,minorversion=1,sec=sys <account>.file.core.windows.net:/<account>/holos /mnt/holos
sudo mkdir -p /mnt/holos/store /mnt/holos/checkpoints /mnt/holos/snapshots
sudo chown -R 10001:10001 /mnt/holos
```

## Blob Storage

### Reading from Blob Storage

Every `semops` subcommand takes a blob wherever it takes an input file:

```sh
semops load --store db --data abfss://data@acct.dfs.core.windows.net/dumps/2026-11.nt.gz
semops check https://acct.blob.core.windows.net/ontologies/domain.ttl --format sarif
semops map run --query sales.tarql --input az://lake/sales/2026.parquet
```

Before the subcommand runs, each argument that names a blob is downloaded to a temporary
file, and the argument becomes that file's path. The file keeps the blob's name, so the
format is still taken from its extension. It is removed when the command finishes.

Only arguments that can only be storage are fetched: the `abfss`, `abfs`, `az`, `azure` and
`adl` schemes, and `https` on `*.blob.core.windows.net` or `*.dfs.core.windows.net`. A
plain `http(s)` IRI is left as typed, because named graphs and base IRIs look just like
one.

Credentials come from the environment:

| Set | To use |
|---|---|
| `AZURE_STORAGE_ACCOUNT_KEY` (and `AZURE_STORAGE_ACCOUNT_NAME` for `az://`) | The account key |
| `AZURE_STORAGE_SAS_KEY`, or a SAS in the URL (`…?sv=…&sig=…`) | A SAS token |
| `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID` | A service principal |
| `AZURE_FEDERATED_TOKEN_FILE`, as AKS workload identity sets it | Workload identity |
| `AZURE_USE_AZURE_CLI=true` | Your `az login` |
| Nothing | The managed identity of the VM, container app or pod |
| `AZURE_SKIP_SIGNATURE=true` | A public container, with no credentials |

The download goes to the system temporary directory (`TMPDIR`, or `TMP` on Windows). For a
large dump, point that at a disk with room.

### Writing to Blob Storage, and backups

A command never writes to a URL. An output flag (`--to`, `--out`, `--output`, `-o`) given a
blob URL is refused, so a report or a store is written locally and then copied up:

```sh
semops upload report.sarif https://acct.blob.core.windows.net/reports/ci/
semops download https://acct.blob.core.windows.net/reports/ci/report.sarif ./report.sarif
```

Under a URL ending in `/` the file or directory keeps its own name, as `cp -r` would.
Otherwise the URL names it. A directory's files go up with their paths, and a large file
goes up in blocks. `download` of a URL ending in `/` (or one that names no blob) fetches
everything under that prefix into a new directory.

`semops backup --to` takes a blob URL, which is the one exception:

```sh
semops backup --store /var/lib/holos/store --to https://acct.blob.core.windows.net/holos-backups/
```

The checkpoint is taken beside the store, uploaded as `holos-<UTC time>/`, and removed.
Beside the store means on the same filesystem, so it costs no extra disk. On AKS the
writer takes checkpoints itself, and the chart's backup job uploads each one when
`backup.blob.url` is set, signing in with workload identity (`deploy/azure/README.md`).
Retention is a lifecycle rule on the storage account (`deploy/azure/backup/`).

To restore, download a checkpoint into a new directory and open it as a store:

```sh
semops download https://acct.blob.core.windows.net/holos-backups/holos-20261125T020000Z/ ./store
semops serve --store ./store
```

## Entra ID

The server does not check tokens. Two Azure services do it for it:

- **Container Apps built-in authentication.** With `entraClientId` set, the template turns
  it on and refuses every request without a valid token for that application. The server
  starts with `--trust-azure-easy-auth` and takes the user's object id, app roles and
  groups from the header the platform sets.
- **API Management**, in front of AKS or anything else. The policy in
  `deploy/azure/apim/holos-identity.xml` validates the token, removes any identity headers
  the client sent, and sets HOLOS's three headers from the token's claims. The server
  starts with `--trust-forwarded-identity`.

## What has and has not been tried

The Helm chart is linted, rendered in every shape it supports, and checked against the
Kubernetes schemas, both locally and in CI. The Bicep template is compiled in CI. Neither
has been deployed to an Azure subscription yet: the first deployment is where resource
versions and regional availability are confirmed. `deploy/azure/README.md` has the full
guide, including the security baseline the templates set.
