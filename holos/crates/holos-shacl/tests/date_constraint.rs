//! A SHACL-SPARQL constraint that compares `xsd:date` values fires.
//!
//! Until 0.26.3 the evaluator gave dates no value, so `?until < "2026-11-10"^^xsd:date` was a
//! type error, the FILTER dropped the row, and the constraint never reported anything: an
//! expired certificate passed validation silently. `sh:lessThan` and the other Core
//! comparisons were never affected, because the SHACL engine compares those itself.

use holos_shacl::engine::EngineRun;
use holos_shacl::Options;
use holos_store::{GraphFilter, Store};
use oxrdf::{NamedNodeRef, TermRef};
use oxrdfio::{RdfFormat, RdfParser};

const SHAPES_AND_DATA: &str = r#"
@prefix ex:  <http://example.com/> .
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:CertificateShape a sh:NodeShape ;
    sh:targetClass ex:Certificate ;
    sh:sparql [
        sh:message "expired on {?until}" ;
        sh:select """
            SELECT $this ?until WHERE {
                $this <http://example.com/validUntil> ?until .
                FILTER (?until < "2026-11-10"^^<http://www.w3.org/2001/XMLSchema#date>)
            }""" ;
    ] .

ex:current a ex:Certificate ; ex:validUntil "2027-03-31"^^xsd:date .
ex:expired a ex:Certificate ; ex:validUntil "2026-06-30"^^xsd:date .
"#;

#[test]
fn an_expired_date_is_reported() {
    let mut store = Store::new();
    let parser = RdfParser::from_format(RdfFormat::Turtle)
        .with_base_iri("http://example.com/")
        .expect("base");
    for quad in parser.for_reader(SHAPES_AND_DATA.as_bytes()) {
        store.insert(quad.expect("parse").as_ref()).expect("insert");
    }
    let options = Options {
        data_graph: GraphFilter::Default,
        shapes_graph: GraphFilter::Default,
    };
    let mut run = EngineRun::prepare(&store, options).expect("prepare");
    let report = run.validate().expect("validate");
    let graph = run.report_to_oxrdf(&report);
    let message = NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#resultMessage");
    let messages: Vec<String> = graph
        .triples_for_predicate(message)
        .filter_map(|t| match t.object {
            TermRef::Literal(l) => Some(l.value().to_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(messages, ["expired on 2026-06-30"]);
}
