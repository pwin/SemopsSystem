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

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use semops_checks::run::{self, Finding, Run};
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// holos-server's counting allocator, the one file of `unsafe` in the workspace, compiled
/// into this binary as it is into holos-server's.
#[path = "../../../holos/crates/holos-server/src/alloc.rs"]
mod alloc;
mod reports;

/// Counting, so `semops serve` enforces `--max-query-memory` as holos-server does, around
/// mimalloc, so `semops validate` parses as fast as `shacl` does.
#[global_allocator]
static ALLOCATOR: alloc::Counting<mimalloc::MiMalloc> = alloc::Counting(mimalloc::MiMalloc);

#[derive(Parser)]
#[command(name = "semops", version, about = "The SemOps System command")]
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
    /// Checkpoint a store (holos backup).
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

fn main() -> ExitCode {
    // The allocator above is installed by the attribute; this tells the engine the counter
    // is live, so a memory ceiling is enforced rather than silently inert.
    holos_engine::memory::mark_tracking();
    let args: Vec<OsString> = std::env::args_os().collect();
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
    let command = Cli::parse_from(args).command;
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
        check @ Command::Check { .. } => return run_check(check),
    };
    Ok(forward(forwarded.0, forwarded.1))
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
