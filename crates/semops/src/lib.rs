//! `semops`: the SemOps System command.
//!
//! One binary for the whole toolchain. `check` runs the ontology quality checks natively:
//! the registry and its queries are compiled in (`semops-checks`), and the checks run on
//! HOLOS's engine, with no Python and no Java. The other subcommands are the engines'
//! commands, linked in as libraries, so `semops serve` is `holos-server`, `semops query` is
//! `holos query`, `semops validate` is `shacl` and `semops map` is `tarka`, argument for
//! argument.
//!
//! Run under another name, as a copy or link called `holos-server`, `holos`, `shacl` or
//! `tarka`, the binary is that command. One file can then stand in for all five.
//!
//! The command is this library's [`main_with`], so that the `semops-system` Python wheel can
//! carry it too. The binary adds the global allocator (`src/main.rs`).

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use semops_checks::run::{self, Finding, Run};
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod remote;
mod reports;

#[derive(Parser)]
#[command(
    name = "semops",
    version,
    about = "The SemOps System command",
    after_help = concat!(
        "Any argument naming an Azure Storage blob (abfss://, az://, ",
        "https://<account>.blob.core.windows.net/...) is downloaded before the subcommand ",
        "runs, and the subcommand reads the local copy. Credentials come from the AZURE_* ",
        "environment variables or a managed identity."
    )
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// A command's own arguments, passed through untouched: its `--help` is its own.
#[derive(clap::Args)]
struct Forward {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 0..)]
    args: Vec<OsString>,
}

