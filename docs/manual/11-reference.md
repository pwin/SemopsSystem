# Reference {#reference}

## The commands

`semops` is the one command. Each of the other four is also a `semops` subcommand, run by
the same code with the same arguments, and `semops` copied or linked under one of their
names is that command.

| `semops` | Is | Its own help |
|---|---|---|
| `semops serve` | `holos-server` | `semops serve --help` |
| `semops load`, `query`, `update`, `stats`, `dump`, `backup`, `compact`, `entail`, `rules` | `holos load`, `holos query` and the rest | `semops query --help` |
| `semops validate` | `shacl` | `semops validate --help` |
| `semops map` | `tarka` | `semops map --help`, `semops map run --help` |
| `semops check` | The ontology quality checks, `semops` only | `semops check --help` |
| `semops diff` | Version comparison: the suggested bump between two versions, `semops` only | `semops diff --help` |
| `semops repair` | The quality checks' repairs, applied to a file, `semops` only | `semops repair --help` |
| `semops upload`, `semops download` | Copies a file or a directory to and from Azure Blob Storage, `semops` only | `semops upload --help` |

| Command | From | What it is for | Its own help |
|---|---|---|---|
| `semops` | `crates/semops` | All of the below, and `semops check`: the ontology quality checks | `semops --help` |
| `holos-server` | `holos/crates/holos-server` | The SPARQL endpoint | `holos-server --help`, below |
| `holos` | `holos/crates/holos-cli` | Loading, queries, updates, validation, backups and maintenance on a store, without a server | `holos --help` |
| `shacl` | `shacl/crates/shacl-cli` | SHACL validation of files | `shacl --help` |
| `tarka` | `tarka/crates/tarka-cli` | Mapping tables to RDF | `tarka --help`, `tarka run --help` |

## Exit statuses

| Command | 0 | 1 | 2 |
|---|---|---|---|
| `semops check` | Nothing at or above `--fail-on` | Something was | The run could not be made, or a check did not execute |
| `shacl` | The data conforms | It does not | The command could not run |
| `holos-server` | Stopped cleanly | Could not start: a bad flag, a store it cannot open | |

## Environment variables

The service scripts in `holos/deploy/` (`run.sh`, `run.ps1`, `install-service.ps1`) read
`holos.env`, then `holos.env.local`, and turn these into `holos-server` flags. A variable
already set in the environment wins over both files, which is how a container platform or
Azure Key Vault reference supplies one.

| Variable | Flag | Default |
|---|---|---|
| `HOLOS_LISTEN` | `--listen` | `127.0.0.1:7878` |
| `HOLOS_THREADS` | `--threads` | `8` |
| `HOLOS_STORE` | `--store` | `./var/store` |
| `HOLOS_DATA` | `--data`, once per space-separated file | none |
| `HOLOS_READ_ONLY` | `--read-only` when `on` | `off` |
| `HOLOS_STORE_READ_ONLY` | `--store-read-only` when `on` | `off` |
| `HOLOS_REORDER` | `--reorder` when `on` | `off` |
| `HOLOS_TIMEOUT` | `--timeout` | `300` |
| `HOLOS_MAX_QUERY_MEMORY` | `--max-query-memory` | the server's, 8 |
| `HOLOS_MAX_BODY` | `--max-body` | the server's, 256 |
| `HOLOS_LOAD_DIR` | `--load-dir` | none: `LOAD` refused |
| `HOLOS_UI`, `HOLOS_UI_TILES` | `--no-ui` when `off`; `--ui-tiles` | `on`; the server's |
| `HOLOS_TRUST_FORWARDED` | `--trust-forwarded-identity` when `on` | `off` |
| `HOLOS_TRUST_AZURE_EASY_AUTH` | `--trust-azure-easy-auth` when `on` | `off` |
| `HOLOS_DEV_ROLES`, `HOLOS_DEV_CLEARANCE` | `--role`, `--clearance`: for development only | none |
| `HOLOS_DENY_ALL`, `HOLOS_FAIL_CLOSED` | `--deny-all`, `--fail-closed` when `on` | `off` |
| `HOLOS_ALLOW_GRAPHS`, `HOLOS_DENY_PREDICATES`, `HOLOS_LABEL_GRAPHS` | `--allow-graph`, `--deny-predicate`, `--label-graph` | none |
| `HOLOS_BACKUP_DIR`, `HOLOS_BACKUP_ROLE` | `--backup-dir`, `--backup-role` | none: no backup endpoint |
| `HOLOS_EXTRA_ARGS` | Anything else, passed as written | none |

