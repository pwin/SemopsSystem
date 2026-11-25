//! `holos-server` — SPARQL 1.2 Protocol over HTTP, with a YASGUI console.
//!
//! HOLOS L6 (`DESIGN.md` §10). Four things worth knowing about how this is built:
//!
//! **It authenticates nobody.** `DESIGN.md` §14.5 puts token verification, Kerberos, mTLS
//! and SAML at the edge, and this server is behind that edge. It reads *already-verified*
//! claims from headers a front door is expected to set — and refuses to do so at all
//! unless started with `--trust-forwarded-identity`, because trusting those headers on an
//! open port would let any client name its own roles.
//!
//! **Every request opens a session.** There is no path from a request to the data that
//! does not go through a [`Session`], so the policy chokepoint of §14 covers HTTP for free.
//!
//! **Reads and writes are separated by an `RwLock`.** The store is single-writer and
//! many-readers, so a thread-per-request model matches it exactly.
//!
//! **SPARQL Update takes the write lock.** `POST /update` is the only endpoint that does.
//! A writer excludes readers for the update's duration, which is what makes the update's
//! failure-atomicity also behave as isolation in this deployment.

mod easyauth;
mod gsp;
mod http;
mod inputs;
mod placetime;
mod problem;
mod telemetry;
mod ui;

use anyhow::{Context, Result};
use holos_engine::{update as sparql_update, Engine, QueryOptions};
use holos_security::{Label, Modes, Policy, Principal, PrincipalMatch, Rule, Scope, Session};
use oxrdf::NamedNode;
use oxrdfio::{RdfFormat, RdfSerializer};
use sparesults::QueryResultsSerializer;
use spareval::QueryResults;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tiny_http::{Header, Request, Response, Server};

/// What this binary was built from, for `--version`, the banner, the `Server` header and
/// `/stats`. See `holos_build`.
const BUILD: holos_build::Build = holos_build::build!();

/// The default ceiling on a request body: 256 MiB.
const DEFAULT_MAX_BODY: usize = 256 * 1024 * 1024;

const USAGE: &str = "\
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
";

struct Config {
    listen: String,
    timeout: Option<Duration>,
    /// Bytes one query may allocate before it is cancelled. `None` disables the ceiling.
    max_query_memory: Option<usize>,
    /// Rows a blocking operator may be estimated to buffer. Needs `--reorder` to apply.
    max_blocking_rows: Option<u64>,
    /// Bytes a DISTINCT may hold before spilling a sorted run. `None` leaves it to spareval.
    spill_bytes: Option<usize>,
    /// Scratch one query may write while spilling. `None` keeps the engine's own default.
    max_spill_disk: Option<usize>,
    read_only: bool,
    /// Open the store itself read-only: no lock, no writes, shareable by several servers.
    store_read_only: bool,
    reorder: bool,
    gsp_base: Option<String>,
    gsp_path: String,
    /// The largest request body accepted, in bytes. `None` accepts any size.
    max_body: Option<usize>,
    /// The directory `LOAD <file:…>` may read from. `None` refuses every `LOAD`.
    load_dir: Option<std::path::PathBuf>,
    data: Vec<String>,
    store: Option<String>,
    /// Where `POST /backup` writes checkpoints. `None` disables the endpoint entirely.
    backup_dir: Option<String>,
    /// The role a principal must hold to trigger a backup. `None` disables the endpoint.
    backup_role: Option<String>,
    /// The role a principal must hold to purge the spatial index. `None` disables the
    /// endpoint, on the same reasoning as `backup_role`.
    purge_role: Option<String>,
    threads: usize,
    ui: bool,
    /// The console map's tile template; `None` draws over a blank background. See `ui`.
    ui_tiles: Option<String>,
    trust_forwarded: bool,
    /// Read the principal from Azure built-in authentication's `X-MS-CLIENT-PRINCIPAL`.
    trust_easy_auth: bool,
    /// Where the placetime URI sets' base comes from; `None` serves none of them.
    placetime: Option<placetime::Mode>,
    /// The vocabulary placetime documents are written in.
    placetime_vocabulary: semops_placetime::Vocabulary,
    roles: Vec<String>,
    clearance: Option<u16>,
    deny_all: bool,
    allow_graphs: Vec<String>,
    deny_predicates: Vec<String>,
    graph_labels: Vec<(String, u16)>,
    fail_closed: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:7878".to_owned(),
            timeout: None,
            // A query that needs more than this is almost always an accident — a
            // `SELECT DISTINCT` or `COUNT(DISTINCT *)` over a whole store, an unbounded
            // `ORDER BY` — and the accident it causes without a ceiling is not a failed
            // request but a dead process, taking every other client's work with it. The
            // figure is deliberately generous and deliberately not derived from the
            // machine's RAM, which this server does not read: it is a backstop against
            // absurdity, not a scheduler. `--max-query-memory 0` removes it.
            max_query_memory: Some(8 * 1024 * 1024 * 1024),
            max_blocking_rows: Some(holos_engine::admit::DEFAULT_BLOCKING_ROWS),
            // On by default: a DISTINCT that cannot finish is the failure this server was
            // taken down by, and a sort costs `n log n` against a hash set's `n` only when
            // the answer was small enough not to matter.
            spill_bytes: Some(holos_engine::spill::SPILL_BYTES),
            max_spill_disk: None,
            read_only: false,
            store_read_only: false,
            reorder: false,
            gsp_base: None,
            gsp_path: "/graph".to_owned(),
            // A body is read into memory before anything looks at it, and the query memory
            // ceiling only starts counting once evaluation does, so without this one request
            // streaming gigabytes at an open port exhausts the process. Generous for a query,
            // an update or a graph; a load larger than this belongs to `--data` or `holos
            // load`, which stream.
            max_body: Some(DEFAULT_MAX_BODY),
            load_dir: None,
            data: Vec::new(),
            store: None,
            backup_dir: None,
            backup_role: None,
            purge_role: None,
            threads: 8,
            ui: true,
            ui_tiles: Some(ui::DEFAULT_TILES.to_owned()),
            trust_forwarded: false,
            trust_easy_auth: false,
            placetime: None,
            placetime_vocabulary: semops_placetime::Vocabulary::DataGovUk,
            roles: Vec::new(),
            clearance: None,
            deny_all: false,
            allow_graphs: Vec::new(),
            deny_predicates: Vec::new(),
            graph_labels: Vec::new(),
            fail_closed: false,
        }
    }
}

struct State {
    engine: RwLock<Engine>,
    policy: Policy,
    config: Config,
    /// Cardinality statistics, when `--reorder` is on.
    ///
    /// A snapshot, replaced after an update. Reading a stale snapshot makes a plan worse,
    /// never wrong — reordering a basic graph pattern cannot change its answer — so a
    /// reader never has to wait for a rebuild.
    statistics: RwLock<Option<Arc<holos_stats::Statistics>>>,
    /// The spatial index, rebuilt on every write alongside the statistics.
    ///
    /// Rebuilding rather than maintaining incrementally, for now: it is the same shape as
    /// the statistics beside it, and it is what keeps the index *current*, which is the
    /// condition the query path checks before it will use one at all.
    spatial: RwLock<Option<Arc<holos_engine::spatial::SpatialIndex>>>,
}

impl State {
    /// Rebuilds the statistics snapshot, if reordering is on.
    /// Returns whether the statistics were rebuilt, as opposed to loaded from the snapshot
    /// the store keeps. `false` also covers "reordering is off" and "it failed".
    fn refresh_statistics(&self) -> bool {
        if !self.config.reorder {
            return false;
        }
        // Under the read lock, so queries keep flowing during a scan that takes minutes
        // on a large store. A current snapshot skips the scan entirely — the ordinary case
        // at startup, and the reason a restart no longer costs a statistics pass.
        let built = {
            let Ok(engine) = self.engine.read() else {
                return false;
            };
            let graph = holos_store::GraphFilter::Default;
            match holos_stats::Statistics::load_cached(engine.store(), graph) {
                Ok(Some(stats)) => Ok((stats, false)),
                Ok(None) => {
                    holos_stats::Statistics::build(engine.store(), graph).map(|stats| (stats, true))
                }
                Err(e) => Err(e),
            }
        };
        match built {
            Ok((stats, fresh)) => {
                // A read-only store keeps nothing, so the snapshot lives in memory only.
                if fresh && !self.config.store_read_only {
                    // The write lock, briefly, and only to save. `save` re-checks the
                    // generation, so a write that slipped in between the read lock going
                    // and this one coming leaves the snapshot unsaved rather than wrong.
                    if let Ok(mut engine) = self.engine.write() {
                        if let Err(e) = stats.save(engine.store_mut()) {
                            eprintln!("statistics could not be kept with the store: {e}");
                        }
                    }
                }
                if let Ok(mut slot) = self.statistics.write() {
                    *slot = Some(Arc::new(stats));
                }
                fresh
            }
            // Losing the statistics costs speed, not correctness, so a failure here is
            // reported and the server carries on without them.
            Err(e) => {
                eprintln!("statistics could not be rebuilt: {e}");
                false
            }
        }
    }