#[derive(Subcommand)]
enum Command {
    /// Serve SPARQL 1.2 over HTTP, with the Graph Store Protocol (holos-server).
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Serve(Forward),
    /// Load RDF files into a persistent store (holos load).
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Load(Forward),
    /// Run a SPARQL query over files or a store (holos query).
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Query(Forward),
    /// Run a SPARQL update against a store (holos update).
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Update(Forward),
    /// Validate RDF against SHACL shapes (shacl).
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Validate(Forward),
    /// Map CSV, Parquet or Arrow to RDF with TARQL or OTTR (tarka).
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Map(Forward),
    /// Store statistics (holos stats).
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Stats(Forward),
    /// Write a store out as RDF (holos dump).
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Dump(Forward),
    /// Checkpoint a store (holos backup). With `--to <blob URL>`, upload the checkpoint.
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Backup(Forward),
    /// Copy a store's live data into a fresh one (holos compact).
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Compact(Forward),
    /// Write RDFS entailments into their own graph (holos entail).
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Entail(Forward),
    /// Run SHACL-AF or SPARQL rules over a store (holos rules).
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Rules(Forward),
    /// Upload a file or a directory, such as a checkpoint, to Azure Blob Storage.
    ///
    /// Under a URL ending in `/` the source keeps its own name, as `cp -r` would; otherwise
    /// the URL names it. A directory's files go under that prefix with their paths. Files
    /// stream from disk, a large one in blocks. Credentials come from the AZURE_* environment
    /// variables or a managed identity.
    Upload {
        /// The file or directory to upload.
        from: PathBuf,
        /// The blob, or with a trailing `/` the prefix, to upload to.
        to: String,
    },
    /// Download a blob, or every blob under a prefix, from Azure Blob Storage.
    ///
    /// A URL ending in `/`, or one that names no blob, is a prefix: its blobs are written
    /// under TO with their paths, which is how a checkpoint uploaded by `semops backup --to`
    /// is restored. TO must not exist.
    Download {
        /// The blob, or the prefix.
        from: String,
        /// The file or directory to create.
        to: PathBuf,
    },
    /// Compare two versions of an ontology and suggest a semantic-version bump.
    ///
    /// Classes, properties, subclass edges, disjointness, equivalence, domains, ranges,
    /// property characteristics and rdfs:label/rdfs:comment are compared, as the Ontology
    /// Quality Suite's `version-diff` compares them. Removing or narrowing anything is a
    /// major change; adding or widening is minor; changing only labels and comments is a
    /// patch. Imports are not followed: each file is compared as it stands.
    ///
    /// Exit status: 0, or 1 when the bump is at or above --fail-on; 2 when a file cannot be
    /// read.
    Diff {
        /// The earlier version.
        old: PathBuf,
        /// The later version.
        new: PathBuf,
        #[arg(long, value_enum, default_value_t = DiffFormat::Text)]
        format: DiffFormat,
        /// The least bump that fails the run.
        #[arg(long, value_enum, default_value_t = DiffFailOn::Never)]
        fail_on: DiffFailOn,
        /// Write the report here rather than to standard output.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Apply the quality checks' repairs to an ontology file.
    ///
    /// Runs the checks, then each finding's quick fix: a SPARQL Update template, bound to the
    /// finding and to the project's standards, run against the file's own triples, one
    /// after another. Repairs that only add triples are appended to a Turtle file as a new
    /// block, which keeps its formatting and comments. Any repair that removes a triple means
    /// the whole file is written out again, without its comments.
    ///
    /// The repaired document goes to standard output, or back to the file with --write, or
    /// to --out; what was repaired goes to standard error. Exit status: 0, or 2 when the
    /// file cannot be read or a repair fails.
    Repair {
        /// The ontology file to repair.
        file: PathBuf,
        /// Repair only this check's findings. Repeatable; the default is every check with a
        /// repair.
        #[arg(long = "check", value_name = "ID")]
        checks: Vec<String>,
        /// Write the repaired document back to the file.
        #[arg(long, conflicts_with = "out")]
        write: bool,
        /// Write the repaired document here.
        #[arg(long)]
        out: Option<PathBuf>,
        /// The language tag of labels a repair adds.
        #[arg(long, default_value = "en")]
        language: String,
        /// The owl:versionInfo a repair adds.
        #[arg(long, default_value = "0.1.0")]
        version_info: String,
        /// The base IRI of an owl:Ontology a repair declares.
        #[arg(long, default_value = "https://example.org/ontology/")]
        ontology_base: String,
        /// Which axiom stays when owl:equivalentClass and rdfs:subClassOf are both asserted
        /// (LOG-003).
        #[arg(long, value_enum, default_value_t = RedundantEquivalencePolicy::KeepEquivalentClass)]
        redundant_equivalence_policy: RedundantEquivalencePolicy,
    },
    /// Run the ontology quality checks over an ontology and its data.
    ///
    /// The files are read into one graph, so declarations in the ontology are checked
    /// against their use in the data and the other way round. Each check has a SHACL shape
    /// and a portable SPARQL query; --engine picks which run, as `ontology-suite check
    /// --engine` does, with the same findings.
    ///
    /// Exit status: 0 when nothing at or above --fail-on was found, 1 when something was,
    /// 2 when the run could not be made or a check failed to execute. A check that fails
    /// to execute finds nothing, which looks exactly like a clean graph, so it never
    /// passes a gate.
    Check {
        /// RDF files: Turtle, N-Triples, RDF/XML, TriG, N-Quads, N3 or JSON-LD, by
        /// extension, optionally gzipped.
        #[arg(required_unless_present = "list")]
        files: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// The least severe finding that fails the run.
        #[arg(long, value_enum, default_value_t = FailOn::Violation)]
        fail_on: FailOn,
        /// Which formulation of the checks runs: the shapes on the SHACL engine, the
        /// portable queries on HOLOS, or both merged. `both` and `sparql` find the same
        /// things, and `both` cross-checks the two formulations; `shacl` alone misses the
        /// checks that have only a query.
        #[arg(long, value_enum, default_value_t = EngineArg::Both)]
        engine: EngineArg,
        /// Write the report here rather than to standard output.
        #[arg(long)]
        out: Option<PathBuf>,
        /// List the registry's checks and exit.
        #[arg(long)]
        list: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    /// One line per finding, then a count by severity.
    Text,
    /// The findings as a JSON array.
    Json,
    /// The suite's `full_results.csv` columns.
    Csv,
    /// JUnit XML: one test case per check, failed by its findings at or above --fail-on,
    /// for test result views such as Azure DevOps's Tests tab.
    Junit,
    /// SARIF 2.1.0: each finding at the line its focus node first appears on, for code
    /// scanning such as GitHub's.
    Sarif,
}

#[derive(Clone, Copy, ValueEnum)]
enum DiffFormat {
    /// The suite's report: each kind of change, then the suggested bump.
    Text,
    /// The suite's `diff.json`.
    Json,
}

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq)]
enum DiffFailOn {
    Major,
    Minor,
    Patch,
    Never,
}

#[derive(Clone, Copy, ValueEnum)]
enum RedundantEquivalencePolicy {
    KeepEquivalentClass,
    KeepSubClassOf,
}

#[derive(Clone, Copy, ValueEnum)]
enum EngineArg {
    Both,
    Sparql,
    Shacl,
}

