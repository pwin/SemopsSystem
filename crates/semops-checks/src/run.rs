//! Runs the registry's checks natively: the portable queries on HOLOS, the shapes on the
//! SHACL engine, or both.
//!
//! The Python suite's `run_registry_suite_on_graph`. Each check has two formulations, a SHACL
//! shape and a portable SPARQL `CONSTRUCT` query, and [`Engine`] picks which run:
//!
//! - [`Engine::Sparql`], which the suite's walkthrough recommends for CI: every
//!   `sparql/**/*.rq` query but the subject-specific ones, on HOLOS's engine.
//! - [`Engine::Shacl`]: the shapes, compiled and run by the SHACL engine.
//! - [`Engine::Both`], the suite's default: both, so a check that fires from one formulation
//!   and not the other shows up as a finding with one source rather than two.
//!
//! Every shape has a portable twin, so [`Engine::Sparql`] and [`Engine::Both`] find the same
//! things and differ only in which formulation's message a finding keeps (the shape's, when
//! both report it). Not every query has a shape, so [`Engine::Shacl`] alone finds less: 54
//! findings against 64 on the suite's `domain.ttl`, as the suite's own run does. The literal
//! check runs under every engine, since it covers what neither formulation can see. The
//! findings are merged and ordered as the suite merges and orders them (`checks/merge.py`),
//! and `tests/parity.rs` holds each engine to the suite's own output.
//!
//! One difference is deliberate. The suite parses with rdflib, which rewrites some literals
//! as it parses (`"yesplz"^^xsd:boolean` becomes `"false"`), so a finding about such a
//! literal quotes the rewrite, or is lost. Here the graph is read as written, and the finding
//! quotes what the author wrote.

mod literal;
mod merge;
mod shacl;

pub use merge::Finding;
pub use shacl::shapes_graph;

use holos_engine::{Engine as Holos, QueryOptions};
use holos_security::Session;
use oxrdf::{BlankNode, Graph, GraphName, Quad, Term, Triple};
use spareval::QueryResults;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

/// Why a run could not start. A query that fails during a run does not stop it: see
/// [`QueryOutcome`].
#[derive(Debug)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// Which formulation of the checks runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Engine {
    /// The portable SPARQL queries, on HOLOS.
    Sparql,
    /// The SHACL shapes, on the SHACL engine.
    Shacl,
    /// Both, merged: the suite's default.
    #[default]
    Both,
}

/// How one check query went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryOutcome {
    /// The check id the query reports, from its file name.
    pub id: String,
    pub path: String,
    /// The engine's message, if the query did not run.
    pub error: Option<String>,
    /// Triples the query constructed.
    pub triples: usize,
}

/// A finished run.
#[derive(Clone, Debug)]
pub struct Run {
    /// The findings, merged and ordered: by severity, then category, then check id, then
    /// focus node.
    pub findings: Vec<Finding>,
    /// One per query run, in the order they ran; empty under [`Engine::Shacl`].
    pub queries: Vec<QueryOutcome>,
}

impl Run {
    /// Whether every query ran. A query that errors finds nothing, which looks exactly like
    /// a clean graph, so a caller gating a build should check this too.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.queries.iter().all(|q| q.error.is_none())
    }
}

/// `triples` with every blank node replaced by a fresh one, consistently: what a parser does
/// per document, so two documents' `_:b0` stay two nodes once their triples share a graph.
pub(crate) fn relabel(triples: Vec<Triple>) -> Vec<Triple> {
    let mut fresh: HashMap<BlankNode, BlankNode> = HashMap::new();
    let mut node = |b: BlankNode| fresh.entry(b).or_default().clone();
    triples
        .into_iter()
        .map(|t| {
            let subject = match t.subject {
                oxrdf::NamedOrBlankNode::BlankNode(b) => node(b).into(),
                named => named,
            };
            let object = match t.object {
                Term::BlankNode(b) => node(b).into(),
                other => other,
            };
            Triple::new(subject, t.predicate, object)
        })
        .collect()
}