    fn statistics(&self) -> Option<Arc<holos_stats::Statistics>> {
        self.statistics.read().ok().and_then(|s| s.clone())
    }

    /// Brings the spatial index up to date with the store.
    ///
    /// Called wherever the statistics are refreshed, and for the same reason: a write has
    /// happened, so anything derived from the store is out of date. An index that is not
    /// refreshed is not *wrong* — the query path notices it no longer describes the store and
    /// does the full scan instead — but it stops narrowing anything, so refreshing is what
    /// makes it worth having.
    ///
    /// Updates the existing index rather than replacing it. A rebuild re-decodes and
    /// re-parses every geometry in the store, which is 57% of its cost and all of it wasted
    /// on geometries that have not changed; a refresh pays that only for terms it has not
    /// seen before. The first call, when there is no index yet, still builds one.
    fn refresh_spatial(&self) {
        let Ok(engine) = self.engine.read() else {
            return;
        };
        let existing = self.spatial.read().ok().and_then(|slot| slot.clone());
        let outcome = match existing {
            Some(index) => index.refresh(engine.store()),
            None => holos_engine::spatial::SpatialIndex::build(engine.store()).map(|built| {
                if let Ok(mut slot) = self.spatial.write() {
                    *slot = Some(Arc::new(built));
                }
            }),
        };
        // Losing the index costs speed, not correctness: without one, topology relations are
        // evaluated by scanning, which is what they did before it existed.
        if let Err(e) = outcome {
            eprintln!("the spatial index could not be brought up to date: {e}");
        }
    }

    fn spatial(&self) -> Option<Arc<holos_engine::spatial::SpatialIndex>> {
        self.spatial.read().ok().and_then(|s| s.clone())
    }
}

/// The server, given its arguments as the operating system passed them, program name
/// first. `holos-server`'s own binary calls this, and so does `semops serve`.
///
/// The caller installs the counting allocator (`src/alloc.rs`) and calls
/// [`holos_engine::memory::mark_tracking`] first. A binary that does not leaves
/// `--max-query-memory` with nothing to count, and the ceiling is not enforced.
pub fn main_with(args: impl IntoIterator<Item = std::ffi::OsString>) -> Result<()> {
    let args: Vec<String> = args
        .into_iter()
        .skip(1)
        .map(|arg| {
            arg.into_string().map_err(|arg| {
                anyhow::anyhow!("an argument is not UTF-8: {}", arg.to_string_lossy())
            })
        })
        .collect::<Result<_>>()?;
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return Ok(());
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("holos-server {BUILD}");
        return Ok(());
    }
    let config = parse_args(&args)?;

    let mut engine = open_engine(&config)?;
    // Named and counted before anything else happens, because a server pointed at the wrong
    // directory serves an empty database with perfect composure. That happened: a store was
    // loaded into one path with the CLI, `deploy/run.sh` started the server on its default
    // `./var/store`, and every query came back empty against 653 million triples sitting a
    // volume away. Ten lines of startup output and none of them said where the data was.
    //
    // `len` is `META_QUADS` on RocksDB, one read, so this costs nothing on a large store.
    match &config.store {
        Some(path) => {
            let quads = engine.store().len();
            if quads == 0 && config.data.is_empty() {
                eprintln!(
                    "  store    {path} — empty. Nothing has been loaded here; if that is a \
surprise, check that this is the directory the load wrote to."
                );
            } else {
                eprintln!("  store    {path} — {quads} quads");
            }
            if config.store_read_only {
                eprintln!("           opened read-only: no lock, no writes");
            }
            // A network filesystem is a supported place for a store, and a slower one: every
            // synchronous write is a round trip, and only the lock keeps a second writer out.
            if let holos_store::fscheck::Locking::Shared {
                fstype,
                mount_point,
            } = holos_store::fscheck::inspect(std::path::Path::new(path))
            {
                if is_network_fs(&fstype) {
                    eprintln!(
                        "           on {fstype} at {}, a network filesystem: one writer only, \
                         and writes cost a round trip each (see deploy/azure/README.md)",
                        mount_point.display()
                    );
                }
            }
        }
        None => eprintln!("  store    in memory (no --store); nothing persists past exit"),
    }
    for path in &config.data {
        let format = format_for(path)?;
        let reader = holos_engine::source::reader(std::path::Path::new(path))?;
        let n = engine.bulk_load(reader, format, None)?;
        eprintln!("loaded {n} quads from {path}");
    }

    let policy = build_policy(&config)?;
    let listen = config.listen.clone();
    let threads = config.threads;
    let ui_enabled = config.ui;
    let state = Arc::new(State {
        engine: RwLock::new(engine),
        policy,
        config,
        statistics: RwLock::new(None),
        spatial: RwLock::new(None),
    });
    // Built unconditionally, and deliberately not inside the `--reorder` block below. The
    // two were coupled, which meant a server started without `--reorder` — the default — had
    // no spatial index at all until its first write, and every GeoSPARQL query until then
    // did a full scan. Reordering and spatial routing are unrelated features that happened
    // to be refreshed together.
    state.refresh_spatial();
    if state.config.reorder {
        let started = std::time::Instant::now();
        let built = state.refresh_statistics();
        eprintln!(
            "  reorder  statistics {} in {:.2}s",
            if built {
                "built and kept with the store"
            } else {
                "loaded from the store"
            },
            started.elapsed().as_secs_f64()
        );
    }

    // Armed here, and not a line earlier: everything above this is the resting set — the
    // store, whatever `--data` loaded, the statistics and the spatial index — and a ceiling
    // measured from zero would count all of it against the first query.
    if let Some(limit) = state.config.max_query_memory {
        holos_engine::memory::set_ceiling(limit);
    }

    let server = Arc::new(Server::http(&listen).map_err(|e| anyhow::anyhow!("{e}"))?);
    eprintln!("holos-server {BUILD} listening on http://{listen}");
    if ui_enabled {
        eprintln!("  console  http://{listen}/");
    }
    eprintln!("  query    http://{listen}/query");
    let vocabulary = state.config.placetime_vocabulary;
    match &state.config.placetime {
        Some(placetime::Mode::Host) => {
            eprintln!("  placetime https://<Host>/id/... (303 to /doc/...), in {vocabulary}");
        }
        Some(placetime::Mode::Fixed(base)) => {
            eprintln!(
                "  placetime {base}id/... answered at {}id/..., in {vocabulary}",
                base.path()
            );
        }
        None => {}
    }
    match state.config.max_query_memory {
        Some(limit) => eprintln!(
            "  memory   queries are refused once the process grows {} past its resting set              (--max-query-memory GiB, 0 to disable)",
            holos_engine::memory::human(limit)
        ),
        // Worth saying out loud rather than leaving to the flag reference: without a
        // ceiling a single query can abort the process and take every other client's
        // in-flight work with it.
        None => eprintln!(
            "  memory   NO ceiling; one query can exhaust the machine and abort the server"
        ),
    }
    if state.config.trust_easy_auth {
        eprintln!("  identity from Azure built-in authentication (X-MS-CLIENT-PRINCIPAL)");
    } else if !state.config.trust_forwarded {
        eprintln!(
            "  identity forwarded headers are NOT trusted; every request is anonymous \
             (--trust-forwarded-identity or --trust-azure-easy-auth to change)"
        );
    }

    let telemetry = Arc::new(telemetry::Telemetry::from_env());
    if let Some(line) = telemetry.describe() {
        eprintln!("  telemetry {line}");
    }

    let mut workers = Vec::with_capacity(threads);
    for _ in 0..threads {
        let server = Arc::clone(&server);
        let state = Arc::clone(&state);
        let telemetry = Arc::clone(&telemetry);
        workers.push(std::thread::spawn(move || {
            // A worker that dies takes its thread with it, not the server; a panic while
            // answering one request must not close the port for everyone else.
            //
            // `catch_unwind` is what makes that true in an unwinding build: without it the
            // panic ends the thread, and once every worker has met one the port still
            // accepts connections that nothing answers. (The container image builds with
            // `panic = "abort"`, where a panic restarts the process instead.)
            while let Ok(request) = server.recv() {
                let in_flight = telemetry.start(
                    request.method().as_str(),
                    route_of(&state, request.method().as_str(), request.url()),
                );
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    dispatch(&state, request)
                }));
                telemetry.finish(in_flight);
                match outcome {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => eprintln!("request failed: {e:#}"),
                    Err(_) => eprintln!("a request panicked; its connection was dropped"),
                }
            }
        }));
    }
    for worker in workers {
        let _ = worker.join();
    }
    Ok(())
}