impl From<EngineArg> for run::Engine {
    fn from(engine: EngineArg) -> Self {
        match engine {
            EngineArg::Both => Self::Both,
            EngineArg::Sparql => Self::Sparql,
            EngineArg::Shacl => Self::Shacl,
        }
    }
}

/// How `semops check` was asked to run and report.
struct Options {
    format: Format,
    fail_on: FailOn,
    out: Option<PathBuf>,
    engine: run::Engine,
}

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq)]
enum FailOn {
    #[value(name = "Violation", alias = "violation")]
    Violation,
    #[value(name = "Warning", alias = "warning")]
    Warning,
    #[value(name = "Info", alias = "info")]
    Info,
    #[value(name = "never")]
    Never,
}

fn rank(severity: &str) -> u8 {
    match severity {
        "Violation" => 0,
        "Warning" => 1,
        "Info" => 2,
        _ => 3,
    }
}

/// The `semops` command, given its arguments as the operating system passed them, program
/// name first, and returning its exit status.
///
/// `--max-query-memory` under `semops serve` is enforced only when the caller has
/// installed holos-server's counting allocator and called
/// [`holos_engine::memory::mark_tracking`], as the `semops` binary does.
pub fn main_with(args: impl IntoIterator<Item = OsString>) -> ExitCode {
    let args: Vec<OsString> = args.into_iter().collect();
    if let Some(code) = args.first().and_then(|argv0| run_as(argv0, &args)) {
        return code;
    }
    match run_cli(args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("semops: {e:#}");
            ExitCode::from(2)
        }
    }
}

/// The command a copy or link of this binary is named after, run with the whole command
/// line; `None` when the name is not one of them, and the binary is `semops`.
fn run_as(argv0: &OsStr, args: &[OsString]) -> Option<ExitCode> {
    let name = Path::new(argv0).file_stem()?.to_str()?;
    let args = args.iter().cloned();
    Some(match name {
        "holos-server" => serve(args),
        "holos" => holos_cli::main_with(args),
        "shacl" => shacl_cli::main_with(args),
        "tarka" => tarka_cli::main_with(args),
        _ => return None,
    })
}

fn serve(args: impl IntoIterator<Item = OsString>) -> ExitCode {
    match holos_server::main_with(args) {
        Ok(()) => ExitCode::SUCCESS,
        // As holos-server's own `main` reports an error, returning it.
        Err(e) => {
            eprintln!("Error: {e:?}");
            ExitCode::FAILURE
        }
    }
}

/// Runs an engine's command as `semops <name>`, with `forward` as its arguments.
fn forward(name: &str, Forward { args }: Forward) -> ExitCode {
    let argv0 = OsString::from(format!("semops {name}"));
    let with = |first: Vec<OsString>| first.into_iter().chain(args.iter().cloned());
    match name {
        "serve" => serve(with(vec![argv0])),
        "validate" => shacl_cli::main_with(with(vec![argv0])),
        "map" => tarka_cli::main_with(with(vec![argv0])),
        // The rest are holos's commands, under the same names.
        command => holos_cli::main_with(with(vec![argv0, OsString::from(command)])),
    }
}

fn run_cli(args: Vec<OsString>) -> Result<ExitCode> {
    match args.get(1).and_then(|a| a.to_str()) {
        // Their URLs are what they transfer, not inputs to fetch first.
        Some("upload" | "download") => return transfer(Cli::parse_from(args).command),
        Some("backup") if blob_destination(&args[2..]).is_some() => return backup_to_blob(&args),
        _ => {}
    }
    // Blob URLs are fetched before anything parses the arguments, and the files live until
    // the command returns (`remote`).
    let local = remote::localise(args)?;
    let command = Cli::parse_from(local.args.iter().cloned()).command;
    if let Command::Repair {
        file, write: true, ..
    } = &command
        && let Some(url) = local.source_of(file)
    {
        bail!(
            "`repair --write` cannot write back to {}; use --out FILE",
            remote::redact(url)
        );
    }
    let forwarded = match command {
        Command::Serve(f) => ("serve", f),
        Command::Load(f) => ("load", f),
        Command::Query(f) => ("query", f),
        Command::Update(f) => ("update", f),
        Command::Validate(f) => ("validate", f),
        Command::Map(f) => ("map", f),
        Command::Stats(f) => ("stats", f),
        Command::Dump(f) => ("dump", f),
        Command::Backup(f) => ("backup", f),
        Command::Compact(f) => ("compact", f),
        Command::Entail(f) => ("entail", f),
        Command::Rules(f) => ("rules", f),
        Command::Diff {
            old,
            new,
            format,
            fail_on,
            out,
        } => return diff(&old, &new, format, fail_on, out),
        Command::Repair {
            file,
            checks,
            write,
            out,
            language,
            version_info,
            ontology_base,
            redundant_equivalence_policy,
        } => {
            let standards = semops_checks::repair::Standards {
                default_language_tag: language,
                redundant_equivalence_policy: match redundant_equivalence_policy {
                    RedundantEquivalencePolicy::KeepEquivalentClass => "keepEquivalentClass",
                    RedundantEquivalencePolicy::KeepSubClassOf => "keepSubClassOf",
                }
                .to_owned(),
                default_ontology_base_iri: ontology_base,
                default_version_info: version_info,
                ..semops_checks::repair::Standards::default()
            };
            let target = if write { Some(file.clone()) } else { out };
            return repair(&file, &checks, &standards, target);
        }
        check @ Command::Check { .. } => return run_check(check),
        transferring @ (Command::Upload { .. } | Command::Download { .. }) => {
            return transfer(transferring);
        }
    };
    Ok(forward(forwarded.0, forwarded.1))
}

