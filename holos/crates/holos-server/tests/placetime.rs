//! The placetime URI sets as the server answers them: which base the URIs get, which
//! redirects and formats a request gets, and that what is served is usable by the store
//! beside it, temporally and spatially.

use holos_engine::{Engine, QueryOptions};
use holos_security::Session;
use oxrdf::{GraphName, Quad};
use oxrdfio::{RdfFormat, RdfParser};
use semops_placetime::{Base, Format};
use spareval::QueryResults;

// The server is a binary, so its modules reach a test through a path include, as `gsp.rs`
// does.
#[path = "../src/placetime.rs"]
mod placetime;

use placetime::{answer, negotiate, Answer, Mode};

fn header<'a>(answer: &'a Answer, name: &str) -> Option<&'a str> {
    answer
        .headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

#[test]
fn the_base_is_https_on_the_host_the_client_named() {
    let a = answer(
        &Mode::Host,
        "id/quarter/2006-Q1",
        Some("Ref.Example:8443"),
        None,
        None,
    );
    assert_eq!(a.status, 303);
    assert_eq!(
        header(&a, "location"),
        Some("https://ref.example:8443/doc/quarter/2006-Q1")
    );
}

#[test]
fn a_forwarded_host_is_used_when_the_caller_trusts_it() {
    // The server passes X-Forwarded-Host only under --trust-forwarded-identity; when it does,
    // the first name in it is the one the client used.
    let a = answer(
        &Mode::Host,
        "id/year/2006",
        Some("10.0.0.7:7878"),
        Some("data.example, proxy.internal"),
        None,
    );
    assert_eq!(
        header(&a, "location"),
        Some("https://data.example/doc/year/2006")
    );
    let a = answer(
        &Mode::Host,
        "id/year/2006",
        Some("10.0.0.7:7878"),
        None,
        None,
    );
    assert_eq!(
        header(&a, "location"),
        Some("https://10.0.0.7:7878/doc/year/2006")
    );
}

#[test]
fn a_host_that_is_not_a_host_is_refused() {
    for bad in ["evil.example/x", "a b", "x\"y"] {
        assert_eq!(
            answer(&Mode::Host, "id/year/2006", Some(bad), None, None).status,
            400,
            "{bad}"
        );
    }
    assert_eq!(
        answer(&Mode::Host, "id/year/2006", None, None, None).status,
        400
    );
}

#[test]
fn a_configured_base_is_served_at_its_own_path() {
    let mode = Mode::Fixed(Base::new("https://data.example/ref").expect("base"));
    assert_eq!(mode.relative("/ref/id/year/2006"), Some("id/year/2006"));
    assert_eq!(mode.relative("/id/year/2006"), None);
    assert_eq!(mode.relative("/ref/query"), None);
    // The Host header does not matter under a configured base.
    let a = answer(&mode, "id/year/2006", Some("localhost:7878"), None, None);
    assert_eq!(
        header(&a, "location"),
        Some("https://data.example/ref/doc/year/2006")
    );
    assert_eq!(Mode::Host.relative("/doc/year/2006"), Some("doc/year/2006"));
    assert_eq!(Mode::Host.relative("/graph"), None);
}

#[test]
fn a_document_is_negotiated_or_named_by_its_extension() {
    let a = answer(
        &Mode::Host,
        "doc/geopoint/wgs84/X-0.1276Y51.5072",
        Some("ref.example"),
        None,
        Some("text/html, application/ld+json;q=0.9, */*;q=0.1"),
    );
    assert_eq!(a.status, 200);
    assert_eq!(a.content_type, "application/ld+json; charset=utf-8");
    assert_eq!(
        header(&a, "content-location"),
        Some("https://ref.example/doc/geopoint/wgs84/X-0.1276Y51.5072.jsonld")
    );
    assert_eq!(header(&a, "vary"), Some("Accept"));
    let quads = RdfParser::from_format(RdfFormat::JsonLd {
        profile: oxrdfio::JsonLdProfileSet::empty(),
    })
    .for_slice(&a.body)
    .count();
    assert!(quads > 10);

    let a = answer(
        &Mode::Host,
        "doc/year/2006.nt",
        Some("ref.example"),
        None,
        Some("text/turtle"),
    );
    assert_eq!(a.content_type, "application/n-triples; charset=utf-8");
    assert_eq!(
        header(&a, "content-location"),
        Some("https://ref.example/doc/year/2006.nt")
    );
    assert_eq!(header(&a, "vary"), None);
    let text = String::from_utf8(a.body).expect("utf8");
    assert!(text.contains("<https://ref.example/id/year/2006> <http://www.w3.org/2000/01/rdf-schema#label> \"British Year:2006\"@en ."));

    let a = answer(
        &Mode::Host,
        "doc/year/2006",
        Some("ref.example"),
        None,
        None,
    );
    assert_eq!(a.content_type, "text/turtle; charset=utf-8");
    let text = String::from_utf8(a.body).expect("utf8");
    assert!(text.contains("@prefix interval: <http://reference.data.gov.uk/def/intervals/>"));
}

