//! SHACL-SPARQL through the native evaluator: Core checked over the store's indexes, the
//! SPARQL constraints by the adapted engine, one report.
//!
//! What has to hold for that to be one validator rather than two stapled together: the
//! results name the same terms a full run would, a message filled in from the solution
//! survives into the rendered report though no graph holds it, a blank-node focus comes back
//! as the store's own blank node, revalidation on the write path still finds a SPARQL
//! violation, and every function of the shared library works in a constraint.

use holos_shacl::incremental::Change;
use holos_shacl::{CompiledShapes, Options};
use holos_store::{GraphFilter, Store};
use oxrdf::Quad;
use oxrdfio::{RdfFormat, RdfParser};
use semops_functions::cases;

fn load(turtle: &str) -> Store {
    let mut store = Store::new();
    let parser = RdfParser::from_format(RdfFormat::Turtle)
        .with_base_iri("http://example.com/")
        .expect("base");
    for quad in parser.for_reader(turtle.as_bytes()) {
        store.insert(quad.expect("parse").as_ref()).expect("insert");
    }
    store
}

fn options() -> Options {
    Options {
        data_graph: GraphFilter::Default,
        shapes_graph: GraphFilter::Default,
    }
}

fn rendered(store: &Store, turtle_shapes_and_data: &str) -> (bool, Vec<Quad>) {
    let _ = turtle_shapes_and_data;
    let shapes = CompiledShapes::compile(store, options()).expect("compile");
    let report = shapes.validate(store).expect("validate");
    let quads = shapes.report_to_quads(store, &report).expect("render");
    (report.conforms, quads)
}

fn objects(quads: &[Quad], predicate: &str) -> Vec<String> {
    let mut out: Vec<String> = quads
        .iter()
        .filter(|q| q.predicate.as_str() == predicate)
        .map(|q| q.object.to_string())
        .collect();
    out.sort();
    out
}

const SH: &str = "http://www.w3.org/ns/shacl#";

const LIBRARY: &str = r#"
@prefix ex: <http://example.com/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .

ex:BookShape a sh:NodeShape ;
    sh:targetClass ex:Book ;
    # Core, checked natively
    sh:property [ sh:path ex:title ; sh:minCount 1 ] ;
    # SPARQL, with a message filled in from the solution and a function from the library
    sh:sparql [
        sh:message "title {?value} is not in title case" ;
        sh:select """
            SELECT $this ?value WHERE {
                $this <http://example.com/title> ?value .
                FILTER (?value != <http://spinrdf.org/spif#titleCase>(?value))
            }""" ;
    ] .

ex:TagShape a sh:NodeShape ;
    sh:targetSubjectsOf ex:tag ;
    sh:property [
        sh:path ex:tag ;
        sh:sparql [
            sh:message "a tag is lower case" ;
            sh:select "SELECT $this ?value WHERE { $this $PATH ?value . FILTER (?value != LCASE(?value)) }" ;
        ] ;
    ] .

ex:good a ex:Book ; ex:title "War And Peace" ; ex:tag "classic" .
ex:shouty a ex:Book ; ex:title "war and peace" ; ex:tag "LOUD" .
ex:untitled a ex:Book .
_:anon a ex:Book ; ex:title "anonymous work" .
"#;

#[test]
fn core_and_sparql_results_come_back_in_one_report() {
    let store = load(LIBRARY);
    let shapes = CompiledShapes::compile(&store, options()).expect("compile");
    assert!(shapes.has_sparql());
    let report = shapes.validate(&store).expect("validate");
    assert!(!report.conforms);
    // ex:untitled (minCount, natively), ex:shouty and _:anon (title case), ex:shouty (tag)
    assert_eq!(report.results.len(), 4, "{report:#?}");
    let (_, quads) = rendered(&store, LIBRARY);
    let components = objects(&quads, &format!("{SH}sourceConstraintComponent"));
    assert_eq!(
        components
            .iter()
            .filter(|c| c.contains("SPARQLConstraintComponent"))
            .count(),
        3
    );
    assert_eq!(
        components
            .iter()
            .filter(|c| c.contains("MinCountConstraintComponent"))
            .count(),
        1
    );
}