/// `semops upload` and `semops download`.
fn transfer(command: Command) -> Result<ExitCode> {
    let (copied, verb, url) = match command {
        Command::Upload { from, to } => (remote::upload(&from, &to)?, "uploaded", to),
        Command::Download { from, to } => (remote::download(&from, &to)?, "downloaded", from),
        _ => unreachable!("only the transfer commands come here"),
    };
    eprintln!(
        "semops: {verb} {} file{}, {} bytes ({})",
        copied.files,
        if copied.files == 1 { "" } else { "s" },
        copied.bytes,
        remote::redact(&url)
    );
    Ok(ExitCode::SUCCESS)
}

/// The value of `--to` among a command's arguments, when it is a blob URL.
fn blob_destination(args: &[OsString]) -> Option<String> {
    let mut args = args.iter().filter_map(|a| a.to_str());
    while let Some(arg) = args.next() {
        let value = match arg.strip_prefix("--to=") {
            Some(value) => Some(value),
            None if arg == "--to" => args.next(),
            None => None,
        };
        if let Some(value) = value.filter(|v| remote::is_blob_url(v)) {
            return Some(value.to_owned());
        }
    }
    None
}

/// `semops backup --store DIR --to <blob URL>`: a checkpoint, uploaded, then removed.
///
/// The checkpoint is taken beside the store, so on the same filesystem RocksDB hard-links its
/// files: instant, and no disk beyond what the upload reads. It is uploaded as
/// `holos-<UTC time>`, the name `deploy/backup.sh` gives one, under a URL ending in `/`, or
/// to exactly the URL given otherwise, and removed whether or not the upload succeeds.
fn backup_to_blob(args: &[OsString]) -> Result<ExitCode> {
    let url = blob_destination(&args[2..]).expect("checked by the caller");
    let mut store: Option<PathBuf> = None;
    let mut rest: Vec<OsString> = Vec::new();
    let mut it = args[2..].iter();
    while let Some(arg) = it.next() {
        match arg.to_str() {
            Some("--store") => store = it.next().map(PathBuf::from),
            Some(a) if a.starts_with("--store=") => store = Some(PathBuf::from(&a[8..])),
            Some("--to") => {
                it.next();
            }
            Some(a) if a.starts_with("--to=") => {}
            _ => rest.push(arg.clone()),
        }
    }
    let store = store.context("`semops backup` needs --store DIR")?;
    let name = format!("holos-{}", remote::utc_stamp());
    let beside = store
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let store_name = store
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "store".to_owned());
    let local = beside.join(format!(".{store_name}-{name}"));

    let mut argv: Vec<OsString> = ["semops backup", "backup", "--store"]
        .map(OsString::from)
        .into();
    argv.push(store.into_os_string());
    argv.push("--to".into());
    argv.push(local.clone().into_os_string());
    argv.extend(rest);
    let code = holos_cli::main_with(argv);
    if code != ExitCode::SUCCESS {
        let _ = std::fs::remove_dir_all(&local);
        return Ok(code);
    }

    let target = remote::upload_target(Path::new(&name), &url);
    let uploaded = remote::upload_to(&local, &target);
    let removed = std::fs::remove_dir_all(&local);
    let copied = uploaded?;
    removed.with_context(|| format!("removing the local checkpoint {}", local.display()))?;
    eprintln!(
        "semops: uploaded the checkpoint, {} files, {} bytes, to {}",
        copied.files,
        copied.bytes,
        remote::redact(&target)
    );
    Ok(ExitCode::SUCCESS)
}

