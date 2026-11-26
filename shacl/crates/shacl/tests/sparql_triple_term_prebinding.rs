//! A pre-bound `$this` inside a triple term is the focus node, like anywhere else.
//!
//! SHACL-SPARQL pre-binds `$this` by substituting the focus node into the query. The
//! substitution reached every variable position but the inside of an RDF 1.2 triple-term
//! pattern, `<<( $this ?p ?o )>>`, where `$this` stayed a free variable. So the pattern
//! matched every reified statement whatever its subject: each focus node picked up the other
//! people's statements as well as its own, and was reported once per match.
//!
//! The example is the one the bug was reported with: two people, each with a source for
//! their birth date, and person 1's date changed afterwards, so their reified statement
//! no longer holds. The query asks for focus nodes with a reified statement that no longer
//! holds. Before the fix it answered person 1 twice and person 2; the answer is person 1.
#![cfg(feature = "rdf-12")]

use oxrdf::{NamedNode, Term};
use oxrdfio::RdfFormat;
use shacl::model::{Graph, GraphBuilder, TermStore, Vocab, loader};
use shacl::shapes::Shapes;

const DATA: &str = r#"
@prefix ex: <http://example.org/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

ex:person1 ex:born "1901-02-03"^^xsd:date .
ex:person2 ex:born "1910-05-05"^^xsd:date .

# Each source is about the date it saw. Person 1's has since been corrected.
ex:source1 rdf:reifies <<( ex:person1 ex:born "1900-12-31"^^xsd:date )>> ;
    ex:citation "parish register" .
ex:source2 rdf:reifies <<( ex:person2 ex:born "1910-05-05"^^xsd:date )>> ;
    ex:citation "census" .

# A note on what source 1 says: a triple term inside a triple term.
ex:note rdf:reifies <<( ex:source1 ex:about <<( ex:person1 ex:born "1900-12-31"^^xsd:date )>> )>> .
"#;

fn shapes(query: &str) -> String {
    format!(
        r#"
@prefix ex: <http://example.org/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
ex:SourcedStatements a sh:NodeShape ;
    sh:targetSubjectsOf ex:born ;
    sh:sparql [
        sh:message "a source is about a statement that no longer holds" ;
        sh:select """{query}"""
    ] .
"#
    )
}

fn parse(text: &str, scope: u32, store: &mut TermStore) -> Graph {
    let mut b = GraphBuilder::new();
    loader::parse_str(text, RdfFormat::Turtle, "http://t/", scope, store, &mut b)
        .expect("fixture should parse");
    b.build()
}

/// The focus node of every result, in report order.
fn focus_nodes(query: &str) -> Vec<Term> {
    let mut store = TermStore::new();
    let vocab = Vocab::new(&mut store);
    let data = parse(DATA, 0, &mut store);
    let shapes_graph = parse(&shapes(query), 1, &mut store);
    let compiled = Shapes::compile(&shapes_graph, &store, &vocab).expect("shapes compile");
    let report = shacl::validate::validate_in(&data, &compiled, &shapes_graph, &mut store, &vocab)
        .expect("validation runs");
    report
        .results
        .iter()
        .map(|r| store.to_oxrdf(r.focus_node))
        .collect()
}

fn person(n: u32) -> Term {
    NamedNode::new_unchecked(format!("http://example.org/person{n}")).into()
}

const REIFIES: &str = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies>";

/// The query as reported.
#[test]
fn this_inside_a_triple_term_is_the_focus_node() {
    let query = format!(
        "SELECT $this WHERE {{ ?r {REIFIES} <<( $this ?p ?o )>> . FILTER NOT EXISTS {{ $this ?p ?o }} }}"
    );
    assert_eq!(focus_nodes(&query), [person(1)]);
}

/// The workaround the shapes used meanwhile, binding the subject and comparing, gives the
/// same answer: the fix makes the direct form agree with it.
#[test]
fn the_workaround_agrees() {
    let query = format!(
        "SELECT $this WHERE {{ ?r {REIFIES} <<( ?subject ?p ?o )>> . \
         FILTER(?subject = $this) FILTER NOT EXISTS {{ $this ?p ?o }} }}"
    );
    assert_eq!(focus_nodes(&query), [person(1)]);
}

/// Nested one level deeper, as the object of a triple term, `$this` is still substituted:
/// only person 1 has a note on a source about them. Left free, the nested `$this` matched
/// person 1's statement for person 2 as well.
#[test]
fn this_is_substituted_at_any_depth() {
    let query = format!(
        "SELECT $this WHERE {{ \
         ?note {REIFIES} <<( ?source <http://example.org/about> <<( $this ?p ?o )>> )>> }}"
    );
    assert_eq!(focus_nodes(&query), [person(1)]);
}