/// The route a request matches, for telemetry: one of a fixed set, so a client cannot make
/// the metrics' cardinality grow, and `None` for a path no route serves.
fn route_of(state: &State, method: &str, url: &str) -> Option<std::borrow::Cow<'static, str>> {
    use std::borrow::Cow;
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let fixed = [
        "/",
        "/health",
        "/query",
        "/update",
        "/stats",
        "/backup",
        "/maintenance/purge",
        "/ui/console.css",
        ui::SCRIPT_PATH,
    ];
    if let Some(route) = fixed.into_iter().find(|r| *r == path) {
        return Some(Cow::Borrowed(route));
    }
    if method == "OPTIONS" {
        return Some(Cow::Borrowed("*"));
    }
    if is_graph_store_path(state, path) {
        let base = &state.config.gsp_path;
        return Some(if path == base {
            Cow::Owned(base.clone())
        } else {
            Cow::Owned(format!("{}/{{graph}}", base.trim_end_matches('/')))
        });
    }
    if state
        .config
        .placetime
        .as_ref()
        .is_some_and(|m| m.relative(path).is_some())
    {
        return Some(Cow::Borrowed("{placetime}"));
    }
    None
}

fn dispatch(state: &State, mut request: Request) -> Result<()> {
    let (path, params) = http::split_url(request.url());
    // Kept in its encoded form as well: a duplicate parameter has to be spotted before the
    // parser collapses it, and the protocol makes a duplicated `query` a client error.
    let query_string = request
        .url()
        .split_once('?')
        .map_or_else(String::new, |(_, q)| q.to_owned());
    let accept = header(&request, "accept");
    let content_type = header(&request, "content-type");

    match (request.method().as_str(), path.as_str()) {
        ("GET", "/health") => respond(request, 200, "text/plain", b"ok".to_vec()),
        // The console: a page whose policy names every origin it may reach, its script
        // and stylesheet served from here so the policy needs nothing inline. See `ui`.
        ("GET", "/") if state.config.ui => {
            let policy = ui::csp(state.config.ui_tiles.as_deref());
            respond_with(
                request,
                200,
                "text/html; charset=utf-8",
                ui::page("HOLOS").into_bytes(),
                &[
                    ("Content-Security-Policy", policy.as_str()),
                    ("Referrer-Policy", "no-referrer"),
                    ("X-Content-Type-Options", "nosniff"),
                ],
            )
        }
        ("GET", ui::SCRIPT_PATH) if state.config.ui => {
            let script = ui::script("/query", state.config.ui_tiles.as_deref());
            respond_with(
                request,
                200,
                "text/javascript; charset=utf-8",
                script.into_bytes(),
                &[("X-Content-Type-Options", "nosniff")],
            )
        }
        ("GET", "/ui/console.css") if state.config.ui => respond(
            request,
            200,
            "text/css; charset=utf-8",
            ui::stylesheet().as_bytes().to_vec(),
        ),
        ("GET", "/stats") => {
            let body = stats(state);
            respond(request, 200, "application/json", body.into_bytes())
        }
        ("GET", "/query") => {
            if let Some(why) = repeated(&query_string, "query") {
                return problem::bad_request(request, &why, accept.as_deref());
            }
            let Some(query) = params.get("query") else {
                return problem::bad_request(
                    request,
                    "missing the 'query' parameter",
                    accept.as_deref(),
                );
            };
            answer(state, request, &query.clone(), accept.as_deref(), &params)
        }
        ("POST", "/query") => {
            if let Some(why) = repeated(&query_string, "query") {
                return problem::bad_request(request, &why, accept.as_deref());
            }
            if let Some(why) = unusable_body(content_type.as_deref()) {
                return problem::bad_request(request, &why, accept.as_deref());
            }
            let body = match read_text_body(&mut request, state.config.max_body)? {
                Ok(body) => body,
                Err(refusal) => return refusal.send(request, accept.as_deref()),
            };
            // Two POST encodings are allowed: the query directly, or form-encoded.
            let query = if content_type
                .as_deref()
                .is_some_and(|c| c.starts_with("application/sparql-query"))
            {
                body
            } else {
                if let Some(why) = repeated(&body, "query") {
                    return problem::bad_request(request, &why, accept.as_deref());
                }
                let form = http::parse_form(&body);
                match form.get("query") {
                    Some(q) => q.clone(),
                    None => {
                        return problem::bad_request(
                            request,
                            "missing the 'query' parameter",
                            accept.as_deref(),
                        )
                    }
                }
            };
            answer(state, request, &query, accept.as_deref(), &params)
        }
        ("POST", "/update") => {
            if state.config.read_only {
                return problem::send(
                    request,
                    "read-only",
                    403,
                    "this endpoint is read-only: /update and the writing Graph Store verbs \
                     answer 403 while --read-only is set",
                    &[],
                    accept.as_deref(),
                );
            }
            if let Some(why) = repeated(&query_string, "update") {
                return problem::bad_request(request, &why, accept.as_deref());
            }
            if let Some(why) = unusable_body(content_type.as_deref()) {
                return problem::bad_request(request, &why, accept.as_deref());
            }
            let body = match read_text_body(&mut request, state.config.max_body)? {
                Ok(body) => body,
                Err(refusal) => return refusal.send(request, accept.as_deref()),
            };
            let (update, form) = if content_type
                .as_deref()
                .is_some_and(|c| c.starts_with("application/sparql-update"))
            {
                (body, std::collections::HashMap::new())
            } else {
                if let Some(why) = repeated(&body, "update") {
                    return problem::bad_request(request, &why, accept.as_deref());
                }
                let form = http::parse_form(&body);
                match form.get("update") {
                    Some(u) => (u.clone(), form),
                    None => {
                        return problem::bad_request(
                            request,
                            "missing the 'update' parameter",
                            accept.as_deref(),
                        )
                    }
                }
            };
            // The protocol names the update's dataset with `using-graph-uri`, in the query
            // string or the form body.
            let mut merged = params;
            for (k, v) in form {
                merged.entry(k).or_insert(v);
            }
            apply_update(state, request, &update, &merged, accept.as_deref())
        }
        ("POST", "/backup") => backup(state, request),
        ("POST", "/maintenance/purge") => purge(state, request),
        ("OPTIONS", _) => respond(request, 204, "text/plain", Vec::new()),
        (method, p)
            if matches!(method, "GET" | "HEAD" | "PUT" | "POST" | "DELETE")
                && is_graph_store_path(state, p) =>
        {
            graph_store(state, request, &params)
        }
        (method, p)
            if matches!(method, "GET" | "HEAD")
                && state
                    .config
                    .placetime
                    .as_ref()
                    .is_some_and(|m| m.relative(p).is_some()) =>
        {
            serve_placetime(state, request, &path, accept.as_deref())
        }
        _ => respond(request, 404, "text/plain", b"not found".to_vec()),
    }
}

/// Runs one query under a session built from the request.
fn answer(
    state: &State,
    request: Request,
    query: &str,
    accept: Option<&str>,
    params: &std::collections::HashMap<String, String>,
) -> Result<()> {
    let principal = principal_for(state, &request);
    let guard = state
        .engine
        .read()
        .map_err(|_| anyhow::anyhow!("poisoned"))?;
    let session = match Session::open(guard.store(), principal, state.policy.clone()) {
        Ok(s) => s,
        Err(e) => {
            return problem::send(
                request,
                "internal",
                500,
                &format!("opening a session: {e}"),
                &[],
                accept,
            )
        }
    };
    let view = guard.view(&session);

    let options = match query_options(state, params) {
        // A query may hold relative IRIs, which resolve against the endpoint.
        Ok(options) => options.with_base_iri(base_iri(&request, "/query")),
        Err(e) => return problem::bad_request(request, &e, accept),
    };
    let explain = options.explain;

    let (results, explanation) = match Engine::query_with(&view, query, &options) {
        Ok(pair) => pair,
        Err(e) => return problem::failed(request, &e, accept),
    };

    // `?explain` returns the plan instead of the answers. The results still have to be
    // drained, because the per-operator statistics are only filled in as rows flow.
    if explain {
        if let Some(explanation) = explanation {
            drain(results);
            let mut json = Vec::new();
            explanation.write_in_json(&mut json)?;
            return respond(request, 200, "application/json", json);
        }
    }

    serialise(request, results, accept)
}

/// Answers a placetime URI: see `placetime`.
fn serve_placetime(
    state: &State,
    request: Request,
    path: &str,
    accept: Option<&str>,
) -> Result<()> {
    let Some(mode) = state.config.placetime.as_ref() else {
        return respond(request, 404, "text/plain", b"not found".to_vec());
    };
    let Some(relative) = mode.relative(path) else {
        return respond(request, 404, "text/plain", b"not found".to_vec());
    };
    let host = header(&request, "host");
    let forwarded = if state.config.trust_forwarded {
        header(&request, "x-forwarded-host")
    } else {
        None
    };
    let answer = placetime::answer(
        mode,
        state.config.placetime_vocabulary,
        relative,
        host.as_deref(),
        forwarded.as_deref(),
        accept,
    );
    let extra: Vec<(&str, &str)> = answer
        .headers
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    respond_with(
        request,
        answer.status,
        &answer.content_type,
        answer.body,
        &extra,
    )
}