fn diff(
    old: &Path,
    new: &Path,
    format: DiffFormat,
    fail_on: DiffFailOn,
    out: Option<PathBuf>,
) -> Result<ExitCode> {
    use semops_checks::diff::{self, Bump};
    let (d, bump) = diff::diff(&run::load(&[old])?, &run::load(&[new])?);
    let mut text = match format {
        DiffFormat::Text => diff::report(&d, bump, &old.to_string_lossy(), &new.to_string_lossy()),
        DiffFormat::Json => serde_json::to_string_pretty(&diff::to_json(&d, bump))?,
    };
    text.push('\n');
    match out {
        Some(path) => {
            std::fs::write(&path, &text).with_context(|| format!("writing {}", path.display()))?;
        }
        None => std::io::stdout().lock().write_all(text.as_bytes())?,
    }
    let threshold = match fail_on {
        DiffFailOn::Major => Some(Bump::Major),
        DiffFailOn::Minor => Some(Bump::Minor),
        DiffFailOn::Patch => Some(Bump::Patch),
        DiffFailOn::Never => None,
    };
    Ok(if threshold.is_some_and(|t| bump >= t) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

fn repair(
    file: &Path,
    checks: &[String],
    standards: &semops_checks::repair::Standards,
    target: Option<PathBuf>,
) -> Result<ExitCode> {
    use semops_checks::repair::{self, Kind};
    let (document, prefixes) = repair::load_document(file)?;
    let report = run::check(&document)?;
    let mut working = document;
    let mut outcomes = Vec::new();
    for finding in &report.findings {
        let wanted = checks.is_empty()
            || finding
                .check_id
                .as_deref()
                .is_some_and(|id| checks.iter().any(|c| c == id));
        if !wanted {
            continue;
        }
        let Some(outcome) = repair::compute(&working, finding, standards, &prefixes)? else {
            continue;
        };
        // A finding an earlier repair already dealt with changes nothing.
        if outcome.added.is_empty() && outcome.removed.is_empty() {
            continue;
        }
        eprintln!(
            "{:<8} {}  {}  +{} -{}",
            outcome.check_id,
            finding.focus_node,
            outcome.title,
            outcome.added.len(),
            outcome.removed.len()
        );
        working.clone_from(&outcome.result);
        outcomes.push(outcome);
    }
    let original = std::fs::read_to_string(file).unwrap_or_default();
    let turtle = file
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("ttl"));
    let text = if outcomes.is_empty() {
        eprintln!("semops: nothing to repair");
        original
    } else if turtle && outcomes.iter().all(|o| o.kind == Kind::Insert) {
        let added: Vec<_> = outcomes
            .iter()
            .flat_map(|o| o.added.iter().cloned())
            .collect();
        let mut text = original;
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&repair::render_added(&added, &prefixes));
        text
    } else {
        eprintln!(
            "semops: a repair removed triples, so the document is written out whole, without \
             its comments or layout"
        );
        let mut serializer = oxrdfio::RdfSerializer::from_format(oxrdfio::RdfFormat::Turtle);
        for (prefix, namespace) in &prefixes {
            serializer = serializer.with_prefix(prefix, namespace)?;
        }
        let mut writer = serializer.for_writer(Vec::new());
        for triple in &working {
            writer.serialize_triple(triple)?;
        }
        String::from_utf8(writer.finish()?)?
    };
    let inserts = outcomes.iter().filter(|o| o.kind == Kind::Insert).count();
    eprintln!(
        "{} repairs: {inserts} insert, {} replace",
        outcomes.len(),
        outcomes.len() - inserts
    );
    match target {
        Some(path) => {
            std::fs::write(&path, &text).with_context(|| format!("writing {}", path.display()))?;
        }
        None => std::io::stdout().lock().write_all(text.as_bytes())?,
    }
    Ok(ExitCode::SUCCESS)
}

fn run_check(command: Command) -> Result<ExitCode> {
    match command {
        Command::Check {
            files,
            format,
            fail_on,
            engine,
            out,
            list,
        } => {
            if list {
                return list_checks().map(|()| ExitCode::SUCCESS);
            }
            check(
                &files,
                Options {
                    format,
                    fail_on,
                    out,
                    engine: engine.into(),
                },
            )
        }
        _ => unreachable!("run_cli passes only `check` here"),
    }
}

