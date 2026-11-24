//! A SPARQL constraint reports `?value` as the query binds it, computed or not.
//!
//! SHACL 5.3.2 gives `sh:value` as "the binding for the variable `value`",
//! with the value node only as the fallback, and puts no condition on where
//! the term came from. The engine used to resolve the binding against the
//! store and drop it when it was not there, so a value the query computed
//! (`BIND`, `COUNT`, `STR()`) and that neither graph happened to hold left the
//! result without `sh:value`, while the message still showed `{?value}`.
//! Reported as SHACL_Engine#3, now pwin/SemopsSystem#24.
//!
//! `?path` has a condition of its own: it counts only "if that is a IRI".

use oxrdf::vocab::xsd;
use oxrdf::{Literal, NamedNode, Term};
use oxrdfio::RdfFormat;
use shacl::model::{Graph, GraphBuilder, TermStore, Vocab, loader};
use shacl::shapes::Shapes;

const PREFIX: &str = "@prefix ex: <http://example.org/> .\n\
                      @prefix sh: <http://www.w3.org/ns/shacl#> .\n";

fn parse(text: &str, scope: u32, store: &mut TermStore) -> Graph {
    let mut b = GraphBuilder::new();
    loader::parse_str(
        &format!("{PREFIX}{text}"),
        RdfFormat::Turtle,
        "http://t/",
        scope,
        store,
        &mut b,
    )
    .expect("fixture should parse");
    b.build()
}

/// One result as the report states it.
#[derive(Debug)]
struct Seen {
    value: Option<Term>,
    path: Option<Term>,
    message: String,
}

/// Validates and returns each result, plus the report serialised as N-Triples.
fn validate(data: &str, shapes: &str) -> (Vec<Seen>, String) {
    let mut store = TermStore::new();
    let vocab = Vocab::new(&mut store);
    let data = parse(data, 0, &mut store);
    let shapes_graph = parse(shapes, 1, &mut store);
    let compiled = Shapes::compile(&shapes_graph, &store, &vocab).expect("shapes should compile");
    let report = shacl::validate::validate_in(&data, &compiled, &shapes_graph, &mut store, &vocab)
        .expect("validation should run");
    let seen = report
        .results
        .iter()
        .map(|r| Seen {
            value: r.value.map(|id| store.to_oxrdf(id)),
            path: r.path.map(|id| store.to_oxrdf(id)),
            message: r
                .messages
                .first()
                .and_then(|&m| store.lexical_form(m))
                .unwrap_or_default()
                .to_owned(),
        })
        .collect();
    let text = report
        .serialize(
            RdfFormat::NTriples,
            &store,
            &vocab,
            &shapes_graph,
            &compiled,
            &[],
        )
        .expect("report should serialise");
    (seen, text)
}

fn only(data: &str, shapes: &str) -> (Seen, String) {
    let (mut seen, text) = validate(data, shapes);
    assert_eq!(seen.len(), 1, "expected one result, got {seen:?}");
    (seen.remove(0), text)
}

fn node_shape(select: &str) -> String {
    format!(
        r#"ex:S a sh:NodeShape ; sh:targetNode ex:a ;
             sh:sparql [ sh:message "value is {{?value}}" ; sh:select """{select}""" ] ."#
    )
}

fn integer(n: i64) -> Term {
    Literal::new_typed_literal(n.to_string(), xsd::INTEGER).into()
}

fn iri(s: &str) -> Term {
    NamedNode::new_unchecked(s).into()
}

const SV: &str = "<http://www.w3.org/ns/shacl#value>";

/// The issue's own example: the same result whether or not the data happens
/// to hold the literal `5`.
#[test]
fn bound_literal_is_reported_whether_or_not_the_data_holds_it() {
    let shapes = node_shape("SELECT $this ?value WHERE { BIND(5 AS ?value) }");
    for data in ["ex:a ex:p ex:b .", "ex:a ex:p 5 ."] {
        let (r, text) = only(data, &shapes);
        assert_eq!(r.value, Some(integer(5)), "data: {data}");
        assert_eq!(r.message, "value is 5", "data: {data}");
        let line = format!("{SV} \"5\"^^<http://www.w3.org/2001/XMLSchema#integer> .");
        assert!(
            text.contains(&line),
            "data: {data}\nno sh:value 5 in:\n{text}"
        );
    }
}

