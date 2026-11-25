# 0013. Checkpoints to Blob Storage

Status: accepted and implemented, 25 November 2026, in `crates/semops/src/remote.rs`,
`crates/semops/src/lib.rs` and the AKS chart.

## Context

The proposal's Azure table: "RocksDB checkpoints are uploaded to Blob Storage, extending the
existing `backup.sh`, with lifecycle rules for retention." [0012](0012-blob-urls-as-inputs.md)
made blobs readable as inputs and left writing out. Until now a checkpoint stayed on the
machine or on the cluster's shared volume. `holos/deploy/backup.sh` warns that a checkpoint
on the store's own filesystem is no off-machine backup.

Checkpoints are taken in two ways. Without a server, `holos backup --store DIR --to DIR`
takes one. With a server, the writer must take it, through `POST /backup`, because a second
process cannot open a store its writer holds. The chart's backup job does that, and it can
only work with the directory the writer leaves on the shared volume.

## Decision

**Writing is explicit.** It does not go through 0012's rewriting:

- **`semops upload PATH URL`** copies a file, or a directory tree with its paths, to a blob
  or prefix. Under a URL ending in `/` the source keeps its own name, as `cp -r` would.
  Otherwise the URL names it. Files stream from disk through `object_store`'s buffered
  writer: one Put Blob up to 10 MiB, staged blocks and a block list above that.
- **`semops download URL PATH`** does the reverse. A URL ending in `/`, or one that names
  no blob, is a prefix, and everything under it lands in a new directory. The destination
  must not exist, so a restore never mixes two checkpoints. A blob name that would climb
  out of it (`..`) is refused.
- **`semops backup --store DIR --to <blob URL>`** checkpoints the store beside itself and
  uploads it as `holos-<UTC time>/`, the name `backup.sh` gives one, under a URL ending in
  `/`. Then it removes the local copy, whether or not the upload worked. Beside the store
  means on the same filesystem, where RocksDB hard-links the files, so the checkpoint is
  instant and costs no extra disk.
- **The chart's backup job** runs `semops upload` on the newest checkpoint after `POST
  /backup`, when `backup.blob.url` is set. It signs in with AKS workload identity:
  `backup.blob.clientId` names a managed identity, the chart makes the `<release>-backup`
  service account that the identity's federated credential points at, and no key is stored
  in the cluster. The chart refuses a URL without a trailing `/` or without a client id.
- **Retention is a lifecycle rule on the storage account**, not a job.
  `deploy/azure/backup/lifecycle-policy.json` deletes `holos-backups/holos-*` 30 days
  after it was written. The chart's `backup.keep` still prunes the shared volume.

**Output flags never fetch.** Under 0012, `semops backup --to az://…` would have tried to
download the destination. Now the value of `--to`, `--out`, `--output` or `-o` is passed
through untouched. Given a blob URL, it is refused, with an error that names `semops upload`
and `semops backup --to`, rather than written to a local file named like the URL. `upload`
and `download` skip the rewriting entirely, because their URLs are what they transfer.

## Tests

`crates/semops/tests/blob_transfer.rs` runs the binary against an in-memory stand-in for the
Blob service that keeps what it is sent. The stand-in implements Put Blob, Put Block, Put
Block List, Get Blob, Get Blob Properties and List Blobs. The tests cover:

- **A directory round trip:** a 12 MiB file, which goes up as blocks, plus a nested file
  come back byte for byte, and a second download into the same directory is refused.
- **A single file:** it keeps its name under a prefix and downloads back to a file.
- **Backup and restore:** a loaded store is backed up with `backup --to az://backups/`, no
  local checkpoint is left behind, and the checkpoint downloaded from
  `backups/holos-<time>/` opens as a store that answers a query with the original data.

Unit tests cover the target naming, the timestamp (checked against known dates, a leap
day among them) and the output-flag refusal. The chart's new shape is rendered and
schema-checked in CI, together with the two combinations it must refuse.

## Not done

- **A Container Apps backup job.** The Container Apps template has no scheduled backup,
  with or without Blob Storage.
- **Point-in-time selection on restore.** `download` takes the prefix of one checkpoint.
  Listing what is there is `az storage blob list`, or the portal.
- **Upload resumption.** A failed upload is retried by running the job again, which uploads
  the whole checkpoint.
