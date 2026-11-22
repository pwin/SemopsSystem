//! `semops`: the SemOps System command.
//!
//! One subcommand so far, `check`, which runs the ontology quality checks natively: the
//! registry and its queries are compiled in (`semops-checks`), and the checks run on HOLOS's
//! engine, with no Python and no Java. The other subcommands the proposal names (`serve`,
//! `load`, `query`, `validate`, `map`, …) are still the `holos-server`, `holos`, `shacl` and
//! `tarka` commands.

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use semops_checks::run::{self, Finding, Run};
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "semops", version, about = "The SemOps System command")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the ontology quality checks over an ontology and its data.
    ///
    /// The files are read into one graph, so declarations in the ontology are checked
    /// against their use in the data and the other way round. Every check runs in its
    /// portable SPARQL formulation, as `ontology-suite check --engine sparql` runs it, with
    /// the same findings.
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
    match run_cli() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("semops: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run_cli() -> Result<ExitCode> {
    match Cli::parse().command {
        Command::Check {
            files,
            format,
            fail_on,
            out,
            list,
        } => {
            if list {
                return list_checks().map(|()| ExitCode::SUCCESS);
            }
            check(&files, format, fail_on, out)
        }
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

fn check(
    files: &[PathBuf],
    format: Format,
    fail_on: FailOn,
    out: Option<PathBuf>,
) -> Result<ExitCode> {
    if files.is_empty() {
        bail!("no files to check");
    }
    let paths: Vec<&std::path::Path> = files.iter().map(PathBuf::as_path).collect();
    let triples = run::load(&paths)?;
    let report = run::check(&triples)?;

    let mut buffer = Vec::new();
    match format {
        Format::Text => write_text(&mut buffer, &report)?,
        Format::Json => {
            serde_json::to_writer_pretty(&mut buffer, &report.findings)?;
            buffer.push(b'\n');
        }
        Format::Csv => write_csv(&mut buffer, &report.findings)?,
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
    let gated = fail_on != FailOn::Never && {
        let threshold = match fail_on {
            FailOn::Violation => 0,
            FailOn::Warning => 1,
            _ => 2,
        };
        report
            .findings
            .iter()
            .any(|f| rank(&f.severity) <= threshold)
    };
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
        "{} findings: {} Violation, {} Warning, {} Info ({} checks ran)",
        report.findings.len(),
        count("Violation"),
        count("Warning"),
        count("Info"),
        report.queries.len()
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
