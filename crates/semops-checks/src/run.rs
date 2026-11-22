//! Runs the registry's portable checks on HOLOS, natively.
//!
//! The Python suite's `--engine sparql`, which its own walkthrough recommends for CI: every
//! `sparql/**/*.rq` query but the subject-specific ones runs against the graph under test,
//! each `sh:ValidationResult` it constructs becomes a [`Finding`], the literal check adds
//! what no query formulation can see, and the findings are merged and ordered as the suite
//! merges and orders them (`checks/merge.py`). Every check with a SHACL shape has a portable
//! twin, so this finds what the suite finds; `tests/parity.rs` holds it to the suite's own
//! output.
//!
//! One difference is deliberate. The suite parses with rdflib, which rewrites some literals
//! as it parses (`"yesplz"^^xsd:boolean` becomes `"false"`), so a finding about such a
//! literal quotes the rewrite, or is lost. Here the graph is read as written, and the finding
//! quotes what the author wrote.

mod literal;
mod merge;

pub use merge::Finding;

use holos_engine::{Engine, QueryOptions};
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
    /// One per query run, in the order they ran.
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

/// Reads RDF files into one graph, the format from each file's extension (`.gz` too).
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
        for quad in parser.for_reader(reader) {
            let quad = quad.map_err(|e| Error(format!("{}: {e}", path.display())))?;
            triples.push(Triple::from(quad));
        }
    }
    Ok(triples)
}

/// Runs the portable checks over `triples`, against the embedded registry.
///
/// # Errors
///
/// If the graph cannot be put in a store. Queries that fail are reported in
/// [`Run::queries`], not as an error.
pub fn check(triples: &[Triple]) -> Result<Run, Error> {
    check_with(triples, crate::Registry::builtin(), &crate::queries())
}

/// [`check`], with a given registry and query set: a project's checks alongside the suite's.
///
/// # Errors
///
/// As [`check`].
pub fn check_with(
    triples: &[Triple],
    registry: &crate::Registry,
    queries: &[crate::Query],
) -> Result<Run, Error> {
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
    let engine = Engine::with_store(store);
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
            Ok(triples) => {
                // The blank nodes a template mints are relabelled per query, so two queries'
                // results can never share a node whatever labels the engine chose; the
                // nodes of one query keep their links to each other.
                let mut fresh: HashMap<BlankNode, BlankNode> = HashMap::new();
                let mut relabel = |b: &BlankNode| fresh.entry(b.clone()).or_default().clone();
                for triple in triples {
                    let subject = match triple.subject {
                        oxrdf::NamedOrBlankNode::BlankNode(b) => relabel(&b).into(),
                        named => named,
                    };
                    let object = match triple.object {
                        Term::BlankNode(b) => relabel(&b).into(),
                        other => other,
                    };
                    results.insert(&Triple::new(subject, triple.predicate, object));
                    outcome.triples += 1;
                }
            }
            Err(e) => outcome.error = Some(e),
        }
        outcomes.push(outcome);
    }

    let sparql = merge::extract(&results, registry, "sparql");
    let literals = merge::extract(&literal::check(triples), registry, literal::SOURCE);
    Ok(Run {
        findings: merge::unify(vec![sparql, literals]),
        queries: outcomes,
    })
}

fn construct(view: &holos_engine::DatasetView<'_>, query: &str) -> Result<Vec<Triple>, String> {
    let (results, _) =
        Engine::query_with(view, query, &QueryOptions::new()).map_err(|e| e.to_string())?;
    match results {
        QueryResults::Graph(triples) => triples.map(|t| t.map_err(|e| e.to_string())).collect(),
        _ => Err("not a CONSTRUCT query".to_owned()),
    }
}