#[test]
fn a_filled_in_message_survives_though_no_graph_holds_it() {
    let store = load(LIBRARY);
    let (_, quads) = rendered(&store, LIBRARY);
    let messages = objects(&quads, &format!("{SH}resultMessage"));
    assert!(
        messages.contains(&"\"title war and peace is not in title case\"".to_owned()),
        "{messages:?}"
    );
    assert!(
        messages.contains(&"\"a tag is lower case\"".to_owned()),
        "{messages:?}"
    );
}

#[test]
fn a_blank_node_focus_is_the_stores_own() {
    let store = load(LIBRARY);
    let (_, quads) = rendered(&store, LIBRARY);
    let focus = objects(&quads, &format!("{SH}focusNode"));
    let blank: Vec<&String> = focus.iter().filter(|f| f.starts_with("_:")).collect();
    assert_eq!(blank.len(), 1, "{focus:?}");
    // the same blank node the data has: it has the title the data gave it
    let label = blank[0].trim_start_matches("_:");
    let anon = oxrdf::BlankNode::new_unchecked(label);
    let title = store
        .lookup_term(oxrdf::TermRef::BlankNode(anon.as_ref()))
        .expect("lookup");
    assert!(
        title.is_some(),
        "the reported blank node is not one the store holds"
    );
}

#[test]
fn a_property_shape_reports_its_path() {
    let store = load(LIBRARY);
    let (_, quads) = rendered(&store, LIBRARY);
    let paths = objects(&quads, &format!("{SH}resultPath"));
    assert!(
        paths.contains(&"<http://example.com/tag>".to_owned()),
        "{paths:?}"
    );
}

#[test]
fn revalidation_on_the_write_path_still_finds_a_sparql_violation() {
    let mut store = load(LIBRARY);
    let shapes = CompiledShapes::compile(&store, options()).expect("compile");
    let quad = Quad::new(
        oxrdf::NamedNode::new_unchecked("http://example.com/good"),
        oxrdf::NamedNode::new_unchecked("http://example.com/tag"),
        oxrdf::Literal::new_simple_literal("NEW"),
        oxrdf::GraphName::DefaultGraph,
    );
    let encoded = store.encode_quad(quad.as_ref()).expect("encode");
    store.insert(quad.as_ref()).expect("insert");
    let report = shapes
        .revalidate(&store, &[Change::added(encoded)])
        .expect("revalidate");
    assert!(!report.conforms);
    let focus: Vec<String> = report
        .results
        .iter()
        .filter_map(|r| store.decode_term(r.focus_node).ok().flatten())
        .map(|t| t.to_string())
        .collect();
    assert!(
        focus.contains(&"<http://example.com/good>".to_owned()),
        "the new upper-case tag was missed: {focus:?}"
    );
}

/// The shared function table, as constraints the native evaluator hands on.
#[test]
fn every_function_works_in_a_sparql_constraint() {
    let store = load(&format!("{}\n{}", cases::shapes(), cases::DATA));
    let (conforms, quads) = rendered(&store, "");
    let messages = objects(&quads, &format!("{SH}resultMessage"));
    assert!(
        conforms,
        "{} of {} fail:\n{}",
        messages.len(),
        cases::CASES.len(),
        messages.join("\n")
    );
}

/// The control: a case that does not hold is reported.
#[test]
fn a_wrong_answer_is_a_violation() {
    let wrong = [cases::ok(r#"fn:upper-case("a")"#, r#""b""#)];
    let store = load(&format!("{}\n{}", cases::shapes_for(&wrong), cases::DATA));
    let (conforms, _) = rendered(&store, "");
    assert!(!conforms);
}