/// Reads RDF files into one graph, the format from each file's extension (`.gz` too). Each
/// file's blank nodes are its own.
///
/// # Errors
///
/// If a file cannot be read or parsed, or its format cannot be told from its name.
pub fn load(paths: &[&Path]) -> Result<Vec<Triple>, Error> {
    let mut triples = Vec::new();
    for path in paths {
        let (format, reader) = holos_engine::source::open(path)
            .map_err(|e| Error(format!("{}: {e}", path.display())))?;
        let base = format!("file:///{}", path.display()).replace('\\', "/");
        let mut parser = oxrdfio::RdfParser::from_format(format);
        if let Ok(with_base) = parser.clone().with_base_iri(&base) {
            parser = with_base;
        }
        let mut document = Vec::new();
        for quad in parser.for_reader(reader) {
            let quad = quad.map_err(|e| Error(format!("{}: {e}", path.display())))?;
            document.push(Triple::from(quad));
        }
        triples.extend(relabel(document));
    }
    Ok(triples)
}

/// Runs the checks over `triples` with [`Engine::Both`], against the embedded registry.
///
/// # Errors
///
/// If the graph cannot be put in a store, or the shapes do not compile. Queries that fail
/// are reported in [`Run::queries`], not as an error.
pub fn check(triples: &[Triple]) -> Result<Run, Error> {
    check_with(
        triples,
        crate::Registry::builtin(),
        &crate::queries(),
        Engine::Both,
    )
}

/// [`check`], with a given registry, query set and engine.
///
/// # Errors
///
/// As [`check`].
pub fn check_with(
    triples: &[Triple],
    registry: &crate::Registry,
    queries: &[crate::Query],
    engine: Engine,
) -> Result<Run, Error> {
    let shacl = if engine == Engine::Sparql {
        Vec::new()
    } else {
        shacl::check(triples, registry)?
    };
    let (sparql, outcomes) = if engine == Engine::Shacl {
        (Vec::new(), Vec::new())
    } else {
        portable(triples, registry, queries)?
    };
    let literals = merge::extract(&literal::check(triples), registry, literal::SOURCE);
    Ok(Run {
        findings: merge::unify(vec![shacl, sparql, literals]),
        queries: outcomes,
    })
}

/// The portable arm: every query on HOLOS, its results as findings.
fn portable(
    triples: &[Triple],
    registry: &crate::Registry,
    queries: &[crate::Query],
) -> Result<(Vec<Finding>, Vec<QueryOutcome>), Error> {
    let mut store = holos_store::Store::new();
    for triple in triples {
        store
            .insert(
                Quad::new(
                    triple.subject.clone(),
                    triple.predicate.clone(),
                    triple.object.clone(),
                    GraphName::DefaultGraph,
                )
                .as_ref(),
            )
            .map_err(|e| Error(format!("loading the graph: {e}")))?;
    }
    let engine = Holos::with_store(store);
    let session = Session::unrestricted(engine.store())
        .map_err(|e| Error(format!("opening a session: {e}")))?;
    let view = engine.view(&session);

    let mut results = Graph::new();
    let mut outcomes = Vec::new();
    for query in queries.iter().filter(|q| !q.subject_specific) {
        let mut outcome = QueryOutcome {
            id: query.id.to_owned(),
            path: query.path.to_owned(),
            error: None,
            triples: 0,
        };
        match construct(&view, query.text) {
            // The blank nodes a template mints are relabelled per query, so two queries'
            // results can never share a node whatever labels the engine chose.
            Ok(triples) => {
                outcome.triples = triples.len();
                for triple in relabel(triples) {
                    results.insert(&triple);
                }
            }
            Err(e) => outcome.error = Some(e),
        }
        outcomes.push(outcome);
    }
    Ok((merge::extract(&results, registry, "sparql"), outcomes))
}

fn construct(view: &holos_engine::DatasetView<'_>, query: &str) -> Result<Vec<Triple>, String> {
    let (results, _) =
        Holos::query_with(view, query, &QueryOptions::new()).map_err(|e| e.to_string())?;
    match results {
        QueryResults::Graph(triples) => triples.map(|t| t.map_err(|e| e.to_string())).collect(),
        _ => Err("not a CONSTRUCT query".to_owned()),
    }
}
