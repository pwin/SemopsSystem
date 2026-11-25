# 0012. Azure Storage URLs as inputs

Status: accepted and implemented, 25 November 2026, in `crates/semops/src/remote.rs`.

## Context

The proposal's Azure table says `semops map` and `semops load` should read `abfss://` and
`https://….blob.core.windows.net` URLs directly, through Rust's `object_store` crate, so
that tarka can map lakehouse tables in ADLS Gen2 or Fabric and the store can load dumps
kept in Blob Storage. Until now the manual's "not finished" chapter told people to mount
Blob Storage over NFS or copy the files in first.

The subcommands are the engines' own commands, linked in and handed their arguments
untouched (ADR 0008). Their file arguments are spread over many flags: `--data`,
`--query-file`, `--shapes`, `--rules` and the rest for `holos`, and clap options or
positionals for `shacl`, `tarka` and `semops check`.

## Decision

**`semops` fetches each blob first, then runs the command on the local copy.** Before the
arguments are parsed, every argument that names a blob, whole or as the value of
`--flag=URL`, is downloaded to a temporary file. The argument becomes that file's path. The
directory goes when the command returns. One layer covers every subcommand, and no engine's
argument parsing changes.

- **Only unambiguous storage URLs are fetched.** That means the `abfss`, `abfs`, `az`,
  `azure` and `adl` schemes, and `https` on `*.blob.core.windows.net` or
  `*.dfs.core.windows.net` with a path. A plain `http(s)` argument is never fetched,
  because `--base`, `--entail-graph` and named graphs take IRIs that look just like URLs.
- **The file keeps the blob's name.** So `.nt.gz`, `.ttl` or `.parquet` still picks the
  parser. Each URL gets its own subdirectory, since two blobs can share a name. The same
  URL given twice is downloaded once.
- **`object_store`'s Azure client does the work.** It parses the URL forms and reads
  credentials as Azure's own tools do: account key, SAS, service principal, workload
  identity, the Azure CLI, and with none of those set, a managed identity.
  `AZURE_SKIP_SIGNATURE=true` reads a public container. A SAS in the URL's query string is
  used, and is kept out of error messages.
- **The download streams to disk**, so a large dump is never held in memory.
- **`repair --write` refuses a URL.** It would write the repair to the temporary copy and
  lose it. `--out FILE` works.
- **The `holos`, `shacl` and `tarka` names don't fetch.** The binary run under an engine's
  name stays argument-for-argument that engine. Only `semops` adds the layer.

`object_store` 0.13, tokio and reqwest were already linked into `semops` through Polars,
so turning on the `azure` feature adds the Azure client and little else.

## Alternatives not taken

- **Teaching each engine to open URLs.** That means four argument parsers, four sets of
  tests, and the engines' own crates taking a cloud dependency that `shacl` and `holosdb`
  users don't want.
- **Polars' own cloud readers for `semops map`.** They cover the tables Polars reads, not
  RDF inputs, shapes or the mapping files themselves.
- **Streaming without a temporary file.** The parsers take paths, and HOLOS reads `.gz`
  dumps by extension. A temporary file costs disk space, not memory, and the manual says
  how to point `TMPDIR` at a disk with room.

## Tests

- **Unit tests** cover which arguments are rewritten and which are not, the `--flag=URL`
  form, one download for a repeated URL, removal of the files afterwards, and a SAS never
  appearing in an error.
- **`crates/semops/tests/blob.rs` runs the real binary and the real Azure client**, in
  emulator mode, against a stand-in Blob service on localhost. It checks that a query reads
  its data from a blob in one signed GET, that the blob's name decides the format, that
  `semops check` on an `https://….blob.core.windows.net` URL gives the same 64 findings as
  the local file, that an IRI argument is not fetched, and that `repair --write` refuses a
  URL.

## Not done

- **Writing to Blob Storage.** Not done here: [0013](0013-checkpoints-to-blob-storage.md)
  added `semops upload`, `semops download` and `semops backup --to <URL>`, and made output
  flags refuse a blob URL.
- **S3 and GCS.** `object_store` handles them, but the proposal asked for Azure, and every
  added scheme is another argument form to keep apart from IRIs.
- **`semops serve` loading its data from Blob at start-up.** That works through the same
  layer, because `semops serve --data az://…` is rewritten too. It is not tested, and a
  long-running server holding a temporary file is a choice to make deliberately.