/// Whether a path addresses the Graph Store Protocol.
///
/// The configured path exactly, and — when direct graph identification is on — anything
/// beneath it. Direct identification names the graph *with the path itself*, so
/// `/gsp/person/1.ttl` is a request about a different graph from `/gsp/person/2.ttl`, and
/// matching only the exact prefix would 404 every one of them.
///
/// Without `--gsp-base` the sub-paths are not claimed, because nothing could resolve them
/// to a graph name and answering 400 where a 404 belongs would be worse than not matching.
fn is_graph_store_path(state: &State, path: &str) -> bool {
    if path == state.config.gsp_path {
        return true;
    }
    state.config.gsp_base.is_some()
        && path.starts_with(&format!("{}/", state.config.gsp_path.trim_end_matches('/')))
}

/// `POST /maintenance/purge` — reclaim spatial index entries for departed geometries.
///
/// # Why this is a maintenance step and not automatic
///
/// The spatial index tracks the **dictionary**, which never forgets, rather than the store.
/// That is what lets it catch up with a write in a fraction of a millisecond instead of
/// rebuilding — but it means deleting geometries leaves entries behind. Nothing is wrong
/// while they are there: the index is a superset filter, and a geometry with no quads fails
/// to join and contributes no row. It is memory, not correctness.
///
/// **Restarting does not clear them.** The index is rebuilt at startup from the dictionary,
/// so it comes back holding every geometry ever interned. Reclaiming really does need an
/// explicit step, which is why this exists.
///
/// # Scheduling
///
/// There is no timer inside the server. This is an endpoint so that whatever already
/// schedules things on the host — cron, a systemd timer, a Kubernetes CronJob — can call it,
/// the same way `deploy/backup.sh` calls `/backup`. A server that schedules its own
/// maintenance is a server that does something surprising at three in the morning.
///
/// # The guard
///
/// The same shape as [`backup`]: the endpoint does not exist unless `--purge-role` is set,
/// the principal must hold that role, and without `--trust-forwarded-identity` every request
/// is anonymous and holds no roles. Purging cannot destroy data — it only drops derived
/// entries — but it is a whole-store operation whose cost is proportional to the index, and
/// an unauthenticated server should not be able to be told to do work.
fn purge(state: &State, request: Request) -> Result<()> {
    let Some(role) = &state.config.purge_role else {
        // Not 403: a switched-off endpoint should be indistinguishable from one that was
        // never built, so probing cannot map the configuration.
        return respond(request, 404, "text/plain", b"not found".to_vec());
    };

    let principal = principal_for(state, &request);
    if !principal.has_role(role) {
        return respond(
            request,
            403,
            "text/plain",
            format!("a purge requires the `{role}` role").into_bytes(),
        );
    }

    let Some(index) = state.spatial() else {
        return respond(
            request,
            409,
            "text/plain",
            b"there is no spatial index to purge".to_vec(),
        );
    };

    // A read lock: purging reads the store to ask which geometries are still referenced, and
    // mutates only the index, which has its own lock.
    let guard = state
        .engine
        .read()
        .map_err(|_| anyhow::anyhow!("poisoned"))?;
    let outcome = index.purge(guard.store());
    drop(guard);

    match outcome {
        Ok(report) => {
            let body = format!(
                r#"{{"examined":{},"dropped":{},"retained":{}}}"#,
                report.examined, report.dropped, report.retained
            );
            respond(request, 200, "application/json", body.into_bytes())
        }
        Err(e) => respond(request, 500, "text/plain", e.to_string().into_bytes()),
    }
}

/// Takes a consistent snapshot of the store, for an administrator.
///
/// # Why the client cannot name the destination
///
/// It would be the obvious API — `POST /backup?to=/srv/backups/tonight` — and it would be an
/// arbitrary-write primitive: whatever the server process can write, a caller could ask it
/// to fill with a copy of the database. The server owns the parent directory (`--backup-dir`)
/// and mints a timestamped child; the caller chooses nothing but the moment.
///
/// # The guard
///
/// Three conditions, each of which fails closed:
///
/// 1. **The endpoint does not exist** unless both `--backup-dir` and `--backup-role` are set.
///    An unconfigured server answers 404, so the surface is absent rather than merely
///    defended.
/// 2. **The principal must hold the named role.** A backup copies the whole store, ignoring
///    the policy that governs every query — so this is the one operation where the ordinary
///    access controls do not apply, and it needs its own.
/// 3. **Identity has to be trustworthy.** Without `--trust-forwarded-identity` every request
///    is anonymous and holds no roles, so the endpoint answers 403 to everyone. That is the
///    correct default: an unauthenticated server should not be able to be told to copy
///    itself.
fn backup(state: &State, request: Request) -> Result<()> {
    let (Some(dir), Some(role)) = (&state.config.backup_dir, &state.config.backup_role) else {
        // Not 403: an endpoint that is switched off should be indistinguishable from one
        // that was never built, so probing cannot map the configuration.
        return respond(request, 404, "text/plain", b"not found".to_vec());
    };

    let principal = principal_for(state, &request);
    if !principal.has_role(role) {
        return respond(
            request,
            403,
            "text/plain",
            format!("a backup requires the `{role}` role").into_bytes(),
        );
    }

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let destination = std::path::Path::new(dir).join(format!("holos-{stamp}"));
    if let Err(e) = std::fs::create_dir_all(dir) {
        return respond(
            request,
            500,
            "text/plain",
            format!("could not prepare {dir}: {e}").into_bytes(),
        );
    }

    // A read lock: a checkpoint is a read of the store, and blocking writers for its
    // duration would defeat the point of having one.
    let guard = state
        .engine
        .read()
        .map_err(|_| anyhow::anyhow!("poisoned"))?;

    // An in-memory store has no files to checkpoint, so its backup is a snapshot: every
    // graph, as gzipped N-Quads, which `--data` reads back at the next start. That is what
    // makes memory a usable store for a service, not only for a test -- serve from memory,
    // call this from a scheduler, and restart from the newest file.
    //
    // It holds the read lock for the whole dump, so writers wait and the file is one
    // consistent state of the store. That is longer than a checkpoint takes, and the
    // price of a backup that is not a mixture of before and after some write.
    if guard.store().on_disk_bytes().is_none() {
        let destination = destination.with_extension("nq.gz");
        if destination.exists() {
            drop(guard);
            return respond(
                request,
                409,
                "text/plain",
                format!(
                    "{} already exists; backups are named by the second, so try again in one",
                    destination.display()
                )
                .into_bytes(),
            );
        }
        let outcome = Session::open(guard.store(), Principal::anonymous(), Policy::permit_all())
            .map_err(holos_engine::EngineError::from)
            .and_then(|session| {
                // Everything, whatever the caller's own policy: a backup that left out what
                // its trigger could not read would restore a different store.
                let view = guard.view(&session);
                holos_engine::dump::to_file(&view, &destination, None)
            });
        drop(guard);
        return match outcome {
            Ok(quads) => {
                let body = format!(
                    r#"{{"path":{},"quads":{quads}}}"#,
                    ui::json_string(&destination.display().to_string())
                );
                respond(request, 201, "application/json", body.into_bytes())
            }
            // 507, as for a checkpoint that will not fit: the storage is short, not the
            // server broken. The partial file is already gone.
            Err(holos_engine::EngineError::Io(e))
                if e.kind() == std::io::ErrorKind::StorageFull =>
            {
                respond(
                    request,
                    507,
                    "text/plain",
                    format!("not enough room in {dir} for the snapshot: {e}").into_bytes(),
                )
            }
            Err(e) => respond(request, 500, "text/plain", e.to_string().into_bytes()),
        };
    }

    // Refused before it starts, if it cannot fit. This is the endpoint a scheduler calls
    // every night, so it is the one that fills a disk while nobody is watching — and the
    // hard links a checkpoint makes are what stop compaction reclaiming the space, so a
    // backup directory on the store's own filesystem grows until something says no.
    //
    // 507 rather than 500: the request was well formed and the server is healthy. It is the
    // storage that is insufficient, which is exactly what the code means and is a thing a
    // scheduler can alert on differently from a fault.
    if let (Some(needed), Some(available)) = (
        guard.store().on_disk_bytes(),
        fs4::available_space(dir).ok(),
    ) {
        if needed > 0 && available < needed {
            drop(guard);
            return respond(
                request,
                507,
                "text/plain",
                format!(
                    "not enough room for a checkpoint: the store holds {needed} bytes and \
                     {dir} has {available} free"
                )
                .into_bytes(),
            );
        }
    }

    let outcome = guard.store().checkpoint(&destination);
    let quads = guard.store().len();
    drop(guard);

    match outcome {
        Ok(()) => {
            let body = format!(
                r#"{{"path":{},"quads":{quads}}}"#,
                ui::json_string(&destination.display().to_string())
            );
            respond(request, 201, "application/json", body.into_bytes())
        }
        Err(holos_store::StorageError::Unsupported(why)) => {
            // 409 rather than 500: the request was well formed and the server is healthy;
            // this store simply cannot do it right now, or at all.
            respond(request, 409, "text/plain", why.into_bytes())
        }
        Err(e) => respond(request, 500, "text/plain", e.to_string().into_bytes()),
    }
}