#[test]
fn what_is_not_there_is_404_and_odd_spellings_301() {
    assert_eq!(
        answer(&Mode::Host, "id/year/20061", Some("h"), None, None).status,
        303
    );
    assert_eq!(
        answer(&Mode::Host, "id/year/206", Some("h"), None, None).status,
        404
    );
    assert_eq!(
        answer(
            &Mode::Host,
            "doc/government-year/2006",
            Some("h"),
            None,
            None
        )
        .status,
        404
    );
    let a = answer(
        &Mode::Host,
        "doc/geopoint/wgs84/X1.50Y2.0.ttl",
        Some("h"),
        None,
        None,
    );
    assert_eq!(a.status, 301);
    assert_eq!(
        header(&a, "location"),
        Some("https://h/doc/geopoint/wgs84/X1.5Y2.ttl")
    );
}

#[test]
fn negotiation_honours_quality() {
    assert_eq!(negotiate(None), Format::Turtle);
    assert_eq!(negotiate(Some("text/html")), Format::Turtle);
    assert_eq!(
        negotiate(Some("text/turtle;q=0.5, application/rdf+xml")),
        Format::RdfXml
    );
    assert_eq!(negotiate(Some("application/n-triples")), Format::NTriples);
    assert_eq!(negotiate(Some("application/json")), Format::JsonLd);
}

/// Loads every document's triples into one store and runs a query over them.
fn rows(paths: &[&str], query: &str) -> Vec<String> {
    let base = Base::new("https://ref.example/").expect("base");
    let mut store = holos_store::Store::new();
    for path in paths {
        let graph = semops_placetime::describe(&base, path).expect(path);
        for triple in &graph {
            store
                .insert(
                    Quad::new(
                        triple.subject.into_owned(),
                        triple.predicate.into_owned(),
                        triple.object.into_owned(),
                        GraphName::DefaultGraph,
                    )
                    .as_ref(),
                )
                .expect("insert");
        }
    }
    let engine = Engine::with_store(store);
    let session = Session::unrestricted(engine.store()).expect("session");
    let view = engine.view(&session);
    let (results, _) = Engine::query_with(&view, query, &QueryOptions::new()).expect("query");
    let QueryResults::Solutions(solutions) = results else {
        panic!("not a SELECT")
    };
    solutions
        .map(|s| {
            let s = s.expect("row");
            s.iter()
                .map(|(_, t)| t.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

#[test]
fn points_are_found_by_geosparql_in_the_store() {
    let found = rows(
        &[
            "geopoint/wgs84/X-0.1276Y51.5072",
            "geopoint/wgs84/X2.3522Y48.8566",
            "geopoint/wgs84/X-3.1883Y55.9533",
        ],
        r#"PREFIX geo: <http://www.opengis.net/ont/geosparql#>
           PREFIX geof: <http://www.opengis.net/def/function/geosparql/>
           SELECT ?p WHERE {
             ?p geo:asWKT ?wkt .
             FILTER(geof:sfWithin(?wkt,
               "POLYGON((-8 49.9, 2 49.9, 2 61, -8 61, -8 49.9))"^^geo:wktLiteral))
           } ORDER BY ?p"#,
    );
    assert_eq!(
        found,
        [
            "<https://ref.example/id/geopoint/wgs84/X-0.1276Y51.5072>",
            "<https://ref.example/id/geopoint/wgs84/X-3.1883Y55.9533>",
        ]
    );
}

#[test]
fn intervals_are_ordered_by_their_instants_in_the_store() {
    // A British day before the change and its Gregorian equal begin at the same instant.
    let found = rows(
        &[
            "day/1700-01-01",
            "gregorian-day/1700-01-11",
            "gregorian-day/1700-01-01",
        ],
        r#"PREFIX time: <http://www.w3.org/2006/time#>
           SELECT ?d ?t WHERE {
             ?d time:hasBeginning/time:inXSDDateTime ?t .
             FILTER(STRSTARTS(STR(?d), "https://ref.example/id/"))
             FILTER(!CONTAINS(STR(?d), "instant") && !CONTAINS(STR(?d), "interval"))
           } ORDER BY ?t ?d"#,
    );
    assert_eq!(found.len(), 3);
    assert!(found[0].starts_with("<https://ref.example/id/gregorian-day/1700-01-01>"));
    assert!(found[1].contains("\"1700-01-11T00:00:00\""));
    assert!(found[2].contains("\"1700-01-11T00:00:00\""));
}
