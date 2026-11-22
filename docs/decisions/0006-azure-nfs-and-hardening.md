# 0006. Azure, NFS storage, and a security pass

Status: accepted and implemented, 22 November 2026. The templates are in `deploy/azure/`;
the server and store changes are in `holos-server` and `holos-store`.

## Context

The proposal's Azure section sets the direction: the same image as everywhere else, AKS
with a managed disk for a writable store, Container Apps for read-only and short-lived
stores, Entra ID at the edge. It rules out Azure Files, SMB or NFS, for the live store,
because RocksDB needs low-latency block storage and a single writer. The requirement since
then adds NFS: the different ways of running on Azure with NFS storage, alongside a
minimum of security weaknesses and tests that run.

## Decision: what NFS is used for

The proposal's caution holds for the reason it gives, and NFS is still useful, because what
matters is whether the filesystem's locks reach the server:

- **A writable store on NFS is allowed where locks reach the server**: Azure NetApp Files
  (NFS 4.1, or NFS 3 with NLM) and Azure Files premium NFS 4.1. RocksDB's lock then refuses
  a second writer from any machine. NetApp Files is the recommended NFS home for a writer
  (sub-millisecond latency). Azure Files NFS is for small or read-mostly stores, since every
  synchronous write is a round trip.
- **It is refused where locks stay on the client.** Azure Blob Storage's NFS 3.0 endpoint
  must be mounted `nolock`, so every client grants itself the lock. `holos_store::fscheck`
  reads `/proc/self/mountinfo` on Linux, and `RocksStorage::open` refuses a store on an NFS
  mount carrying `nolock`, `local_lock=all` or `local_lock=posix`, unless
  `HOLOS_ALLOW_LOCAL_LOCKS=1` is set.
- **Every NFS service can serve read replicas**, through a new read-only open:
  `RocksStorage::open_read_only` and `holos-server --store-read-only`. RocksDB's read-only
  mode takes no lock; the store writes nothing to the directory (RocksDB's own log is sent
  to the temporary directory). The writer's checkpoints (`POST /backup`), which never change
  once written, are shared on an RWX volume, and any number of replicas open the newest one.
  A test opens one checkpoint twice at once, reads through both, sees a write refused, and
  checks the directory is unchanged; two servers sharing one store were run end to end.

The deployment patterns follow (`deploy/azure/README.md` has the full table): AKS with a
managed disk or NetApp Files for the writer, Azure Files NFS for checkpoints and replicas;
Container Apps in `snapshot`, `replica` or `writer` mode over an Azure Files NFS share.

## Decision: identity

- **Container Apps and App Service built-in authentication**: `--trust-azure-easy-auth`
  reads `X-MS-CLIENT-PRINCIPAL`, the platform's base64 JSON of the verified token's claims,
  into a `Principal` through `Principal::from_verified_claims`: the object id as subject,
  app roles and `groups` as roles, the rest as attributes. An unreadable header grants
  nothing.
- **API Management**: a policy (`deploy/azure/apim/holos-identity.xml`) strips the client's
  identity headers, validates the Entra ID token and sets HOLOS's existing
  `X-Holos-*` headers, for `--trust-forwarded-identity`.

## Decision: security fixes

A review of the server's request handling and a `cargo audit` of the lock file found these,
fixed here:

| Weakness | Fix |
| --- | --- |
| **Any client allowed to update could read any file the server can**, through `LOAD <file:…>`: into the store, as fragments in parse errors, or without end from `/dev/zero` | `holos-server` refuses every `LOAD` unless `--load-dir` names a directory, then admits only regular files inside it after resolving symbolic links. The engine's local behaviour, which the `holos` command relies on, is unchanged |
| **Request bodies were read into memory without limit**, before the query memory ceiling starts counting | `--max-body`, default 256 MiB, enforced on the declared length and, for a chunked body, by counting; 413 beyond it |
| **A panic killed a worker thread for good**: in an unwinding build, enough panicking requests left a server that accepted connections and answered none | Each request runs under `catch_unwind`. And one such panic: the percent-decoding of `LOAD` sources sliced a `&str` inside a multi-byte character; both copies now decode bytes |
| **libz-sys 1.1.29**: a heap buffer overflow (RUSTSEC-2026-0544) and a build-script environment leak (RUSTSEC-2026-0418) | Updated to 1.1.33 |
| **quick-xml 0.37.5 under the RDF/XML parser**: quadratic time in a start tag's attributes (RUSTSEC-2026-0194) and unbounded namespace declarations (RUSTSEC-2026-0195), both high, reachable by sending RDF/XML to the Graph Store, which parses before policy applies | Mitigated, not fixed: RDF/XML bodies with more than 256 attributes on an element or 4,096 namespace declarations are refused before parsing (`holos-server/src/inputs.rs`). See below |

## Not done, and why

- **The quick-xml fix itself.** The newest `oxrdfxml` (0.2.5) and `sparesults` (0.3.5) still
  require quick-xml 0.37. Oxigraph's development branch moved to 0.42 through three commits
  that are entangled with its move to a newer `oxrdf`, which this workspace does not use, and
  a direct port to 0.42 against the released crates fails in 72 places. The choice is to
  vendor and port both crates (the workspace already vendors `spareval` this way), or to wait
  for an Oxigraph release that carries the fix; the mitigation above holds until then.
  `sparesults`' XML reader is not reachable from a request: the server writes XML results
  and never parses them.
- **quick-xml 0.39.4 under `object_store`** (through Polars, in tarka), with the same two
  advisories. It parses cloud storage responses, so exploiting it needs a malicious storage
  endpoint. It moves with Polars.
- **Unmaintained or unsound transitive crates** that `cargo audit` reports as warnings, not
  vulnerabilities: `heapless` (through `rstar`, through `geo`), `bincode`, `chrono-tz`,
  `humantime`, `atomic-polyfill`. None has a compatible replacement to move to today.
- **Parser hardening Oxigraph made after its last release**: a limit on expanded XML entities
  and a nesting limit for triple terms. They arrive with the next Oxigraph release.
- **From the proposal's Azure table**: reading `abfss://` and Blob URLs directly
  (`object_store`), checkpoint upload to Blob with lifecycle rules, OpenTelemetry export to
  Azure Monitor, an `azd` template, and the JUnit and SARIF output for an Azure Pipelines
  task. NFS-mounted Blob Storage covers checkpoints and input files in the meantime.
- **Neither template has been deployed to a subscription.** The chart is linted and
  schema-checked, and the Bicep is compiled, in CI; deployment confirms API versions and
  regional availability.