/// The Graph Store Protocol.
///
/// One function for all five verbs because they share the target resolution, the policy
/// session and the existence check, and splitting them would mean five copies of the part
/// that has to agree.
fn graph_store(
    state: &State,
    mut request: Request,
    params: &std::collections::HashMap<String, String>,
) -> Result<()> {
    let method = request.method().as_str().to_owned();
    let writing = matches!(method.as_str(), "PUT" | "POST" | "DELETE");

    if writing && state.config.read_only {
        return respond(
            request,
            403,
            "text/plain",
            b"this endpoint is read-only".to_vec(),
        );
    }

    let path = request.url().split('?').next().unwrap_or("/").to_owned();
    // `POST` to the protocol endpoint itself, carrying neither `?graph` nor `?default`,
    // is the specification's "create a graph and tell me its name" (§5.5). It has to be
    // recognised before the ordinary rules run: with a base configured those would resolve
    // the bare endpoint path by direct identification and name the new graph after the
    // endpoint, which is not a graph.
    let minting = method == "POST"
        && path == state.config.gsp_path
        && !params.contains_key("graph")
        && !params.contains_key("default");

    // Set only for a minted graph, and only so the response can carry `Location`.
    let mut minted: Option<String> = None;

    let target = if minting {
        let Some(base) = state.config.gsp_base.as_deref() else {
            return respond(
                request,
                400,
                "text/plain",
                b"creating a graph by POST needs --gsp-base, so the server can name it".to_vec(),
            );
        };
        let name = gsp::mint_graph_name(base, &path);
        minted = Some(name.as_str().to_owned());
        gsp::Target::Named(name)
    } else {
        match gsp::target(params, &path, state.config.gsp_base.as_deref()) {
            Ok(target) => target,
            Err(e) => return respond(request, 400, "text/plain", e.to_string().into_bytes()),
        }
    };

    let accept = header(&request, "accept");
    let content_type = header(&request, "content-type");
    let mut body = Vec::new();
    if writing {
        body = match read_body(&mut request, state.config.max_body)? {
            Ok(body) => body,
            Err(refusal) => return refusal.send(request, accept.as_deref()),
        };
    }

    // What the body holds, worked out before any lock is taken so an unusable one costs
    // nothing. Ordinarily this is one document; a graph submitted as a file upload arrives
    // as `multipart/form-data` and carries one per part, each with its own prefixes.
    let mut documents: Vec<(Vec<u8>, RdfFormat)> = Vec::new();
    let mut unusable = false;
    match content_type.as_deref().and_then(gsp::multipart_boundary) {
        Some(boundary) => {
            for (part, media) in gsp::multipart_parts(&body, &boundary) {
                match rdf_format_of(Some(&media)) {
                    Some(format) => documents.push((part.to_vec(), format)),
                    // One part this store cannot read makes the whole upload a 415.
                    // Merging the rest would leave the client holding a graph that is not
                    // what it sent, under a status code saying it succeeded.
                    None => unusable = true,
                }
            }
        }
        None => match rdf_format_of(content_type.as_deref()) {
            Some(format) => documents.push((body, format)),
            None => unusable = true,
        },
    }
    // The RDF/XML parser has two denial-of-service defects (see `inputs`); a document that
    // would trip them is refused before it reaches the parser.
    for (document, format) in &documents {
        if *format == RdfFormat::RdfXml {
            if let Err(why) = inputs::check_rdf_xml(document) {
                return problem::bad_request(request, &why, accept.as_deref());
            }
        }
    }
    let usable = !unusable && !documents.is_empty();

    let principal = principal_for(state, &request);

    // A read takes the read lock; a write takes the write lock. Sharing one path would
    // mean every GET excluding every other request for no reason.
    if writing {
        let mut guard = state
            .engine
            .write()
            .map_err(|_| anyhow::anyhow!("poisoned"))?;
        let mut session = match Session::open(guard.store(), principal, state.policy.clone()) {
            Ok(s) => s,
            Err(e) => {
                return respond(
                    request,
                    500,
                    "text/plain",
                    format!("session: {e}").into_bytes(),
                )
            }
        };
        let existed = match gsp::exists(&guard, &session, &target) {
            Ok(existed) => existed,
            Err(e) => return respond(request, 500, "text/plain", e.to_string().into_bytes()),
        };

        // Decided before a scope is opened, because both answers are refusals rather than
        // writes and an early return past an open scope would leak it.
        if method == "DELETE" && !existed {
            // Not an error: the client learns the graph was not there.
            return respond(request, 404, "text/plain", b"no such graph".to_vec());
        }
        if matches!(method.as_str(), "PUT" | "POST") && !usable {
            return respond(
                request,
                415,
                "text/plain",
                b"unsupported media type: send RDF with a content-type this store parses".to_vec(),
            );
        }

        // One commit scope for the whole request. `PUT` is clear-then-merge, and without
        // this a body that fails to parse — or a quad the policy refuses — leaves the graph
        // cleared and not replaced, having told the client the request failed. Emptying
        // someone's graph is not an acceptable outcome of a 400.
        if let Err(e) = guard.store_mut().begin() {
            return respond(
                request,
                500,
                "text/plain",
                format!("begin: {e}").into_bytes(),
            );
        }

        let outcome = match method.as_str() {
            "DELETE" => {
                // DELETE removes the graph, not just its contents — otherwise a second
                // DELETE cannot answer 404.
                gsp::drop_graph(&mut guard, &mut session, &target).map(|_| 204)
            }
            // PUT replaces, POST merges — see `gsp::write`, which is where the two
            // operations a replace is made of are held together.
            "PUT" | "POST" => gsp::write(
                &mut guard,
                &mut session,
                &target,
                &documents,
                method == "PUT",
            )
            // 201 tells the client it created the graph; 204 that it changed one.
            .map(|_| if existed { 204 } else { 201 }),
            _ => Ok(400),
        };

        // A 400 is still a well-formed answer rather than a failure, so it commits — there
        // is nothing buffered under it. Anything that produced an error discards the batch,
        // which is the whole point.
        let outcome = match (&outcome, guard.store().in_scope()) {
            (Ok(_), true) => match guard.store_mut().commit() {
                Ok(()) => outcome,
                Err(e) => Err(anyhow::anyhow!("commit: {e}")),
            },
            (Err(_), true) => {
                guard.store_mut().rollback();
                outcome
            }
            (_, false) => outcome,
        };

        drop(guard);
        if matches!(method.as_str(), "PUT" | "POST" | "DELETE") {
            state.refresh_statistics();
            state.refresh_spatial();
        }

        return match outcome {
            // A graph the server named has to say the name, or the client has no way to
            // address what it just asked to have created.
            Ok(status) => match &minted {
                Some(name) => respond_with(
                    request,
                    status,
                    "text/plain",
                    Vec::new(),
                    &[("Location", name.as_str())],
                ),
                None => respond(request, status, "text/plain", Vec::new()),
            },
            Err(e) => {
                let denied = e
                    .downcast_ref::<holos_engine::EngineError>()
                    .is_some_and(|e| matches!(e, holos_engine::EngineError::AccessDenied));
                let status = if denied { 403 } else { 400 };
                respond(request, status, "text/plain", e.to_string().into_bytes())
            }
        };
    }

    let guard = state
        .engine
        .read()
        .map_err(|_| anyhow::anyhow!("poisoned"))?;
    let session = match Session::open(guard.store(), principal, state.policy.clone()) {
        Ok(s) => s,
        Err(e) => {
            return respond(
                request,
                500,
                "text/plain",
                format!("session: {e}").into_bytes(),
            )
        }
    };
    match gsp::exists(&guard, &session, &target) {
        Ok(false) => return respond(request, 404, "text/plain", b"no such graph".to_vec()),
        Err(e) => return respond(request, 500, "text/plain", e.to_string().into_bytes()),
        Ok(true) => {}
    }

    let format = http::negotiate_rdf(accept.as_deref());
    match gsp::read(&guard, &session, &target, format) {
        // HEAD is GET without the body, so the content-type still has to be right.
        Ok(_) if method == "HEAD" => respond(request, 200, format.media_type(), Vec::new()),
        Ok(body) => respond(request, 200, format.media_type(), body),
        Err(e) => respond(request, 500, "text/plain", e.to_string().into_bytes()),
    }
}

/// The RDF format a request body declares.
/// Whether a protocol parameter was given more than once, as a message saying so.
///
/// The protocol allows a request to carry exactly one `query` or `update`. Two is a client
/// error rather than a choice for the server to make: answering the first would run
/// something the client did not unambiguously ask for.
///
/// Takes the *encoded* parameter string — a query string or a form body — because the
/// parser keeps the first value of a non-repeatable key and drops the rest.
fn repeated(encoded: &str, key: &str) -> Option<String> {
    http::given_more_than_once(encoded, key)
        .then(|| format!("the request carries more than one '{key}' parameter"))
}

