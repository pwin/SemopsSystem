//! The shared function library and the property functions, in queries against the store.
//!
//! The first test runs `semops_functions::cases` — the table every evaluator is held to —
//! through `Engine::query`. The rest are property functions over data, which only a store
//! can test: the staged ones carried through joins, blank nodes, `OPTIONAL`, `UNION`, `GRAPH`
//! and updates, and the ones that read RDF lists and containers.

use holos_engine::{update, Engine, EngineError};
use holos_security::Session;
use oxrdfio::RdfFormat;
use semops_functions::cases;
use spareval::QueryResults;

const PREFIXES: &str = "PREFIX ex: <http://example.com/>
PREFIX apf: <http://jena.apache.org/ARQ/property#>
PREFIX list: <http://jena.apache.org/ARQ/list#>
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
PREFIX spif: <http://spinrdf.org/spif#>
";

const DATA: &str = r#"
@prefix ex: <http://example.com/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:a ex:tags "red; green" .
ex:b ex:tags "blue" .
ex:red ex:label "Red" .
ex:green ex:label "Green" .
ex:blue ex:label "Blue" .
_:anon ex:tags "x;y" ; ex:kind "anonymous" .
ex:things ex:items ( ex:one ex:two ex:three ) .
ex:box a rdf:Bag ; rdf:_1 ex:pen ; rdf:_2 ex:ink .
ex:shelf ex:holds ex:box .
ex:explicit rdfs:member ex:stated .
"#;

const NAMED: &str = r#"
@prefix ex: <http://example.com/> .
ex:c ex:tags "north;south" .
"#;

fn engine() -> Engine {
    let mut e = Engine::new();
    e.bulk_load(DATA.as_bytes(), RdfFormat::Turtle, None)
        .expect("load");
    let graph =
        oxrdf::GraphName::NamedNode(oxrdf::NamedNode::new_unchecked("http://example.com/g"));
    e.bulk_load_into_graph(NAMED.as_bytes(), RdfFormat::Turtle, None, &graph)
        .expect("load named");
    e
}

fn try_rows(e: &Engine, query: &str) -> Result<Vec<String>, EngineError> {
    let session = Session::unrestricted(e.store()).expect("session");
    let view = e.view(&session);
    let QueryResults::Solutions(iter) = Engine::query(&view, &format!("{PREFIXES}{query}"), None)?
    else {
        panic!("expected solutions");
    };
    let mut out: Vec<String> = iter
        .map(|s| {
            let s = s.expect("solution");
            let mut cells: Vec<String> = s
                .iter()
                .map(|(v, t)| format!("{}={t}", v.as_str()))
                .collect();
            cells.sort();
            cells.join(" ")
        })
        .collect();
    out.sort();
    Ok(out)
}

fn rows(e: &Engine, query: &str) -> Vec<String> {
    try_rows(e, query).unwrap_or_else(|err| panic!("{query}: {err}"))
}

