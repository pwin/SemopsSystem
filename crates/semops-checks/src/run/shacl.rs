//! The SHACL arm: the registry's shapes, compiled and run by the SHACL engine.
//!
//! The suite's `checks/shacl_native_runner.run_shacl_native_rows`, which builds its rows from
//! the same engine's structured results through the Python bindings. Two things there are
//! reconstructed rather than read off a result, and are here too:
//!
//! - **The shape a result belongs to.** A constraint inside `sh:property [ … ]` reports the
//!   nested blank node, which names nothing in anyone else's copy of the shapes graph; the
//!   nearest enclosing shape with an IRI is what a check id resolves from.
//! - **The path.** Read from that shape's own `sh:path` and rendered as the portable arm
//!   renders paths, so a finding from the two arms merges; failing that, the engine's own
//!   SPARQL rendering with the angle brackets taken off, which is the one way the two
//!   notations differ.

use super::merge::{self, Finding};
use super::{Error, relabel};
use oxrdf::{Graph, NamedNode, NamedOrBlankNodeRef, Term, Triple};
use oxrdfio::{RdfFormat, RdfParser};
use shacl::model::{GraphBuilder, scope};
use shacl::{TermId, TermStore, Vocab};

/// The embedded shapes as one graph, each file's blank nodes its own, as a parser gives
/// them per document.
///
/// # Errors
///
/// If a shapes file does not parse, which the crate's tests rule out for the embedded ones.
pub fn shapes_graph() -> Result<Graph, Error> {
    let mut graph = Graph::new();
    for file in crate::shapes() {
        let mut triples = Vec::new();
        for quad in RdfParser::from_format(RdfFormat::Turtle).for_slice(file.text.as_bytes()) {
            let quad = quad.map_err(|e| Error(format!("{}: {e}", file.path)))?;
            triples.push(Triple::from(quad));
        }
        for triple in relabel(triples) {
            graph.insert(&triple);
        }
    }
    Ok(graph)
}

fn intern(
    store: &mut TermStore,
    triples: impl IntoIterator<Item = Triple>,
    scope: u32,
) -> shacl::Graph {
    let mut builder = GraphBuilder::new();
    for t in triples {
        let s = store.intern_oxrdf(Term::from(t.subject).as_ref(), scope);
        let p = store.intern_oxrdf(Term::from(t.predicate).as_ref(), scope);
        let o = store.intern_oxrdf(t.object.as_ref(), scope);
        builder.push(s, p, o);
    }
    builder.build()
}

/// Runs the shapes over `data`, as findings from the `shacl` arm.
///
/// # Errors
///
/// If the shapes do not compile or validation fails to run.
pub fn check(data: &[Triple], registry: &crate::Registry) -> Result<Vec<Finding>, Error> {
    let shapes_ox = shapes_graph()?;
    let mut store = TermStore::new();
    let vocab = Vocab::new(&mut store);
    let shapes = intern(
        &mut store,
        shapes_ox.iter().map(|t| t.into_owned()),
        scope::SHAPES,
    );
    let compiled = shacl::shapes::Shapes::compile(&shapes, &store, &vocab)
        .map_err(|e| Error(format!("compiling the shapes: {e}")))?;
    let data_graph = intern(&mut store, data.iter().cloned(), scope::DATA);
    let report = shacl::validate::validate_in_with(
        &data_graph,
        &compiled,
        &shapes,
        &mut store,
        &vocab,
        shacl::validate::Options {
            max_results: None,
            blocking: None,
            threads: 0,
        },
    )
    .map_err(|e| Error(format!("validating: {e}")))?;

    // The nearest enclosing shape with an IRI, bounded as the bindings bound it: nesting
    // is written by hand, and a malformed graph can hold an `sh:property` cycle.
    let root_of = |mut node: TermId| -> Option<TermId> {
        for _ in 0..16 {
            if store.iri(node).is_some() {
                return Some(node);
            }
            node = shapes.subjects(vocab.sh_property, node).next()?;
        }
        None
    };
    let sh_path = oxrdf::NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#path");

    let mut findings = Vec::with_capacity(report.results.len());
    for r in &report.results {
        let shape = r
            .source_shape
            .and_then(root_of)
            .and_then(|t| store.iri(t))
            .map(NamedNode::new_unchecked);
        let declared = shape
            .as_ref()
            .and_then(|s| shapes_ox.object_for_subject_predicate(s, sh_path));
        let path = match declared {
            Some(declared) => Some(merge::path_expression(&shapes_ox, declared, 0)),
            None => r.path.map(|p| {
                let rendered = match shacl::path::Path::compile(p, &shapes, &store, &vocab) {
                    Ok(path) => path.to_sparql(&store),
                    Err(_) => store.to_oxrdf(p).to_string(),
                };
                rendered.replace(['<', '>'], "")
            }),
        };
        let component = store.iri(r.source_constraint_component).map_or_else(
            || store.to_oxrdf(r.source_constraint_component).to_string(),
            str::to_owned,
        );
        let check_id = registry
            .resolve_in_shapes(
                Some(&component),
                shape.as_ref().map(NamedOrBlankNodeRef::from),
                &shapes_ox,
            )
            .map(str::to_owned);
        let check = check_id.as_deref().and_then(|id| registry.get(id));
        // An IRI bare, a blank node as `_:label`: how the structured results spell them.
        let focus = match store.iri(r.focus_node) {
            Some(iri) => iri.to_owned(),
            None => store.to_oxrdf(r.focus_node).to_string(),
        };
        let focus_is_blank = focus.starts_with("_:");
        let value = r
            .value
            .and_then(|v| store.lexical_form(v))
            .map(str::to_owned)
            .or_else(|| Some(focus.clone()));
        let message = r
            .messages
            .first()
            .and_then(|&m| store.lexical_form(m))
            .unwrap_or_default();
        let severity = store
            .iri(r.severity)
            .map(merge::severity_label)
            .unwrap_or_default();
        findings.push(Finding {
            category: check.map(|c| c.category.clone()),
            title: check.map(|c| c.title.clone()),
            remediation: check.map(|c| c.remediation.clone()),
            check_id,
            severity,
            message: merge::substitute(
                message,
                Some(&focus),
                path.as_deref(),
                value.as_deref(),
                focus_is_blank,
            ),
            focus_node: focus,
            path,
            value,
            sources: vec!["shacl".to_owned()],
            focus_is_blank,
        });
    }
    Ok(findings)
}