/// Whether a POST body cannot be used, given its declared content type.
///
/// Two refusals the protocol requires:
///
/// * **No `Content-Type` at all.** A body that is form-encoded looks identical to one that
///   is a query; guessing means running text the client never said was a query.
/// * **A charset that is not UTF-8.** The protocol fixes the encoding of both media types
///   at UTF-8, so a request declaring UTF-16 is asking for something the protocol does not
///   offer, and decoding it as UTF-8 anyway would answer over mojibake.
fn unusable_body(content_type: Option<&str>) -> Option<String> {
    let Some(value) = content_type else {
        return Some(
            "a POST body needs a Content-Type: application/sparql-query, \
             application/sparql-update, or application/x-www-form-urlencoded"
                .to_owned(),
        );
    };
    let charset = value.split(';').skip(1).find_map(|p| {
        let (name, v) = p.split_once('=')?;
        name.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| v.trim().trim_matches('"').to_ascii_lowercase())
    })?;
    (charset != "utf-8" && charset != "utf8")
        .then(|| format!("the protocol requires UTF-8; this request declares `{charset}`"))
}

/// The graphs `using-graph-uri` and `using-named-graph-uri` name.
///
/// Both are repeatable, and arrive newline-joined from the query-string parser.
fn named_dataset(
    params: &std::collections::HashMap<String, String>,
) -> Result<(Vec<NamedNode>, Vec<NamedNode>), holos_engine::EngineError> {
    let read = |key: &str| -> Result<Vec<NamedNode>, holos_engine::EngineError> {
        http::values(params, key)
            .into_iter()
            .map(|iri| {
                NamedNode::new(&iri).map_err(|e| {
                    holos_engine::EngineError::BadRequest(format!("{key} `{iri}`: {e}"))
                })
            })
            .collect()
    };
    Ok((read("using-graph-uri")?, read("using-named-graph-uri")?))
}

/// The base IRI a relative IRI in a request resolves against.
///
/// The protocol says a service *may* define one, and names the endpoint itself as the
/// obvious candidate. Without it `CONSTRUCT { <s> <p> 1 }` cannot parse at all, so the
/// choice is between having one and refusing a class of legal requests.
///
/// Built from the `Host` header so it matches the URI the client actually used, which is
/// what makes the resolved IRIs meaningful to that client.
fn base_iri(request: &Request, path: &str) -> String {
    service_base(header(request, "host").as_deref(), path)
}

/// `https://{host}{path}`. The scheme is `https` whatever the request arrived on, as the
/// placetime URIs' is: a server behind a front door that ends TLS receives plain HTTP, and
/// reading the scheme off the connection would mint `http://` IRIs for a service published
/// as `https://`.
fn service_base(host: Option<&str>, path: &str) -> String {
    format!("https://{}{path}", host.unwrap_or("localhost"))
}

fn rdf_format_of(content_type: Option<&str>) -> Option<RdfFormat> {
    let value = content_type?;
    let media = value.split(';').next()?.trim().to_ascii_lowercase();
    match media.as_str() {
        "text/turtle" | "application/x-turtle" => Some(RdfFormat::Turtle),
        "application/n-triples" | "text/plain" => Some(RdfFormat::NTriples),
        "application/trig" => Some(RdfFormat::TriG),
        "application/n-quads" => Some(RdfFormat::NQuads),
        "application/rdf+xml" => Some(RdfFormat::RdfXml),
        "text/n3" => Some(RdfFormat::N3),
        "application/ld+json" => Some(RdfFormat::JsonLd {
            profile: oxrdfio::JsonLdProfileSet::empty(),
        }),
        _ => None,
    }
}

/// Consumes a result stream for its side effect on the explanation's statistics.
fn drain(results: QueryResults<'_>) {
    match results {
        QueryResults::Solutions(iter) => {
            for solution in iter {
                if solution.is_err() {
                    return;
                }
            }
        }
        QueryResults::Graph(iter) => {
            for triple in iter {
                if triple.is_err() {
                    return;
                }
            }
        }
        QueryResults::Boolean(_) => {}
    }
}

/// Builds the query options the SPARQL Protocol's parameters ask for.
///
/// `default-graph-uri` and `named-graph-uri` were previously parsed and then ignored, so a
/// client asking for one dataset silently got answers over another. They are repeatable in
/// the protocol; `http::parse_form` keeps the first of a repeated key, so multiples arrive
/// space-separated from the query string helper instead.
fn query_options(
    state: &State,
    params: &std::collections::HashMap<String, String>,
) -> Result<QueryOptions, String> {
    let mut options = QueryOptions::new();
    if let Some(timeout) = state.config.timeout {
        options = options.with_timeout(timeout);
    }
    if let Some(limit) = state.config.max_query_memory {
        options = options.with_memory_limit(limit);
    }
    if let Some(rows) = state.config.max_blocking_rows {
        options = options.with_blocking_budget(rows);
    }
    if let Some(bytes) = state.config.max_spill_disk {
        options = options.with_spill_disk(bytes);
    }
    if let Some(bytes) = state.config.spill_bytes {
        options = options.spilling(bytes);
    }
    if let Some(stats) = state.statistics() {
        options = options.reordering(stats);
    }
    if let Some(index) = state.spatial() {
        options = options.with_spatial(index);
    }
    for iri in http::values(params, "default-graph-uri") {
        let node = NamedNode::new(&iri).map_err(|e| format!("default-graph-uri `{iri}`: {e}"))?;
        options = options.with_default_graph(node.into());
    }
    for iri in http::values(params, "named-graph-uri") {
        let node = NamedNode::new(&iri).map_err(|e| format!("named-graph-uri `{iri}`: {e}"))?;
        options = options.with_named_graph(node.into());
    }
    if params.contains_key("explain") {
        options = options.explaining();
    }
    Ok(options)
}

/// Applies a SPARQL update, holding the write lock for its duration.
fn apply_update(
    state: &State,
    request: Request,
    update: &str,
    params: &std::collections::HashMap<String, String>,
    accept: Option<&str>,
) -> Result<()> {
    // Read before the request is consumed by a response, and before the lock is taken.
    let base = base_iri(&request, "/update");
    let principal = principal_for(state, &request);
    let mut guard = state
        .engine
        .write()
        .map_err(|_| anyhow::anyhow!("poisoned"))?;
    let mut session = match Session::open(guard.store(), principal, state.policy.clone()) {
        Ok(s) => s,
        Err(e) => {
            return problem::send(
                request,
                "internal",
                500,
                &format!("opening a session: {e}"),
                &[],
                accept,
            )
        }
    };
    // `using-graph-uri` names the dataset the update's WHERE runs against. It is applied
    // to the parsed form rather than the text, so an update that names its own dataset can
    // be told apart from one that does not — the protocol makes carrying both an error.
    let outcome = named_dataset(params).and_then(|(default_graphs, named_graphs)| {
        let mut parsed = sparql_update::parse(update, Some(&base))?;
        inputs::check_loads(&parsed, state.config.load_dir.as_deref())
            .map_err(holos_engine::EngineError::BadRequest)?;
        sparql_update::with_protocol_dataset(&mut parsed, default_graphs, named_graphs)?;
        sparql_update::apply(&mut guard, &mut session, &parsed)
    });
    // Release the write lock before rebuilding: refresh_statistics takes a read lock, and
    // holding the write lock across it would deadlock.
    drop(guard);
    if outcome.is_ok() {
        state.refresh_statistics();
        state.refresh_spatial();
    }

    match outcome {
        Ok(outcome) => {
            let body = format!(
                r#"{{"inserted":{},"deleted":{},"graphsCreated":{},"graphsDropped":{}}}"#,
                outcome.inserted, outcome.deleted, outcome.graphs_created, outcome.graphs_dropped
            );
            respond(request, 200, "application/json", body.into_bytes())
        }
        Err(e) => problem::failed(request, &e, accept),
    }
}

/// Writes results in the negotiated format.
///
/// The rows are produced while they are written, so this is where a timeout, the memory
/// ceiling or a scan failure actually arrives. It used to propagate out of the handler,
/// which answered the client with an empty 500 and the operator with a log line; now it is
/// a problem document like any other failure, which is the one place a client learns why
/// a query that started well did not finish.
fn serialise(request: Request, results: QueryResults<'_>, accept: Option<&str>) -> Result<()> {
    match render(results, accept) {
        Ok((media_type, body)) => respond(request, 200, media_type, body),
        Err(e) => problem::failed(request, &e, accept),
    }
}