#[test]
fn the_shared_function_table_holds_in_store_queries() {
    let e = Engine::new();
    let session = Session::unrestricted(e.store()).expect("session");
    let view = e.view(&session);
    let mut failures = Vec::new();
    for case in cases::CASES {
        let QueryResults::Solutions(mut iter) =
            Engine::query(&view, &cases::query(case), None).expect("query")
        else {
            unreachable!()
        };
        let solution = iter.next().expect("one row").expect("solution");
        let holds =
            matches!(solution.get("ok"), Some(oxrdf::Term::Literal(l)) if l.value() == "true");
        if !holds {
            failures.push(format!(
                "{} gave {:?}",
                case.expression,
                solution.get("r").map(ToString::to_string)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} fail:\n{}",
        failures.len(),
        cases::CASES.len(),
        failures.join("\n")
    );
}

#[test]
fn str_split_splits_stored_values_and_the_parts_join_onwards() {
    let e = engine();
    assert_eq!(
        rows(&e, "SELECT ?s ?t WHERE { ?s ex:tags ?tags . FILTER(isIRI(?s)) ?t apf:strSplit (?tags \";\") }"),
        [
            "s=<http://example.com/a> t=\"green\"",
            "s=<http://example.com/a> t=\"red\"",
            "s=<http://example.com/b> t=\"blue\""
        ]
    );
    // the parts become IRIs and meet the data again after the split
    assert_eq!(
        rows(
            &e,
            "SELECT ?label WHERE { ex:a ex:tags ?tags . ?t apf:strSplit (?tags \";\") \
             BIND(IRI(CONCAT(\"http://example.com/\", ?t)) AS ?tag) ?tag ex:label ?label }"
        ),
        ["label=\"Green\"", "label=\"Red\""]
    );
}

#[test]
fn a_blank_node_survives_the_step_and_still_joins() {
    let e = engine();
    // ?n is a blank node in the store: carried through the staged table, it still matches
    // its own ex:kind afterwards.
    assert_eq!(
        rows(&e, "SELECT ?t ?kind WHERE { ?n ex:tags ?tags . FILTER(isBlank(?n)) ?t apf:strSplit (?tags \";\") . ?n ex:kind ?kind }"),
        ["kind=\"anonymous\" t=\"x\"", "kind=\"anonymous\" t=\"y\""]
    );
    let labels = rows(
        &e,
        "SELECT ?label WHERE { ?n ex:kind \"anonymous\" . ?n apf:blankNode ?label }",
    );
    assert_eq!(labels.len(), 1);
    assert!(labels[0].starts_with("label=\""), "{labels:?}");
}

#[test]
fn staged_functions_work_inside_optional_union_and_graph() {
    let e = engine();
    assert_eq!(
        rows(&e, "SELECT ?s ?t WHERE { ?s ex:label ?l OPTIONAL { ?s ex:tags ?tags . ?t apf:strSplit (?tags \";\") } } "),
        ["s=<http://example.com/blue>", "s=<http://example.com/green>", "s=<http://example.com/red>"]
    );
    assert_eq!(
        rows(&e, "SELECT ?t WHERE { { ex:b ex:tags ?x . ?t apf:strSplit (?x \";\") } UNION { ?t apf:strSplit (\"p;q\" \";\") } }"),
        ["t=\"blue\"", "t=\"p\"", "t=\"q\""]
    );
    assert_eq!(
        rows(
            &e,
            "SELECT ?g ?t WHERE { GRAPH ?g { ?s ex:tags ?tags . ?t apf:strSplit (?tags \";\") } }"
        ),
        [
            "g=<http://example.com/g> t=\"north\"",
            "g=<http://example.com/g> t=\"south\""
        ]
    );
}

#[test]
fn the_other_functions_of_their_arguments() {
    let e = engine();
    assert_eq!(
        rows(&e, "SELECT ?p WHERE { ?p spif:split (\"a, b\" \",\") }"),
        ["p=\" b\"", "p=\"a\""]
    );
    assert_eq!(
        rows(&e, "SELECT ?c WHERE { ?c apf:concat (\"x\" 1 ex:y) }"),
        ["c=\"x1http://example.com/y\""]
    );
    assert_eq!(
        rows(
            &e,
            "SELECT ?ns ?ln WHERE { ex:red ex:label ?l . ex:red apf:splitIRI (?ns ?ln) }"
        ),
        ["ln=\"red\" ns=<http://example.com/>"]
    );
    assert_eq!(
        rows(&e, "SELECT ?v WHERE { ?v apf:assign 42 }"),
        ["v=\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>"]
    );
}

#[test]
fn list_functions_read_rdf_lists() {
    let e = engine();
    assert_eq!(
        rows(
            &e,
            "SELECT ?m WHERE { ex:things ex:items ?l . ?l list:member ?m }"
        ),
        [
            "m=<http://example.com/one>",
            "m=<http://example.com/three>",
            "m=<http://example.com/two>"
        ]
    );
    assert_eq!(
        rows(
            &e,
            "SELECT ?i ?m WHERE { ex:things ex:items ?l . ?l list:index (?i ?m) }"
        ),
        [
            "i=\"0\"^^<http://www.w3.org/2001/XMLSchema#integer> m=<http://example.com/one>",
            "i=\"1\"^^<http://www.w3.org/2001/XMLSchema#integer> m=<http://example.com/two>",
            "i=\"2\"^^<http://www.w3.org/2001/XMLSchema#integer> m=<http://example.com/three>"
        ]
    );
    assert_eq!(
        rows(
            &e,
            "SELECT ?m WHERE { ex:things ex:items ?l . ?l list:index (1 ?m) }"
        ),
        ["m=<http://example.com/two>"]
    );
    assert_eq!(
        rows(
            &e,
            "SELECT ?n WHERE { ex:things ex:items ?l . ?l list:length ?n }"
        ),
        ["n=\"3\"^^<http://www.w3.org/2001/XMLSchema#integer>"]
    );
    assert_eq!(
        rows(&e, "SELECT * WHERE { rdf:nil list:length 0 }").len(),
        1,
        "the empty list has length 0"
    );
}

#[test]
fn container_membership_reads_rdf_containers() {
    let e = engine();
    assert_eq!(
        rows(
            &e,
            "SELECT ?m WHERE { ex:shelf ex:holds ?c . ?c rdfs:member ?m }"
        ),
        ["m=<http://example.com/ink>", "m=<http://example.com/pen>"]
    );
    assert_eq!(rows(&e, "SELECT ?m WHERE { ex:box apf:bag ?m }").len(), 2);
    assert_eq!(
        rows(&e, "SELECT ?m WHERE { ex:box apf:seq ?m }").len(),
        0,
        "a bag is not a sequence"
    );
    // rdfs:member still matches statements that use it
    assert_eq!(
        rows(&e, "SELECT ?m WHERE { ex:explicit rdfs:member ?m }"),
        ["m=<http://example.com/stated>"]
    );
}

#[test]
fn an_update_can_split_what_it_inserts() {
    let mut e = engine();
    let mut session = Session::unrestricted(e.store()).expect("session");
    update::update(
        &mut e,
        &mut session,
        &format!("{PREFIXES}INSERT {{ ?s ex:tag ?t }} WHERE {{ ?s ex:tags ?tags . FILTER(isIRI(?s)) ?t apf:strSplit (?tags \";\") }}"),
        None,
    )
    .expect("update");
    assert_eq!(
        rows(&e, "SELECT ?t WHERE { ex:a ex:tag ?t }"),
        ["t=\"green\"", "t=\"red\""]
    );
}

#[test]
fn what_cannot_be_staged_is_refused_with_its_name() {
    let e = engine();
    let err = |q: &str| try_rows(&e, q).expect_err(q).to_string();
    assert!(
        err("SELECT * WHERE { ?s apf:textMatch \"x\" }").contains("apf:textMatch is not supported")
    );
    assert!(
        err("SELECT * WHERE { ?s ex:tags ?x FILTER EXISTS { ?t apf:strSplit (?x \";\") } }")
            .contains("FILTER EXISTS")
    );
    assert!(err("SELECT * WHERE { ?p apf:strSplit (?s) }").contains("apf:strSplit is written"));
}

#[test]
fn a_substitution_reaches_the_step_that_uses_it_and_no_other() {
    // ?s is given by the protocol, not the text (projected, as the evaluator requires of a
    // substituted variable). The first step, the triples before the split, uses it; a step
    // that does not mention it must not be handed it, or the evaluator refuses the step.
    let e = engine();
    let session = Session::unrestricted(e.store()).expect("session");
    let view = e.view(&session);
    let options = holos_engine::QueryOptions::new().with_substitution(
        oxrdf::Variable::new_unchecked("s"),
        oxrdf::NamedNode::new_unchecked("http://example.com/a"),
    );
    let query = format!(
        "{PREFIXES}SELECT ?s ?t WHERE {{ ?s ex:tags ?tags . ?t apf:strSplit (?tags \";\") }}"
    );
    let (results, _) = Engine::query_with(&view, &query, &options).expect("query");
    let QueryResults::Solutions(iter) = results else {
        panic!("expected solutions")
    };
    let mut tags: Vec<String> = iter
        .map(|s| s.expect("solution").get("t").expect("bound").to_string())
        .collect();
    tags.sort();
    assert_eq!(tags, ["\"green\"", "\"red\""]);
}