/// An ASK validator, the other form a SPARQL-based constraint takes, through a custom
/// component: `$this` in a triple term is the focus node there too.
#[test]
fn an_ask_validator_sees_the_focus_node_inside_a_triple_term() {
    let shapes_graph = format!(
        r#"
@prefix ex: <http://example.org/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
ex:StillHolds a sh:ConstraintComponent ;
    sh:parameter [ sh:path ex:stillHolds ] ;
    sh:validator [ a sh:SPARQLAskValidator ;
        sh:ask """ASK {{ FILTER NOT EXISTS {{ ?r {REIFIES} <<( $this ?p ?o )>> . FILTER NOT EXISTS {{ $this ?p ?o }} }} }}""" ] .
ex:S a sh:NodeShape ; sh:targetSubjectsOf ex:born ; ex:stillHolds true .
"#
    );
    let mut store = TermStore::new();
    let vocab = Vocab::new(&mut store);
    let data = parse(DATA, 0, &mut store);
    let shapes = parse(&shapes_graph, 1, &mut store);
    let compiled = Shapes::compile(&shapes, &store, &vocab).expect("shapes compile");
    let report = shacl::validate::validate_in(&data, &compiled, &shapes, &mut store, &vocab)
        .expect("validation runs");
    let focus: Vec<Term> = report
        .results
        .iter()
        .map(|r| store.to_oxrdf(r.focus_node))
        .collect();
    assert_eq!(focus, [person(1)]);
}

/// Blank-node focus nodes are bound by the evaluator's own substitution rather than the
/// algebra rewrite the fix is in. That route was right already; pinned here, so the two keep
/// agreeing. The same people as blank nodes: one result, the one born 1901 whose source saw
/// 1900.
#[test]
fn a_blank_node_focus_inside_a_triple_term_is_the_focus_node() {
    let data_text = r#"
@prefix ex: <http://example.org/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
_:p1 ex:born "1901-02-03"^^xsd:date .
_:p2 ex:born "1910-05-05"^^xsd:date .
ex:source1 rdf:reifies <<( _:p1 ex:born "1900-12-31"^^xsd:date )>> .
ex:source2 rdf:reifies <<( _:p2 ex:born "1910-05-05"^^xsd:date )>> .
"#;
    let query = format!(
        "SELECT $this WHERE {{ ?r {REIFIES} <<( $this ?p ?o )>> . FILTER NOT EXISTS {{ $this ?p ?o }} }}"
    );
    let mut store = TermStore::new();
    let vocab = Vocab::new(&mut store);
    let data = parse(data_text, 0, &mut store);
    let shapes_graph = parse(&shapes(&query), 1, &mut store);
    let compiled = Shapes::compile(&shapes_graph, &store, &vocab).expect("shapes compile");
    let report = shacl::validate::validate_in(&data, &compiled, &shapes_graph, &mut store, &vocab)
        .expect("validation runs");
    assert_eq!(
        report.results.len(),
        1,
        "one result, not one per reified statement"
    );
    let focus = report.results[0].focus_node;
    assert!(matches!(store.to_oxrdf(focus), Term::BlankNode(_)));
    let born = store
        .get_named_node("http://example.org/born")
        .expect("ex:born is interned");
    let dates: Vec<String> = data
        .objects(focus, born)
        .map(|d| store.to_oxrdf(d).to_string())
        .collect();
    assert_eq!(
        dates,
        ["\"1901-02-03\"^^<http://www.w3.org/2001/XMLSchema#date>"]
    );
}

/// The same route through an ASK validator, whose only `$this` is inside the triple term:
/// "every person has a sourced birth date". Person 3 has none and is the one result; a `$this`
/// left free would let person 1's source answer for everybody, and nobody would be flagged.
#[test]
fn an_ask_validator_binds_a_blank_node_focus_inside_a_triple_term() {
    let data_text = r#"
@prefix ex: <http://example.org/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
_:p1 ex:born "1901-02-03"^^xsd:date .
_:p3 ex:born "1920-07-07"^^xsd:date .
ex:source1 rdf:reifies <<( _:p1 ex:born "1901-02-03"^^xsd:date )>> .
"#;
    let shapes_text = format!(
        r#"
@prefix ex: <http://example.org/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
ex:Sourced a sh:ConstraintComponent ;
    sh:parameter [ sh:path ex:sourced ] ;
    sh:validator [ a sh:SPARQLAskValidator ;
        sh:ask """ASK {{ ?r {REIFIES} <<( $this <http://example.org/born> ?date )>> }}""" ] .
ex:S a sh:NodeShape ; sh:targetSubjectsOf ex:born ; ex:sourced true .
"#
    );
    let mut store = TermStore::new();
    let vocab = Vocab::new(&mut store);
    let data = parse(data_text, 0, &mut store);
    let shapes_graph = parse(&shapes_text, 1, &mut store);
    let compiled = Shapes::compile(&shapes_graph, &store, &vocab).expect("shapes compile");
    let report = shacl::validate::validate_in(&data, &compiled, &shapes_graph, &mut store, &vocab)
        .expect("validation runs");
    assert_eq!(report.results.len(), 1, "person 3, and only person 3");
    let focus = report.results[0].focus_node;
    let born = store
        .get_named_node("http://example.org/born")
        .expect("ex:born is interned");
    let dates: Vec<String> = data
        .objects(focus, born)
        .map(|d| store.to_oxrdf(d).to_string())
        .collect();
    assert_eq!(
        dates,
        ["\"1920-07-07\"^^<http://www.w3.org/2001/XMLSchema#date>"]
    );
}