/// The whole answer, in the negotiated format, or the failure that stopped it.
fn render(
    results: QueryResults<'_>,
    accept: Option<&str>,
) -> std::result::Result<(&'static str, Vec<u8>), holos_engine::EngineError> {
    use holos_engine::EngineError;
    match results {
        QueryResults::Graph(triples) => {
            let format = http::negotiate_rdf(accept);
            let mut writer = RdfSerializer::from_format(format).for_writer(Vec::new());
            for triple in triples {
                let triple = triple.map_err(EngineError::Evaluation)?;
                writer
                    .serialize_triple(triple.as_ref())
                    .map_err(EngineError::Io)?;
            }
            let body = writer.finish().map_err(EngineError::Io)?;
            Ok((format.media_type(), body))
        }
        QueryResults::Boolean(value) => {
            let format = http::negotiate_results(accept);
            let body = QueryResultsSerializer::from_format(format)
                .serialize_boolean_to_writer(Vec::new(), value)
                .map_err(EngineError::Io)?;
            Ok((http::results_media_type(format), body))
        }
        QueryResults::Solutions(solutions) => {
            let format = http::negotiate_results(accept);
            let variables = solutions.variables().to_vec();
            let mut writer = QueryResultsSerializer::from_format(format)
                .serialize_solutions_to_writer(Vec::new(), variables)
                .map_err(EngineError::Io)?;
            for solution in solutions {
                let solution = solution.map_err(EngineError::Evaluation)?;
                writer.serialize(&solution).map_err(EngineError::Io)?;
            }
            let body = writer.finish().map_err(EngineError::Io)?;
            Ok((http::results_media_type(format), body))
        }
    }
}

/// Builds the principal for a request.
///
/// Forwarded identity headers are read **only** when the operator turned them on. Reading
/// them by default would let any client on the port name its own roles and clearance,
/// which is the opposite of what §14 is for.
fn principal_for(state: &State, request: &Request) -> Principal {
    let mut principal = if state.config.trust_easy_auth {
        // A header that is missing or unreadable grants nothing: the request is anonymous.
        match header(request, "x-ms-client-principal") {
            Some(value) => {
                let id = header(request, "x-ms-client-principal-id");
                easyauth::principal(&value, id.as_deref()).unwrap_or_else(|e| {
                    eprintln!("ignoring an unreadable identity: {e}");
                    Principal::anonymous()
                })
            }
            None => Principal::anonymous(),
        }
    } else if state.config.trust_forwarded {
        match header(request, "x-holos-principal") {
            Some(id) => Principal::new(NamedNode::new_unchecked(format!(
                "urn:holos:principal:forwarded/{}",
                id.replace(
                    |c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_',
                    "_"
                )
            ))),
            None => Principal::anonymous(),
        }
    } else {
        Principal::anonymous()
    };

    if state.config.trust_forwarded {
        if let Some(roles) = header(request, "x-holos-roles") {
            for role in roles.split(',').map(str::trim).filter(|r| !r.is_empty()) {
                principal = principal.with_role(role);
            }
        }
        if let Some(level) =
            header(request, "x-holos-clearance").and_then(|c| c.trim().parse().ok())
        {
            principal = principal.with_clearance(Label::level(level));
        }
    }
    for role in &state.config.roles {
        principal = principal.with_role(role);
    }
    if let Some(level) = state.config.clearance {
        principal = principal.with_clearance(Label::level(level));
    }
    principal
}

/// The store's counts and the server's build, as one JSON object.
///
/// The build is here as well as in the `Server` header because a monitor that reads
/// `/stats` already, and keeps the numbers, should be able to keep the version next to
/// them without parsing headers.
fn stats(state: &State) -> String {
    let Ok(guard) = state.engine.read() else {
        return r#"{"error":"store lock poisoned"}"#.to_owned();
    };
    let store = guard.store();
    let graphs = store.named_graphs().map(|g| g.len()).unwrap_or(0);
    format!(
        r#"{{"quads":{},"dictionaryTerms":{},"namedGraphs":{},{}}}"#,
        store.len(),
        store.dictionary_len(),
        graphs,
        BUILD.json_members()
    )
}

/// Why a request body was not read.
#[derive(Debug, PartialEq, Eq)]
enum BodyRefusal {
    /// Larger than the ceiling, which is carried.
    TooLarge(usize),
    /// Not UTF-8, where the protocol requires it.
    NotUtf8,
}

impl BodyRefusal {
    fn send(self, request: Request, accept: Option<&str>) -> Result<()> {
        match self {
            Self::TooLarge(limit) => problem::send(
                request,
                "too-large",
                413,
                &format!(
                    "the request body is larger than this server accepts ({} MiB; --max-body)",
                    limit / (1024 * 1024)
                ),
                &[],
                accept,
            ),
            Self::NotUtf8 => problem::bad_request(
                request,
                "the request body is not UTF-8, which the protocol requires",
                accept,
            ),
        }
    }
}

/// Reads a request body, refusing one larger than `limit`: by its declared length before
/// reading anything, and by counting for a chunked body that declares none, so the cap
/// holds whatever the client says.
fn read_body(
    request: &mut Request,
    limit: Option<usize>,
) -> Result<std::result::Result<Vec<u8>, BodyRefusal>> {
    let Some(limit) = limit else {
        let mut body = Vec::new();
        request.as_reader().read_to_end(&mut body)?;
        return Ok(Ok(body));
    };
    if request
        .body_length()
        .is_some_and(|declared| declared > limit)
    {
        return Ok(Err(BodyRefusal::TooLarge(limit)));
    }
    Ok(capped(request.as_reader(), limit)?)
}

/// Reads at most `limit` bytes from `reader`, or reports that there were more.
fn capped(
    reader: impl std::io::Read,
    limit: usize,
) -> std::io::Result<std::result::Result<Vec<u8>, BodyRefusal>> {
    use std::io::Read as _;
    let mut body = Vec::new();
    reader
        .take(u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut body)?;
    Ok(if body.len() > limit {
        Err(BodyRefusal::TooLarge(limit))
    } else {
        Ok(body)
    })
}

/// [`read_body`], for a body the protocol fixes at UTF-8.
fn read_text_body(
    request: &mut Request,
    limit: Option<usize>,
) -> Result<std::result::Result<String, BodyRefusal>> {
    Ok(read_body(request, limit)?
        .and_then(|bytes| String::from_utf8(bytes).map_err(|_| BodyRefusal::NotUtf8)))
}

fn respond(request: Request, status: u16, content_type: &str, body: Vec<u8>) -> Result<()> {
    respond_with(request, status, content_type, body, &[])
}

/// `respond`, plus headers this particular answer needs — `Location`, so far.
fn respond_with(
    request: Request,
    status: u16,
    content_type: &str,
    body: Vec<u8>,
    extra: &[(&str, &str)],
) -> Result<()> {
    let mut response = Response::from_data(body).with_status_code(status);
    // RFC 9110 §10.2.4: the one place HTTP gives a server to say what it is. tiny-http
    // fills it with its own name unless told otherwise, and an operator asking "which
    // build is answering on this port" wants ours. A front door may replace it.
    let product = BUILD.product("holos");
    for (name, value) in [
        ("Server", product.as_str()),
        ("Content-Type", content_type),
        // The console is served from this same origin, but a SPARQL endpoint is routinely
        // queried from a page somewhere else, and refusing that by default makes the
        // endpoint useless for its most common job.
        ("Access-Control-Allow-Origin", "*"),
        ("Access-Control-Allow-Headers", "content-type, accept"),
        // The Graph Store Protocol uses all five verbs, so a browser client that can only
        // GET and POST cannot reach half of it.
        (
            "Access-Control-Allow-Methods",
            "GET, HEAD, POST, PUT, DELETE, OPTIONS",
        ),
        // Without this a browser cannot read the name of a graph it just created.
        ("Access-Control-Expose-Headers", "location"),
    ]
    .into_iter()
    .chain(extra.iter().copied())
    {
        if let Ok(header) = Header::from_bytes(name.as_bytes(), value.as_bytes()) {
            response.add_header(header);
        }
    }
    telemetry::record_status(status);
    request.respond(response)?;
    Ok(())
}

fn header(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str().to_owned())
}

fn is_network_fs(fstype: &str) -> bool {
    holos_store::fscheck::Locking::Shared {
        fstype: fstype.to_owned(),
        mount_point: std::path::PathBuf::new(),
    }
    .is_network()
}

fn open_engine(config: &Config) -> Result<Engine> {
    if config.store_read_only && !config.data.is_empty() {
        anyhow::bail!("--data cannot load into a store opened with --store-read-only");
    }
    if config.store_read_only && config.store.is_none() {
        anyhow::bail!("--store-read-only needs a --store to open");
    }
    match &config.store {
        None => Ok(Engine::new()),
        #[cfg(feature = "rocksdb")]
        Some(path) => {
            let storage = if config.store_read_only {
                holos_store::RocksStorage::open_read_only(path)
            } else {
                holos_store::RocksStorage::open(path)
            }
            .with_context(|| format!("opening the store at {path}"))?;
            Ok(Engine::with_store(holos_store::Store::with_storage(
                storage,
            )))
        }
        #[cfg(not(feature = "rocksdb"))]
        Some(_) => anyhow::bail!("this build has no persistent backend"),
    }
}

fn build_policy(config: &Config) -> Result<Policy> {
    let mut policy = if config.deny_all {
        Policy::default()
    } else {
        Policy::permit_all()
    };
    if config.fail_closed {
        policy = policy.with_semantics(holos_security::Semantics::Fail);
    }
    for iri in &config.allow_graphs {
        policy = policy.with_rule(Rule::allow(
            Modes::READ,
            Scope::Graph(iri_arg(iri)?),
            PrincipalMatch::Everyone,
        ));
    }
    for iri in &config.deny_predicates {
        policy = policy.with_rule(Rule::deny(
            Modes::READ,
            Scope::Predicate(iri_arg(iri)?),
            PrincipalMatch::Everyone,
        ));
    }
    for (iri, level) in &config.graph_labels {
        policy = policy.with_graph_label(iri_arg(iri)?, Label::level(*level));
    }
    Ok(policy)
}

