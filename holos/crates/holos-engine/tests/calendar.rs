//! `xsd:date`, `xsd:time` and the duration types compare, sort and subtract by value.
//!
//! Until 0.26.3 the workspace built `spareval` without its `sep-0002` feature, which is what
//! gives these datatypes values. Without it they were opaque typed literals: `<` between two
//! dates was a type error, and a FILTER whose expression errors removes the row, so
//! `FILTER(?d >= "2024-11-04"^^xsd:date)` returned nothing at all, silently. Only
//! `xsd:dateTime` compared. The case that found it was a "what is open on this day?" query
//! over seasons with xsd:date bounds, which answered "closed" for every place on every day:
//! right in November, wrong in July. These tests are that query and its parts, against the
//! evaluator HOLOS actually runs, reading the dates from the store.

use holos_engine::Engine;
use holos_security::Session;
use oxrdfio::RdfFormat;
use spareval::QueryResults;

const DATA: &str = r#"
@prefix ex:  <http://example.com/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:meeting1 ex:on "2025-06-12"^^xsd:date .
ex:meeting2 ex:on "2023-05-10"^^xsd:date .
ex:meeting3 ex:on "2026-02-03"^^xsd:date .

ex:centre  ex:from "2026-03-06"^^xsd:date ; ex:to "2026-11-08"^^xsd:date .
ex:skellig ex:from "2026-05-12"^^xsd:date ; ex:to "2026-09-21"^^xsd:date .
ex:museum  ex:name "no season published" .

ex:early ex:opens "08:30:00"^^xsd:time .
ex:late  ex:opens "10:00:00"^^xsd:time .
"#;

fn rows(query: &str) -> Vec<String> {
    let mut e = Engine::new();
    e.bulk_load(DATA.as_bytes(), RdfFormat::Turtle, None)
        .expect("load");
    let session = Session::unrestricted(e.store()).expect("session");
    let view = e.view(&session);
    let results = Engine::query(&view, query, None).expect("query");
    let QueryResults::Solutions(iter) = results else {
        panic!("expected a solutions result");
    };
    iter.map(|s| {
        s.expect("solution")
            .iter()
            .map(|(v, t)| format!("{}={t}", v.as_str()))
            .collect::<Vec<_>>()
            .join(" ")
    })
    .collect()
}

const PREFIXES: &str =
    "PREFIX ex: <http://example.com/> PREFIX xsd: <http://www.w3.org/2001/XMLSchema#> ";

fn q(body: &str) -> Vec<String> {
    rows(&format!("{PREFIXES}{body}"))
}

#[test]
fn a_filter_compares_stored_dates_with_a_constant() {
    assert_eq!(
        q(r#"SELECT ?m WHERE { ?m ex:on ?d FILTER(?d >= "2024-11-04"^^xsd:date) } ORDER BY ?m"#),
        [
            "m=<http://example.com/meeting1>",
            "m=<http://example.com/meeting3>"
        ]
    );
    assert_eq!(
        q(r#"SELECT ?m WHERE { ?m ex:on ?d FILTER(?d < "2024-11-04"^^xsd:date) }"#),
        ["m=<http://example.com/meeting2>"]
    );
    // Equality worked before and still does.
    assert_eq!(
        q(r#"SELECT ?m WHERE { ?m ex:on ?d FILTER(?d = "2023-05-10"^^xsd:date) }"#),
        ["m=<http://example.com/meeting2>"]
    );
}

#[test]
fn a_date_bound_by_values_compares_with_a_stored_one() {
    // The shape of the reported query: the day comes from VALUES, the season from the data.
    let open = |day: &str| {
        q(&format!(
            r#"SELECT ?place WHERE {{
                VALUES ?day {{ "{day}"^^xsd:date }}
                ?place ex:from ?from ; ex:to ?to .
                FILTER (?from <= ?day && ?day <= ?to)
            }} ORDER BY ?place"#
        ))
    };
    assert_eq!(
        open("2026-07-15"),
        [
            "place=<http://example.com/centre>",
            "place=<http://example.com/skellig>"
        ]
    );
    assert_eq!(open("2026-10-01"), ["place=<http://example.com/centre>"]);
    assert!(open("2026-11-21").is_empty());
}

#[test]
fn exists_over_a_date_range_is_true_in_season() {
    // Inside EXISTS, an erroring comparison made EXISTS false rather than failing loudly,
    // so the place was reported closed.
    let status = q(r#"SELECT ?place ?status WHERE {
            VALUES ?day { "2026-07-15"^^xsd:date }
            VALUES ?place { ex:centre ex:skellig ex:museum }
            BIND (IF(EXISTS { ?place ex:from ?f ; ex:to ?t . FILTER(?f <= ?day && ?day <= ?t) }, "open",
                  IF(EXISTS { ?place ex:from ?f }, "closed", "not known")) AS ?status)
        } ORDER BY ?place"#);
    assert_eq!(
        status,
        [
            "place=<http://example.com/centre> status=\"open\"",
            "place=<http://example.com/museum> status=\"not known\"",
            "place=<http://example.com/skellig> status=\"open\"",
        ]
    );
}

#[test]
fn dates_sort_by_value() {
    assert_eq!(
        q("SELECT ?d WHERE { ?m ex:on ?d } ORDER BY DESC(?d) LIMIT 1"),
        ["d=\"2026-02-03\"^^<http://www.w3.org/2001/XMLSchema#date>"]
    );
}

#[test]
fn times_compare() {
    assert_eq!(
        q(r#"SELECT ?s WHERE { ?s ex:opens ?t FILTER(?t < "09:00:00"^^xsd:time) }"#),
        ["s=<http://example.com/early>"]
    );
}

#[test]
fn subtracting_dates_gives_a_duration() {
    assert_eq!(
        q(r#"SELECT ?gap WHERE { BIND("2026-11-04"^^xsd:date - "2024-11-04"^^xsd:date AS ?gap) }"#),
        ["gap=\"P730D\"^^<http://www.w3.org/2001/XMLSchema#dayTimeDuration>"]
    );
    assert_eq!(
        q(
            r#"SELECT ?gap WHERE { BIND("2026-11-20T14:00:00"^^xsd:dateTime - "2026-11-20T13:28:00"^^xsd:dateTime AS ?gap) }"#
        ),
        ["gap=\"PT32M\"^^<http://www.w3.org/2001/XMLSchema#dayTimeDuration>"]
    );
}

#[test]
fn durations_compare() {
    assert_eq!(
        q(
            r#"SELECT ?ok WHERE { BIND("PT32M"^^xsd:dayTimeDuration < "PT45M"^^xsd:dayTimeDuration AS ?ok) }"#
        ),
        ["ok=\"true\"^^<http://www.w3.org/2001/XMLSchema#boolean>"]
    );
}