`semops` reads Azure's own variables (`AZURE_STORAGE_ACCOUNT_KEY`, `AZURE_CLIENT_ID` and the
rest) when an argument is a Blob Storage URL. [Chapter 10](10-azure.md#reading-from-blob-storage)
lists them.

`holos-server` and `semops serve` read OpenTelemetry's own variables
(`OTEL_EXPORTER_OTLP_ENDPOINT` and the rest) to send traces and metrics.
[Chapter 10](10-azure.md#telemetry) lists them.

One more is read by the store itself, wherever it runs:

| Variable | Effect |
|---|---|
| `HOLOS_ALLOW_LOCAL_LOCKS=1` | Open a writable store on an NFS mount whose locks stay on the client, which is otherwise refused ([chapter 8](08-storage.md)) |

## holos-server {#holos-server}

As `holos-server --help` prints it, for version 0.31.0:

```text
holos-server — SPARQL 1.2 over HTTP, with a YASGUI console

USAGE
    holos-server [--listen <ADDR>] [--data <FILE>...] [--store <DIR>] [OPTIONS]

ENDPOINTS
    GET  /                   YASGUI console
    GET  /query?query=...    SPARQL 1.2 Protocol
    POST /query              SPARQL 1.2 Protocol (form-encoded or application/sparql-query)
    POST /update             SPARQL 1.1 Update (form-encoded or application/sparql-update)

GRAPH STORE PROTOCOL      https://www.w3.org/TR/sparql11-http-rdf-update/
    GET    /graph?graph=<IRI>    Serialise a graph (or ?default)
    HEAD   /graph?graph=<IRI>    As GET, without the body
    PUT    /graph?graph=<IRI>    Replace a graph with the request body
    POST   /graph?graph=<IRI>    Merge the request body into a graph
    DELETE /graph?graph=<IRI>    Remove a graph
    GET  /stats              Store statistics and the server's version, as JSON
    GET  /health             Liveness

PLACETIME                 (off unless --placetime or --placetime-base is given)
    GET  /id/<SET>/<REF>     303 See Other to the document describing it
    GET  /doc/<SET>/<REF>    The description, negotiated, or by extension: .ttl .nt .rdf
                             .jsonld (and .n3 .json for old links)

    The interval URI sets of reference.data.gov.uk, in its patterns: year/2006,
    quarter/2006-Q1, day/1752-09-14 (British: Julian until 2 September 1752),
    gregorian-month/2006-02, julian-day/1582-10-04, gregorian-instant/2006-01-01T00:00:00,
    gregorian-interval/2006-01-01T00:00:00/P3M; and WGS84 points,
    geopoint/wgs84/X-0.1276Y51.5072, described with GeoSPARQL. /id/<SET> is the set itself.
    Described in reference.data.gov.uk's vocabulary, or in gist with
    --placetime-vocabulary gist.

    Every response carries a Server header naming the build: Server: holos/0.13.0 (d4b4fdd).

SERVER
    --version, -V            Print the version, the commit it was built from, and whether
                             the tree was clean, then exit.
    --listen <ADDR>          Default 127.0.0.1:7878
    --data <FILE>            Load a file at start-up. Repeatable. Also reads .gz, streamed.
    --store <DIR>            Use a persistent RocksDB store at DIR.
    --threads <N>            Worker threads. Default 8.
    --load-dir <DIR>         Let SPARQL Update's LOAD read regular files inside DIR, and
                             nothing else. Without it every LOAD is refused: a client that
                             may update could otherwise read any file the server can.
    --max-body <MiB>         The largest request body accepted, answered 413 beyond it.
                             Default 256; 0 removes the ceiling. Bodies are read into memory
                             before the query memory ceiling starts counting, so this is what
                             stops one request from exhausting the process.
    --timeout <SECONDS>      Abandon a query after this long. Default 0 (no limit).
                             Enforced while a query reads or streams rows; a query blocked
                             inside one in-memory step is not interruptible. See
                             OPERATIONS.md.
    --spill-bytes <MiB>      Bytes a SELECT DISTINCT, a COUNT(DISTINCT *) or an ORDER BY
                             may hold before spilling a sorted run to disk. Default 128;
                             0 disables it, leaving the operator to buffer in memory.
                             Was --spill-distinct, which is still accepted.
    --max-spill-disk <GiB>   Scratch one query may write while spilling, before it is
                             refused. Default 16; 0 removes the ceiling. Scratch goes to
                             TMPDIR (TMP/TEMP on Windows), which is often the system
                             volume: see OPERATIONS.md before raising this.

                             Engaged only where --max-blocking-rows would otherwise refuse
                             the query, so it needs --reorder too. It is not the fast path
                             and is not meant to be: on three million rows it took 23.6s
                             against the evaluator's 2.2s, and 1,259 MiB against 396 MiB,
                             with the disk never touched. That is the price of an answer
                             where the alternative is no answer.

                             A spilled DISTINCT comes back sorted rather than in arrival
                             order, which SPARQL permits since DISTINCT promises no order.

    --max-blocking-rows <N>  Refuse a query whose ORDER BY, DISTINCT or keyed GROUP BY is
                             *estimated* to buffer more than N rows. Default 10,000,000;
                             0 disables it. Needs --reorder, since without statistics there
                             is no estimate and nothing is refused.

                             This declines in a millisecond what --max-query-memory would
                             otherwise stop several gigabytes and some minutes later. A
                             LIMIT rescues a DISTINCT and does not rescue an ORDER BY, and
                             the refusal says which case it is.

    --max-query-memory <GiB> Refuse reads once the process has grown this far past its
                             resting set -- what was resident after the store opened and
                             any --data finished loading. Default 8, 0 disables it.

                             Set it to a THIRD of the memory you can spare, at most. A
                             growing buffer asks for its next block before releasing the
                             old one, so a container crossing an 8 GiB ceiling wants 24 GiB
                             in hand at that instant. A ceiling set near the machine's limit
                             does not prevent the abort, it only chooses where it happens.

                             Without a ceiling, a SELECT DISTINCT or COUNT(DISTINCT *) over
                             a large store exhausts the machine and aborts the server,
                             taking every other request with it.
    --read-only              Answer 403 to /update and to every writing Graph Store
                             Protocol verb. The store is still opened writable, so a loader
                             can use it; this refuses writes over HTTP only.
    --store-read-only        Open the --store read-only, which implies --read-only: RocksDB
                             takes no lock and nothing is written to the directory, so any
                             number of servers can open the same store at once. For a
                             checkpoint (POST /backup) on a shared volume, such as an Azure
                             Files or NetApp Files NFS share, served by several replicas;
                             never for a store a writer is still changing.
    --gsp-path <PATH>        Where the Graph Store Protocol is served. Default /graph.
    --gsp-base <IRI>         Enable *direct* graph identification, treating the request path
                             under this base as the graph name. Off by default: a server
                             behind a proxy does not know its own external base, and
                             guessing would mint names that do not match what clients ask
                             for later.
    --reorder                Order each basic graph pattern by estimated cardinality before
                             evaluating, making query cost independent of how the query was
                             written. Statistics are built once at start-up and rebuilt after
                             an update. Measured 14x on a badly ordered join (BENCHMARKS.md).
    --no-ui                  Do not serve the console. The endpoints still work, and the
                             server then needs no network access of any kind.
    --ui-tiles <TEMPLATE>    The console map's basemap, as a Leaflet tile template. Default
                             https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png. The
                             console's Content-Security-Policy lets it reach this server,
                             the script CDN and this one tile host, and nothing else; but
                             which tiles a map asks for says where its user is looking, so
                             `none` draws geometries over a blank background and the page
                             then reaches nothing beyond the CDN and this server.

PLACETIME
    --placetime              Serve the placetime URI sets under https://<Host>/, the host
                             the client named. X-Forwarded-Host is read instead only with
                             --trust-forwarded-identity. The scheme is https whatever the
                             request came in on, since a front door that ends TLS hands this
                             server plain HTTP.
    --placetime-base <URL>   Serve them under this base instead, for every request: the URIs
                             are <URL>id/..., answered at the base's path on this server, so
                             --placetime-base https://example.org/ref/ answers /ref/id/....
    --placetime-vocabulary <data.gov.uk|gist>
                             The vocabulary the documents describe the URIs in (default
                             data.gov.uk): reference.data.gov.uk's, with OWL-Time, SCOVO and
                             WGS84, as the URI sets have always been described; or gist
                             14.1.0 with the placetime ontology built on it.

MAINTENANCE
    --backup-dir <DIR>       Parent directory for `POST /backup`. With --store it writes a
                             RocksDB checkpoint, holos-<seconds>/. In memory it writes a
                             snapshot, holos-<seconds>.nq.gz: every graph, which --data
                             reads back at the next start. The endpoint does not exist
                             unless this and --backup-role are both set: a client never
                             names a path, so it cannot be aimed at one.
    --backup-role <ROLE>     Role a principal must hold to trigger a backup.
    --purge-role <ROLE>      Role a principal must hold to call POST /maintenance/purge,
                             which reclaims spatial index entries for geometries no longer
                             referenced by any quad. There is no timer: schedule it with
                             cron or a systemd timer, as with backups.

IDENTITY  (see DESIGN.md §14.5)
    --trust-forwarded-identity
                             Read the principal from X-Holos-Principal, X-Holos-Roles and
                             X-Holos-Clearance. Only safe behind a front door that sets
                             them and strips whatever a client sent. Off by default.
    --trust-azure-easy-auth
                             Read the principal from X-MS-CLIENT-PRINCIPAL, which Azure
                             Container Apps and App Service built-in authentication set on
                             a request they have authenticated (Entra ID or another
                             provider): the object id is the subject, app roles and groups
                             are roles. Only safe where that authentication is required and
                             nothing reaches the server except through it. Off by default.
    --role <NAME>            Give every request this role. Repeatable. For development.
    --clearance <N>          Give every request this clearance level.

POLICY
    --deny-all               Deny by default instead of permit-all.
    --allow-graph <IRI>      Grant read on a named graph. Repeatable.
    --deny-predicate <IRI>   Refuse read on a predicate. Repeatable.
    --label-graph <IRI>=<N>  Classify a graph at level N. Repeatable.
    --fail-closed            Error on refusal rather than filtering silently.
```
