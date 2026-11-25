//! A SPARQL constraint's computed `?value` reaches HOLOS's report.
//!
//! The adapted engine now interns a term the query computed (pwin/SemopsSystem#24), and
//! the bridge has to carry it back: the store holds no such term, so it comes back as an
//! ephemeral id, the way a message filled in from the solution does, and must still render
//! as `sh:value`. The same goes for a computed IRI in `?path`.

use holos_shacl::{CompiledShapes, Options};
use holos_store::{GraphFilter, Store};
use oxrdf::Quad;
use oxrdfio::{RdfFormat, RdfParser};

fn load(turtle: &str) -> Store {
    let mut store = Store::new();
    let parser = RdfParser::from_format(RdfFormat::Turtle)
        .with_base_iri("http://example.org/")
        .expect("base");
    for quad in parser.for_reader(turtle.as_bytes()) {
        store.insert(quad.expect("parse").as_ref()).expect("insert");
    }
    store
}

fn report(turtle: &str) -> (usize, Vec<Quad>) {
    let store = load(turtle);
    let options = Options {
        data_graph: GraphFilter::Default,
        shapes_graph: GraphFilter::Default,
    };
    let shapes = CompiledShapes::compile(&store, options).expect("compile");
    let report = shapes.validate(&store).expect("validate");
    let quads = shapes.report_to_quads(&store, &report).expect("render");
    (report.results.len(), quads)
}

fn objects(quads: &[Quad], local: &str) -> Vec<String> {
    let predicate = format!("http://www.w3.org/ns/shacl#{local}");
    let mut out: Vec<String> = quads
        .iter()
        .filter(|q| q.predicate.as_str() == predicate)
        .map(|q| q.object.to_string())
        .collect();
    out.sort();
    out
}

const PREFIX: &str = "@prefix ex: <http://example.org/> .\n\
                      @prefix sh: <http://www.w3.org/ns/shacl#> .\n";

/// The issue's example, through the native evaluator: neither graph holds `5`.
#[test]
fn computed_literal_value_is_rendered() {
    let (n, quads) = report(&format!(
        r#"{PREFIX}
        ex:S a sh:NodeShape ; sh:targetNode ex:a ;
          sh:sparql [ sh:message "value is {{?value}}" ;
            sh:select "SELECT $this ?value WHERE {{ BIND(5 AS ?value) }}" ] .
        ex:a ex:p ex:b ."#
    ));
    assert_eq!(n, 1);
    assert_eq!(
        objects(&quads, "value"),
        ["\"5\"^^<http://www.w3.org/2001/XMLSchema#integer>"]
    );
    assert_eq!(objects(&quads, "resultMessage"), ["\"value is 5\""]);
}

#[test]
fn computed_iri_value_and_path_are_rendered() {
    let (n, quads) = report(&format!(
        r#"{PREFIX}
        ex:S a sh:NodeShape ; sh:targetNode ex:a ;
          sh:sparql [ sh:select """SELECT $this ?value ?path WHERE {{
              BIND(IRI(CONCAT(STR($this), "/derived")) AS ?value)
              BIND(<http://example.org/elsewhere> AS ?path) }}""" ] .
        ex:a ex:p ex:b ."#
    ));
    assert_eq!(n, 1);
    assert_eq!(objects(&quads, "value"), ["<http://example.org/a/derived>"]);
    assert_eq!(objects(&quads, "resultPath"), ["<http://example.org/elsewhere>"]);
}