fn iri_arg(s: &str) -> Result<NamedNode> {
    NamedNode::new(s).with_context(|| format!("`{s}` is not a valid IRI"))
}

/// The RDF format a file name implies, seeing through a `.gz` suffix.
fn format_for(path: &str) -> Result<RdfFormat> {
    holos_engine::source::format_for_path(std::path::Path::new(path)).with_context(|| {
        format!(
            "cannot infer an RDF format from `{path}`; expected .ttl .nt .trig .nq .rdf .n3 \
             .jsonld, optionally with .gz"
        )
    })
}

#[allow(clippy::too_many_lines)]
fn parse_args(args: &[String]) -> Result<Config> {
    let mut c = Config::default();
    let mut placetime_vocabulary_named = false;
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].clone();
        let value = |i: &mut usize| -> Result<String> {
            *i += 1;
            args.get(*i)
                .cloned()
                .with_context(|| format!("{flag} needs a value"))
        };
        match args[i].as_str() {
            "--listen" => c.listen = value(&mut i)?,
            "--data" => c.data.push(value(&mut i)?),
            "--store" => c.store = Some(value(&mut i)?),
            "--backup-dir" => c.backup_dir = Some(value(&mut i)?),
            "--backup-role" => c.backup_role = Some(value(&mut i)?),
            "--purge-role" => c.purge_role = Some(value(&mut i)?),
            "--threads" => c.threads = value(&mut i)?.parse()?,
            "--load-dir" => c.load_dir = Some(std::path::PathBuf::from(value(&mut i)?)),
            "--max-body" => {
                let mib: usize = value(&mut i)?.parse()?;
                c.max_body = (mib > 0).then(|| mib.saturating_mul(1024 * 1024));
            }
            "--timeout" => {
                let seconds: f64 = value(&mut i)?.parse()?;
                c.timeout = if seconds > 0.0 {
                    Some(Duration::from_secs_f64(seconds))
                } else {
                    None
                };
            }
            "--max-spill-disk" => {
                let gigabytes: f64 = value(&mut i)?.parse()?;
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a byte count from a gigabyte figure, bounded by the parse"
                )]
                let bytes = (gigabytes.max(0.0) * 1024.0 * 1024.0 * 1024.0) as usize;
                c.max_spill_disk = Some(bytes);
            }
            "--spill-bytes" | "--spill-distinct" => {
                let mib: f64 = value(&mut i)?.parse()?;
                c.spill_bytes = if mib > 0.0 {
                    #[allow(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "a byte count from a mebibyte figure, bounded by the parse"
                    )]
                    Some((mib * 1024.0 * 1024.0) as usize)
                } else {
                    None
                };
            }
            "--max-blocking-rows" => {
                let rows: u64 = value(&mut i)?.parse()?;
                c.max_blocking_rows = if rows > 0 { Some(rows) } else { None };
            }
            "--max-query-memory" => {
                let gigabytes: f64 = value(&mut i)?.parse()?;
                c.max_query_memory = if gigabytes > 0.0 {
                    #[allow(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "a byte count from a gigabyte figure, bounded by the parse"
                    )]
                    Some((gigabytes * 1024.0 * 1024.0 * 1024.0) as usize)
                } else {
                    None
                };
            }
            "--read-only" => c.read_only = true,
            "--store-read-only" => {
                c.store_read_only = true;
                c.read_only = true;
            }
            "--reorder" => c.reorder = true,
            "--gsp-base" => c.gsp_base = Some(value(&mut i)?),
            "--gsp-path" => c.gsp_path = value(&mut i)?,
            "--no-ui" => c.ui = false,
            "--ui-tiles" => {
                let template = value(&mut i)?;
                c.ui_tiles = (template != "none").then_some(template);
            }
            "--trust-forwarded-identity" => c.trust_forwarded = true,
            "--trust-azure-easy-auth" => c.trust_easy_auth = true,
            "--placetime" => {
                if c.placetime.is_none() {
                    c.placetime = Some(placetime::Mode::Host);
                }
            }
            "--placetime-base" => {
                let base = semops_placetime::Base::new(&value(&mut i)?)?;
                c.placetime = Some(placetime::Mode::Fixed(base));
            }
            "--placetime-vocabulary" => {
                let name = value(&mut i)?;
                c.placetime_vocabulary =
                    semops_placetime::Vocabulary::parse(&name).with_context(|| {
                        format!("--placetime-vocabulary is data.gov.uk or gist, not `{name}`")
                    })?;
                placetime_vocabulary_named = true;
            }
            "--role" => c.roles.push(value(&mut i)?),
            "--clearance" => c.clearance = Some(value(&mut i)?.parse()?),
            "--deny-all" => c.deny_all = true,
            "--allow-graph" => c.allow_graphs.push(value(&mut i)?),
            "--deny-predicate" => c.deny_predicates.push(value(&mut i)?),
            "--label-graph" => {
                let raw = value(&mut i)?;
                let (iri, level) = raw
                    .rsplit_once('=')
                    .with_context(|| format!("--label-graph wants <IRI>=<N>, got `{raw}`"))?;
                c.graph_labels.push((iri.to_owned(), level.parse()?));
            }
            "--fail-closed" => c.fail_closed = true,
            other => anyhow::bail!("unknown flag `{other}`\n\n{USAGE}"),
        }
        i += 1;
    }
    anyhow::ensure!(
        !placetime_vocabulary_named || c.placetime.is_some(),
        "--placetime-vocabulary chooses how the placetime URI sets are described, and needs \
         --placetime or --placetime-base to serve them"
    );
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_post_body_with_no_content_type_is_refused() {
        // A form-encoded body and a query body are indistinguishable without one, and
        // guessing means running text the client never said was a query.
        assert!(unusable_body(None).is_some());
    }

    #[test]
    fn a_charset_other_than_utf8_is_refused() {
        // The protocol fixes both media types at UTF-8. Decoding UTF-16 as UTF-8 would
        // answer over mojibake rather than say no.
        assert!(unusable_body(Some("application/sparql-query; charset=UTF-16")).is_some());
        assert!(unusable_body(Some("application/sparql-query; charset=utf-8")).is_none());
        assert!(unusable_body(Some("application/sparql-query; charset=\"UTF-8\"")).is_none());
        // No charset at all is the default, which is UTF-8.
        assert!(unusable_body(Some("application/sparql-query")).is_none());
    }

    #[test]
    fn a_body_over_the_ceiling_is_refused_however_it_arrives() {
        assert_eq!(capped(&b"12345"[..], 5).unwrap(), Ok(b"12345".to_vec()));
        assert_eq!(
            capped(&b"123456"[..], 5).unwrap(),
            Err(BodyRefusal::TooLarge(5))
        );
        // A body that never ends is read only as far as the ceiling and one byte more.
        assert_eq!(
            capped(std::io::repeat(b'x'), 1024).unwrap(),
            Err(BodyRefusal::TooLarge(1024))
        );
    }

    #[test]
    fn the_body_ceiling_is_configurable_and_removable() {
        let args = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(parse_args(&[]).unwrap().max_body, Some(DEFAULT_MAX_BODY));
        assert_eq!(
            parse_args(&args(&["--max-body", "1"])).unwrap().max_body,
            Some(1024 * 1024)
        );
        assert_eq!(
            parse_args(&args(&["--max-body", "0"])).unwrap().max_body,
            None
        );
    }

    #[test]
    fn relative_iris_resolve_against_an_https_base() {
        assert_eq!(
            service_base(Some("data.example:8443"), "/query"),
            "https://data.example:8443/query"
        );
        assert_eq!(service_base(None, "/update"), "https://localhost/update");
    }

    #[test]
    fn a_duplicated_query_parameter_is_reported() {
        assert!(repeated("query=a&query=b", "query").is_some());
        assert!(repeated("query=a", "query").is_none());
    }

    #[test]
    fn the_dataset_parameters_become_graph_names() {
        let params = http::parse_form(
            "using-graph-uri=http%3A%2F%2Fa&using-graph-uri=http%3A%2F%2Fb\
             &using-named-graph-uri=http%3A%2F%2Fc",
        );
        let (default_graphs, named_graphs) = named_dataset(&params).expect("valid IRIs");
        assert_eq!(default_graphs.len(), 2);
        assert_eq!(named_graphs.len(), 1);
    }

    #[test]
    fn a_dataset_parameter_that_is_not_an_iri_is_a_bad_request() {
        // Not a 500: the client sent it, and the message should say which one.
        let params = http::parse_form("using-graph-uri=not%20an%20iri");
        let error = named_dataset(&params).expect_err("should refuse");
        assert!(
            matches!(error, holos_engine::EngineError::BadRequest(_)),
            "got {error:?}"
        );
    }
}