#[test]
fn computed_string_is_reported() {
    let shapes =
        node_shape("SELECT $this ?value WHERE { BIND(CONCAT(\"id:\", STR($this)) AS ?value) }");
    let (r, text) = only("ex:a ex:p ex:b .", &shapes);
    assert_eq!(
        r.value,
        Some(Literal::new_simple_literal("id:http://example.org/a").into())
    );
    assert!(
        text.contains(&format!("{SV} \"id:http://example.org/a\" .")),
        "{text}"
    );
}

#[test]
fn computed_iri_is_reported() {
    let shapes = node_shape(
        "SELECT $this ?value WHERE { BIND(IRI(CONCAT(STR($this), \"/derived\")) AS ?value) }",
    );
    let (r, text) = only("ex:a ex:p ex:b .", &shapes);
    assert_eq!(r.value, Some(iri("http://example.org/a/derived")));
    assert!(
        text.contains(&format!("{SV} <http://example.org/a/derived> .")),
        "{text}"
    );
}

/// An aggregate is the commonest computed value: "has N of these".
#[test]
fn aggregate_is_reported() {
    let shapes = node_shape(
        "SELECT $this (COUNT(?o) AS ?value) WHERE { $this <http://example.org/p> ?o } GROUP BY $this HAVING (COUNT(?o) > 1)",
    );
    let (r, _) = only("ex:a ex:p ex:b, ex:c, ex:d .", &shapes);
    assert_eq!(r.value, Some(integer(3)));
    assert_eq!(r.message, "value is 3");
}

/// The issue names property shapes using `$PATH` as affected too.
#[test]
fn property_shape_with_path_reports_computed_value() {
    let shapes = r#"ex:S a sh:NodeShape ; sh:targetNode ex:a ;
        sh:property [ sh:path ex:p ;
          sh:sparql [ sh:message "length {?value}" ;
            sh:select """SELECT $this ?value WHERE { $this $PATH ?o . BIND(STRLEN(?o) AS ?value) }""" ] ] ."#;
    let (r, _) = only("ex:a ex:p \"abcdefg\" .", shapes);
    assert_eq!(r.value, Some(integer(7)));
    assert_eq!(r.path, Some(iri("http://example.org/p")));
    assert_eq!(r.message, "length 7");
}

/// A value the query only read still resolves to the data's own term.
#[test]
fn value_read_from_the_data_is_unchanged() {
    let shapes = node_shape("SELECT $this ?value WHERE { $this <http://example.org/p> ?value }");
    let (r, _) = only("ex:a ex:p ex:b .", &shapes);
    assert_eq!(r.value, Some(iri("http://example.org/b")));
}

/// Without `?value`, the focus node stands in, as before.
#[test]
fn unbound_value_falls_back_to_the_focus_node() {
    let shapes = node_shape("SELECT $this WHERE { $this <http://example.org/p> ?o }");
    let (r, _) = only("ex:a ex:p ex:b .", &shapes);
    assert_eq!(r.value, Some(iri("http://example.org/a")));
}

/// A computed IRI in `?path` is reported, even though neither graph holds it.
#[test]
fn computed_iri_path_is_reported() {
    let shapes = node_shape(
        "SELECT $this ?value ?path WHERE { $this <http://example.org/p> ?value .          BIND(<http://example.org/elsewhere> AS ?path) }",
    );
    let (r, text) = only("ex:a ex:p ex:b .", &shapes);
    assert_eq!(r.path, Some(iri("http://example.org/elsewhere")));
    assert!(
        text.contains("<http://www.w3.org/ns/shacl#resultPath> <http://example.org/elsewhere> ."),
        "{text}"
    );
}

/// A `?path` that is not an IRI does not count (5.3.2), so the shape's own
/// path stands, and a node shape has none.
#[test]
fn non_iri_path_is_ignored() {
    let select =
        r#"SELECT $this ?value ?path WHERE { $this $PATH ?value . BIND("not a path" AS ?path) }"#;
    let shapes = format!(
        r#"ex:S a sh:NodeShape ; sh:targetNode ex:a ;
             sh:property [ sh:path ex:p ; sh:sparql [ sh:select """{select}""" ] ] ."#
    );
    let (r, _) = only("ex:a ex:p ex:b .", &shapes);
    assert_eq!(r.path, Some(iri("http://example.org/p")));

    let shapes = node_shape(
        r#"SELECT $this ?value ?path WHERE { $this <http://example.org/p> ?value . BIND("not a path" AS ?path) }"#,
    );
    let (r, _) = only("ex:a ex:p ex:b .", &shapes);
    assert_eq!(r.path, None);
}
