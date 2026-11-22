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