fn list_checks() -> Result<()> {
    let registry = semops_checks::Registry::builtin();
    let mut stdout = std::io::stdout().lock();
    for check in registry.checks() {
        writeln!(
            stdout,
            "{:<8} {:<10} {:<12} {}",
            check.id, check.default_severity, check.category, check.title
        )?;
    }
    writeln!(
        stdout,
        "{} checks, from {}",
        registry.checks().len(),
        semops_checks::SUITE
    )?;
    Ok(())
}

fn check(files: &[PathBuf], options: Options) -> Result<ExitCode> {
    let Options {
        format,
        fail_on,
        out,
        engine,
    } = options;
    if files.is_empty() {
        bail!("no files to check");
    }
    let paths: Vec<&std::path::Path> = files.iter().map(PathBuf::as_path).collect();
    let triples = run::load(&paths)?;
    let registry = semops_checks::Registry::builtin();
    let report = run::check_with(&triples, registry, &semops_checks::queries(), engine)?;

    // The rank at or below which a finding fails the run (0 Violation, 1 Warning, 2 Info).
    let threshold = match fail_on {
        FailOn::Violation => Some(0),
        FailOn::Warning => Some(1),
        FailOn::Info => Some(2),
        FailOn::Never => None,
    };
    let mut buffer = Vec::new();
    match format {
        Format::Text => write_text(&mut buffer, &report)?,
        Format::Json => {
            serde_json::to_writer_pretty(&mut buffer, &report.findings)?;
            buffer.push(b'\n');
        }
        Format::Csv => write_csv(&mut buffer, &report.findings)?,
        Format::Junit => {
            buffer.extend_from_slice(reports::junit(&report, registry, threshold, rank).as_bytes());
        }
        Format::Sarif => {
            serde_json::to_writer_pretty(&mut buffer, &reports::sarif(&report, registry, files))?;
            buffer.push(b'\n');
        }
    }
    match out {
        Some(path) => {
            std::fs::write(&path, &buffer).with_context(|| format!("writing {}", path.display()))?
        }
        None => std::io::stdout().lock().write_all(&buffer)?,
    }

    let failed: Vec<_> = report
        .queries
        .iter()
        .filter(|q| q.error.is_some())
        .collect();
    for query in &failed {
        eprintln!(
            "semops: check {} did not run ({}): {}",
            query.id,
            query.path,
            query.error.as_deref().unwrap_or_default()
        );
    }
    if !failed.is_empty() {
        return Ok(ExitCode::from(2));
    }
    let gated = threshold.is_some_and(|threshold| {
        report
            .findings
            .iter()
            .any(|f| rank(&f.severity) <= threshold)
    });
    Ok(if gated {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

fn write_text(out: &mut impl Write, report: &Run) -> Result<()> {
    for f in &report.findings {
        writeln!(
            out,
            "{:<9} {:<8} {}  {}",
            f.severity,
            f.check_id.as_deref().unwrap_or("UNMAPPED"),
            if f.focus_is_blank {
                "[a blank node]"
            } else {
                &f.focus_node
            },
            f.message
        )?;
    }
    let count = |s: &str| report.findings.iter().filter(|f| f.severity == s).count();
    writeln!(
        out,
        "{} findings: {} Violation, {} Warning, {} Info",
        report.findings.len(),
        count("Violation"),
        count("Warning"),
        count("Info"),
    )?;
    Ok(())
}

/// The suite's `full_results.csv`, column for column, so whatever reads that reads this.
fn write_csv(out: &mut impl Write, findings: &[Finding]) -> Result<()> {
    let mut writer = csv::Writer::from_writer(out);
    writer.write_record([
        "check_id",
        "category",
        "title",
        "severity",
        "declared_severity",
        "focus_node",
        "path",
        "value",
        "message",
        "remediation",
        "sources",
        "source_file",
        "line",
    ])?;
    for f in findings {
        writer.write_record([
            f.check_id.as_deref().unwrap_or("UNMAPPED"),
            f.category.as_deref().unwrap_or("unmapped"),
            f.title.as_deref().unwrap_or(""),
            &f.severity,
            "",
            &f.focus_node,
            f.path.as_deref().unwrap_or(""),
            f.value.as_deref().unwrap_or(""),
            &f.message,
            f.remediation.as_deref().unwrap_or(""),
            &f.sources.join("+"),
            "",
            "",
        ])?;
    }
    writer.flush()?;
    Ok(())
}
