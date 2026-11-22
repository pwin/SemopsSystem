//! `semops_system._native`: the one extension module of the `semops-system` wheel.
//!
//! It carries the three engines' Python modules as submodules, built from the same commit of
//! the workspace, and the ontology quality checks:
//!
//! | Submodule | Is the module of |
//! | --- | --- |
//! | `_holosdb` | `holosdb`: the store, SPARQL 1.2, policy, GeoSPARQL and two SHACL validators |
//! | `shacl` | `shacl`: the SHACL engine |
//! | `tarka_rdf` | `tarka-rdf`: mapping tables and data frames to RDF |
//!
//! `semops_system/__init__.py` presents them as `semops_system.holos`, `.shacl` and
//! `.tarka`, the names the bridge release used, so code written against it keeps working.

use oxrdf::Triple;
use pyo3::exceptions::{PyIOError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use pyo3::wrap_pymodule;
use semops_checks::run::{self, Engine, Run};
use std::path::Path;

fn engine(name: &str) -> PyResult<Engine> {
    match name {
        "both" => Ok(Engine::Both),
        "sparql" => Ok(Engine::Sparql),
        "shacl" => Ok(Engine::Shacl),
        other => Err(PyValueError::new_err(format!(
            "unknown engine {other:?}: expected 'both', 'sparql' or 'shacl'"
        ))),
    }
}

/// A run as Python sees it: a dict of `findings` (dicts), `failed` (the queries that did
/// not run, as dicts of `check_id` and `error`) and `complete`.
fn to_python<'py>(py: Python<'py>, outcome: Run) -> PyResult<Bound<'py, PyDict>> {
    let complete = outcome.complete();
    let findings = PyList::empty(py);
    for f in outcome.findings {
        let d = PyDict::new(py);
        d.set_item("check_id", f.check_id)?;
        d.set_item("category", f.category)?;
        d.set_item("title", f.title)?;
        d.set_item("severity", f.severity)?;
        d.set_item("focus_node", f.focus_node)?;
        d.set_item("focus_is_blank", f.focus_is_blank)?;
        d.set_item("path", f.path)?;
        d.set_item("value", f.value)?;
        d.set_item("message", f.message)?;
        d.set_item("remediation", f.remediation)?;
        d.set_item("sources", f.sources)?;
        findings.append(d)?;
    }
    let failed = PyList::empty(py);
    for q in outcome.queries {
        if let Some(error) = q.error {
            let d = PyDict::new(py);
            d.set_item("check_id", q.id)?;
            d.set_item("error", error)?;
            failed.append(d)?;
        }
    }
    let result = PyDict::new(py);
    result.set_item("findings", findings)?;
    result.set_item("failed", failed)?;
    result.set_item("complete", complete)?;
    Ok(result)
}

fn run_checks<'py>(
    py: Python<'py>,
    triples: &[Triple],
    engine_name: &str,
) -> PyResult<Bound<'py, PyDict>> {
    let engine = engine(engine_name)?;
    // The checks hold no Python objects, so other threads may run while they do.
    let outcome = py
        .detach(|| {
            run::check_with(
                triples,
                semops_checks::Registry::builtin(),
                &semops_checks::queries(),
                engine,
            )
        })
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    to_python(py, outcome)
}

/// Runs the ontology quality checks over RDF files, read into one graph, as `semops check`
/// does.
///
/// `engine` is `"both"` (the default: the shapes and the queries, merged), `"sparql"` or
/// `"shacl"`. Returns a dict: `findings`, a list of dicts (`check_id`, `category`,
/// `title`, `severity`, `focus_node`, `focus_is_blank`, `path`, `value`, `message`,
/// `remediation`, `sources`); `failed`, the checks whose query did not run; and
/// `complete`, false when there are any.
#[pyfunction]
#[pyo3(signature = (*paths, engine = "both"))]
fn check<'py>(py: Python<'py>, paths: Vec<String>, engine: &str) -> PyResult<Bound<'py, PyDict>> {
    if paths.is_empty() {
        return Err(PyValueError::new_err("check() needs at least one file"));
    }
    let paths: Vec<&Path> = paths.iter().map(Path::new).collect();
    let triples = run::load(&paths).map_err(|e| PyIOError::new_err(e.to_string()))?;
    run_checks(py, &triples, engine)
}

/// The quality checks over RDF text rather than files. `format` is a media type or a file
/// extension (`"text/turtle"`, `"ttl"`, `"nt"`, `"rdf"`, `"jsonld"` and the rest).
#[pyfunction]
#[pyo3(signature = (text, format = "ttl", engine = "both", base = None))]
fn check_text<'py>(
    py: Python<'py>,
    text: &str,
    format: &str,
    engine: &str,
    base: Option<&str>,
) -> PyResult<Bound<'py, PyDict>> {
    let rdf_format = oxrdfio::RdfFormat::from_extension(format)
        .or_else(|| oxrdfio::RdfFormat::from_media_type(format))
        .ok_or_else(|| PyValueError::new_err(format!("unknown RDF format {format:?}")))?;
    let mut parser = oxrdfio::RdfParser::from_format(rdf_format);
    if let Some(base) = base {
        parser = parser
            .with_base_iri(base)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
    }
    let triples = parser
        .for_slice(text.as_bytes())
        .map(|quad| quad.map(Triple::from))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    run_checks(py, &triples, engine)
}

/// The registry's checks, in its order, as dicts: `id`, `category`, `metric`,
/// `default_severity`, `title`, `description`, `remediation`.
#[pyfunction]
fn list_checks(py: Python<'_>) -> PyResult<Bound<'_, PyList>> {
    let list = PyList::empty(py);
    for c in semops_checks::Registry::builtin().checks() {
        let d = PyDict::new(py);
        d.set_item("id", &c.id)?;
        d.set_item("category", &c.category)?;
        d.set_item("metric", &c.metric)?;
        d.set_item("default_severity", &c.default_severity)?;
        d.set_item("title", &c.title)?;
        d.set_item("description", &c.description)?;
        d.set_item("remediation", &c.remediation)?;
        list.append(d)?;
    }
    Ok(list)
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_wrapped(wrap_pymodule!(holosdb::_holosdb))?;
    m.add_wrapped(wrap_pymodule!(shacl_py::shacl))?;
    m.add_wrapped(wrap_pymodule!(tarka_py::tarka_py))?;
    m.add_function(wrap_pyfunction!(check, m)?)?;
    m.add_function(wrap_pyfunction!(check_text, m)?)?;
    m.add_function(wrap_pyfunction!(list_checks, m)?)?;
    Ok(())
}
