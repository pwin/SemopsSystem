# Stores, backups and network storage {#storage}

A store is in memory unless you give it a directory. On disk it is a RocksDB database:
fast to read, safe across a crash, and written by one process at a time.

## In memory or on disk

| | In memory | On disk |
|---|---|---|
| How | No `--store`; files loaded with `--data` | `--store <DIR>` |
| Survives a restart | No | Yes |
| Load time | Every start | Once |
| Size limit | The machine's memory | The disk |
| Where it suits | Read-only endpoints, CI, tests | Anything that changes, anything large |

Load a large file into a store when the server starts, not over HTTP:

```sh
holos-server --store /var/lib/holos/store --data big.nq.gz
```

`--data` reads the file as it decompresses, so a large dump costs no more memory than a
small one. Small additions to a running server go through `POST /update` or the Graph
Store.

## One writer

RocksDB admits one writer per store, by taking a lock on a file in its directory. A second
`holos-server` or `holos` opening the same store is refused. Stop the server before you
run a command that writes to its store.

## Backups

Two ways, both consistent:

| How | When |
|---|---|
| `holos backup --store <DIR> --to <DIR>` | The server is stopped |
| `POST /backup`, with `--backup-dir` and `--backup-role` | The server is running, and keeps running |

Both write a RocksDB checkpoint: a complete store in a new directory, hard-linked where the
filesystem allows, so it costs little space until the live store moves on. A checkpoint
opens as a store in its own right. For an in-memory server, `POST /backup` writes an
N-Quads snapshot instead, which `--data` reads back.

The endpoint takes no path from the client: it writes under `--backup-dir`, and only for a
principal holding `--backup-role`, so it cannot be aimed at an arbitrary place.

## Read-only stores and replicas

`--store-read-only` opens a store with no lock and no writes. Any number of servers can
open one store that way at once, which is how one checkpoint on a shared volume serves
several read replicas:

```sh
holos-server --store /shared/checkpoints/holos-1763800000 --store-read-only
```

```{.output}
  store    /shared/checkpoints/holos-1763800000 — 1 quads
           opened read-only: no lock, no writes
```

Never open a store a writer is still changing this way. A checkpoint never changes once it
is written, which is what makes it safe to share.

## Network filesystems

A store can live on NFS, if the filesystem's locks reach the server, where every client
sees them. NFS 4.1 and NFS 3 with NLM do that: Azure NetApp Files, Azure Files premium NFS,
and most NFS servers. Every synchronous write then costs a network round trip, so expect
writes to be slower than on a local disk, and the server says at start-up that its store
is on a network filesystem.

Some NFS mounts keep their locks on the client (`nolock`, which Azure Blob Storage's NFS
endpoint requires). There, two machines could each believe they hold the only lock, and two
writers corrupt a store. On Linux the server reads the mount table and refuses to open a
writable store on such a mount:

```{.output}
refusing to open /mnt/blob/store as a writable store: it is on nfs mounted at /mnt/blob
with `nolock`, so the store's lock does not reach other machines and two writers could
corrupt it. …
```

A read-only store is fine on any NFS mount, Blob NFS included, because it takes no lock.
`HOLOS_ALLOW_LOCAL_LOCKS=1` overrides the refusal for a deployment that guarantees one
writer some other way.

| Filesystem | Writable store | Read-only store, checkpoints |
|---|---|---|
| Local disk, managed disk | Yes | Yes |
| NFS 4.1; NFS 3 with NLM | Yes, slower writes | Yes |
| NFS mounted `nolock` (Blob NFS) | Refused | Yes |
| SMB | Not supported | Untested |

[Running on Azure](10-azure.md) turns these rules into deployments.
