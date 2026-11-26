//! A pre-bound `$this` inside an RDF 1.2 triple term, through HOLOS's validator.
//!
//! HOLOS hands SHACL-SPARQL to the `shacl` engine (ADR 0011), whose pre-binding left
//! `$this` free inside `<<( $this ?p ?o )>>`, so the constraint matched other focus nodes'
//! reified statements. The same data and query as `shacl/crates/shacl/tests/
//! sparql_triple_term_prebinding.rs`, from a HOLOS store: person 1, once.

use holos_shacl::{CompiledShapes, Options};
use holos_store::{GraphFilter, Store};
use oxrdfio::{RdfFormat, RdfParser};

const TURTLE: &str = r#"
@prefix ex: <http://example.org/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

ex:person1 ex:born "1901-02-03"^^xsd:date .
ex:person2 ex:born "1910-05-05"^^xsd:date .
ex:source1 rdf:reifies <<( ex:person1 ex:born "1900-12-31"^^xsd:date )>> .
ex:source2 rdf:reifies <<( ex:person2 ex:born "1910-05-05"^^xsd:date )>> .

ex:SourcedStatements a sh:NodeShape ;
    sh:targetSubjectsOf ex:born ;
    sh:sparql [
        sh:message "a source is about a statement that no longer holds" ;
        sh:select """
            SELECT $this WHERE {
                ?r <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> <<( $this ?p ?o )>> .
                FILTER NOT EXISTS { $this ?p ?o }
            }"""
    ] .
"#;

#[test]
fn this_inside_a_triple_term_is_the_focus_node() {
    let mut store = Store::new();
    let parser = RdfParser::from_format(RdfFormat::Turtle)
        .with_base_iri("http://example.org/")
        .expect("base");
    for quad in parser.for_reader(TURTLE.as_bytes()) {
        store.insert(quad.expect("parse").as_ref()).expect("insert");
    }
    let options = Options {
        data_graph: GraphFilter::Default,
        shapes_graph: GraphFilter::Default,
    };
    let shapes = CompiledShapes::compile(&store, options).expect("compile");
    let report = shapes.validate(&store).expect("validate");
    let quads = shapes.report_to_quads(&store, &report).expect("render");
    let focus: Vec<String> = quads
        .iter()
        .filter(|q| q.predicate.as_str() == "http://www.w3.org/ns/shacl#focusNode")
        .map(|q| q.object.to_string())
        .collect();
    assert_eq!(focus, ["<http://example.org/person1>"], "person 1, once");
}
