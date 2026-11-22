//! SemOps System for WebAssembly.
//!
//! One module holds the toolchain a browser or Node can run:
//!
//! - HOLOS's in-memory store and SPARQL 1.2 engine: `Store`, from holos-wasm.
//! - The SHACL engine: `Validator`, `Report`, `validateTurtle` and `rdfsClosure`, from
//!   shacl-wasm.
//! - The 61 ontology quality checks: [`check`] and [`list_checks`] (`listChecks`), which run
//!   on both engines, as `semops check` does.
//!
//! The classes and functions of holos-wasm and shacl-wasm keep their names and signatures,
//! so code written against either moves here by changing its import. The two engines share
//! one copy of the RDF model, parsers and SPARQL evaluator, where two packages carried two.

use oxrdf::Triple;
use oxrdfio::{RdfFormat, RdfParser};
use semops_checks::run::{self, Engine};
use serde::Serialize;
use wasm_bindgen::prelude::*;

pub use holos_wasm::Store;
pub use shacl_wasm::{Report, Validator, rdfs_closure_turtle, validate_turtle_once};

/// Installs the panic hook, for both engines, when the module is instantiated.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// The SemOps System release this module was built from.
#[wasm_bindgen]
#[must_use]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

fn error(message: impl std::fmt::Display) -> JsValue {
    JsError::new(&message.to_string()).into()
}

/// The RDF syntax a name stands for: the names holos-wasm and the Python bindings accept.
fn format_by_name(name: &str) -> Result<RdfFormat, JsValue> {
    Ok(match name.to_ascii_lowercase().as_str() {
        "turtle" | "ttl" => RdfFormat::Turtle,
        "ntriples" | "nt" | "n-triples" => RdfFormat::NTriples,
        "trig" => RdfFormat::TriG,
        "nquads" | "nq" | "n-quads" => RdfFormat::NQuads,
        "rdfxml" | "rdf" | "xml" => RdfFormat::RdfXml,
        "n3" => RdfFormat::N3,
        other => {
            return Err(error(format!(
                "unknown RDF format {other:?}; expected turtle, ntriples, trig, nquads, \
                 rdfxml or n3"
            )));
        }
    })
}

fn engine_by_name(name: &str) -> Result<Engine, JsValue> {
    match name {
        "both" => Ok(Engine::Both),
        "sparql" => Ok(Engine::Sparql),
        "shacl" => Ok(Engine::Shacl),
        other => Err(error(format!(
            "unknown engine {other:?}; expected both, sparql or shacl"
        ))),
    }
}

/// A finding, as JavaScript sees it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsFinding {
    check_id: Option<String>,
    category: Option<String>,
    title: Option<String>,
    severity: String,
    focus_node: String,
    focus_is_blank: bool,
    path: Option<String>,
    value: Option<String>,
    message: String,
    remediation: Option<String>,
    sources: Vec<String>,
}

/// A portable query that did not run. Its check found nothing, which is not the same as
/// passing, so a caller gating on the result has to see it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsFailure {
    check_id: String,
    error: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsRun {
    findings: Vec<JsFinding>,
    failed: Vec<JsFailure>,
    complete: bool,
}

/// Runs the ontology quality checks over RDF text: an ontology, its data, or both in one
/// document.
///
/// `format` defaults to `turtle`; `engine` is `both` (the default, both formulations
/// merged), `sparql` (the portable queries on HOLOS) or `shacl` (the shapes on the SHACL
/// engine, which misses the checks that have only a query); `base` resolves relative IRIs.
///
/// Returns `{ findings, failed, complete }`. Each finding carries `checkId`, `category`,
/// `title`, `severity` (`Violation`, `Warning` or `Info`), `focusNode`, `focusIsBlank`,
/// `path`, `value`, `message`, `remediation` and `sources`. `failed` lists the queries that
/// did not run, and `complete` is false when there are any.
///
/// # Errors
///
/// If the text does not parse, or `format` or `engine` is not one of the names above.
#[wasm_bindgen]
pub fn check(
    text: &str,
    format: Option<String>,
    engine: Option<String>,
    base: Option<String>,
) -> Result<JsValue, JsValue> {
    let format = format_by_name(format.as_deref().unwrap_or("turtle"))?;
    let engine = engine_by_name(engine.as_deref().unwrap_or("both"))?;
    let mut parser = RdfParser::from_format(format);
    if let Some(base) = base {
        parser = parser.with_base_iri(&base).map_err(error)?;
    }
    let triples = parser
        .for_slice(text.as_bytes())
        .map(|quad| quad.map(Triple::from))
        .collect::<Result<Vec<_>, _>>()
        .map_err(error)?;
    let outcome = run::check_with(
        &triples,
        semops_checks::Registry::builtin(),
        &semops_checks::queries(),
        engine,
    )
    .map_err(error)?;
    let complete = outcome.complete();
    let result = JsRun {
        findings: outcome
            .findings
            .into_iter()
            .map(|f| JsFinding {
                check_id: f.check_id,
                category: f.category,
                title: f.title,
                severity: f.severity,
                focus_node: f.focus_node,
                focus_is_blank: f.focus_is_blank,
                path: f.path,
                value: f.value,
                message: f.message,
                remediation: f.remediation,
                sources: f.sources,
            })
            .collect(),
        failed: outcome
            .queries
            .into_iter()
            .filter_map(|q| {
                q.error.map(|error| JsFailure {
                    check_id: q.id,
                    error,
                })
            })
            .collect(),
        complete,
    };
    serde_wasm_bindgen::to_value(&result).map_err(error)
}

/// A check in the registry, as JavaScript sees it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsCheck<'a> {
    id: &'a str,
    category: &'a str,
    metric: &'a str,
    default_severity: &'a str,
    title: &'a str,
    description: &'a str,
    remediation: &'a str,
}

/// The registry's checks, in its order: `{ id, category, metric, defaultSeverity, title,
/// description, remediation }` each.
///
/// # Errors
///
/// Never in practice: only if the registry cannot be turned into JavaScript values.
#[wasm_bindgen(js_name = listChecks)]
pub fn list_checks() -> Result<JsValue, JsValue> {
    let checks: Vec<JsCheck<'_>> = semops_checks::Registry::builtin()
        .checks()
        .iter()
        .map(|c| JsCheck {
            id: &c.id,
            category: &c.category,
            metric: &c.metric,
            default_severity: &c.default_severity,
            title: &c.title,
            description: &c.description,
            remediation: &c.remediation,
        })
        .collect();
    serde_wasm_bindgen::to_value(&checks).map_err(error)
}
