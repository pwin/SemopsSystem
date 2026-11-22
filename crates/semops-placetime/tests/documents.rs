//! What the documents say, checked against IntervalServer's published example and against
//! every serialisation a document is offered in.

use oxrdf::vocab::rdf;
use oxrdf::{Graph, NamedNodeRef, TermRef, Triple};
use oxrdfio::{RdfFormat, RdfParser, RdfSerializer};
use semops_placetime::{Base, Format, Outcome, resolve};

const BASE: &str = "https://ref.example/";

fn base() -> Base {
    Base::new(BASE).expect("base")
}

fn document(path: &str) -> semops_placetime::Document {
    match resolve(&base(), path) {
        Outcome::Document(d) => *d,
        other => panic!("{path}: expected a document, got {other:?}"),
    }
}

/// Every triple of `expected` (Turtle, prefixes supplied) is in `graph`.
fn assert_contains(graph: &Graph, expected: &str) {
    let turtle = format!(
        "@base <{BASE}> .
         @prefix interval: <http://reference.data.gov.uk/def/intervals/> .
         @prefix dgu: <http://reference.data.gov.uk/def/reference/> .
         @prefix time: <http://www.w3.org/2006/time#> .
         @prefix scv: <http://purl.org/NET/scovo#> .
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
         @prefix foaf: <http://xmlns.com/foaf/0.1/> .
         @prefix dcterms: <http://purl.org/dc/terms/> .
         @prefix geo: <http://www.opengis.net/ont/geosparql#> .
         @prefix sf: <http://www.opengis.net/ont/sf#> .
         @prefix wgs84: <http://www.w3.org/2003/01/geo/wgs84_pos#> .
         {expected}"
    );
    for quad in RdfParser::from_format(RdfFormat::Turtle).for_slice(turtle.as_bytes()) {
        let quad = quad.expect("expected Turtle parses");
        let triple = Triple::from(quad);
        assert!(graph.contains(&triple), "missing: {triple}");
    }
}

/// The members of the RDF list `node predicate (…)`, in order.
fn list(graph: &Graph, node: &str, predicate: NamedNodeRef<'_>) -> Vec<String> {
    let node = oxrdf::NamedNode::new_unchecked(node);
    let mut out = Vec::new();
    let mut cell = graph
        .object_for_subject_predicate(&node, predicate)
        .expect("a list")
        .into_owned();
    while let oxrdf::Term::BlankNode(b) = &cell {
        match graph.object_for_subject_predicate(b, rdf::FIRST) {
            Some(TermRef::NamedNode(n)) => out.push(n.as_str().to_owned()),
            other => panic!("list member {other:?}"),
        }
        cell = graph
            .object_for_subject_predicate(b, rdf::REST)
            .expect("rest")
            .into_owned();
    }
    out
}

#[test]
fn the_published_quarter_example_is_reproduced() {
    // interval-uris.md in IntervalServer annotates the document for /id/quarter/2006-Q1;
    // these are its statements, under the new base.
    let doc = document("doc/quarter/2006-Q1");
    assert_contains(
        &doc.graph,
        r#"
        <id/quarter/2006-Q1>
            rdfs:comment "The first quarter of the British calendar year 2006"@en ;
            rdfs:label "British Quarter:2006-Q1"@en ;
            skos:prefLabel "British Quarter:2006-Q1"@en ;
            dgu:uriSet <id/quarter> ;
            a interval:Q1, interval:Quarter, interval:CalendarQuarter, scv:Dimension ;
            scv:max "2006-03-31"^^xsd:date ;
            scv:min "2006-01-01"^^xsd:date ;
            time:hasBeginning <id/gregorian-instant/2006-01-01T00:00:00> ;
            time:hasEnd <id/gregorian-instant/2006-04-01T00:00:00> ;
            interval:hasXsdDurationDescription "P3M"^^xsd:duration ;
            time:hasDurationDescription interval:one-quarter ;
            interval:nextInterval <id/quarter/2006-Q2> ;
            interval:previousInterval <id/quarter/2005-Q4> ;
            time:intervalMeets <id/quarter/2006-Q2> ;
            time:intervalMetBy <id/quarter/2005-Q4> ;
            interval:intervalContainsMonth <id/month/2006-01>, <id/month/2006-02>, <id/month/2006-03> ;
            time:intervalContains <id/month/2006-01>, <id/month/2006-02>, <id/month/2006-03> ;
            time:intervalDuring <id/year/2006>, <id/half/2006-H1> ;
            time:intervalEquals <id/gregorian-interval/2006-01-01T00:00:00/P3M> ;
            interval:ordinalQuarterOfYear 1 ;
            interval:ordinalHalfOfYear 1 ;
            interval:ordinalYear 2006 .
        <id/quarter/2006-Q2> time:intervalMetBy <id/quarter/2006-Q1> .
        <id/year/2006> interval:intervalContainsQuarter <id/quarter/2006-Q1> .
        <id/gregorian-instant/2006-01-01T00:00:00>
            a time:Instant ;
            time:inXSDDateTime "2006-01-01T00:00:00"^^xsd:dateTime ;
            rdfs:label "Gregorian Instant:2006-01-01T00:00:00"@en ;
            interval:dayOfWeek time:Sunday .
        <doc/quarter/2006-Q1> a foaf:Document ;
            foaf:primaryTopic <id/quarter/2006-Q1> ;
            dcterms:hasFormat <doc/quarter/2006-Q1.ttl>, <doc/quarter/2006-Q1.jsonld> .
        "#,
    );
    assert_eq!(
        list(
            &doc.graph,
            &format!("{BASE}id/quarter/2006-Q1"),
            NamedNodeRef::new_unchecked(
                "http://reference.data.gov.uk/def/intervals/intervalContainsMonths"
            )
        ),
        ["01", "02", "03"].map(|m| format!("{BASE}id/month/2006-{m}"))
    );
}

#[test]
fn a_british_date_before_1752_equals_its_gregorian_one() {
    let doc = document("doc/day/1700-01-01");
    assert_contains(
        &doc.graph,
        r#"
        <id/day/1700-01-01>
            rdfs:comment "Monday the 1st of January in the British calendar year 1700"@en ;
            time:intervalEquals <id/gregorian-interval/1700-01-11T00:00:00/P1D> ;
            time:hasBeginning <id/gregorian-instant/1700-01-11T00:00:00> ;
            scv:min "1700-01-11T00:00:00"^^xsd:dateTime ;
            scv:max "1700-01-11T23:59:59"^^xsd:dateTime ;
            interval:ordinalDayOfYear 1 ;
            interval:dayOfWeek time:Monday ;
            time:dayOfWeek time:Monday .
        "#,
    );
    // The British year 1752 lost eleven days, so its Gregorian equal is not a year long.
    let doc = document("doc/year/1752");
    assert_contains(
        &doc.graph,
        r#"
        <id/year/1752>
            time:intervalEquals <id/gregorian-interval/1752-01-12T00:00:00/P11M20D> ;
            scv:min "1752-01-12"^^xsd:date ;
            scv:max "1752-12-31"^^xsd:date .
        "#,
    );
}

#[test]
fn the_corrected_bugs_stay_corrected() {
    // IntervalServer gave a half year a duration of one second.
    assert_contains(
        &document("doc/gregorian-half/2006-H2").graph,
        r#"<id/gregorian-half/2006-H2> interval:hasXsdDurationDescription "P6M"^^xsd:duration ;
               a interval:H2 ."#,
    );
    // It gave an hour's previous hour as the same hour a day earlier.
    assert_contains(
        &document("doc/gregorian-hour/2006-01-01T00").graph,
        r#"<id/gregorian-hour/2006-01-01T00> interval:previousInterval <id/gregorian-hour/2005-12-31T23> ."#,
    );
    // And it called every week a Gregorian one.
    assert_contains(
        &document("doc/week/2006-W01").graph,
        r#"<id/week/2006-W01> rdfs:label "British Week:2006-W01"@en ."#,
    );
    // And spelled the fourth quarter "forth".
    assert_contains(
        &document("doc/quarter/2006-Q4").graph,
        r#"<id/quarter/2006-Q4> rdfs:comment "The fourth quarter of the British calendar year 2006"@en ."#,
    );
}

#[test]
fn instants_intervals_and_julian_intervals_are_described() {
    assert_contains(
        &document("doc/gregorian-interval/2016-06-30T00:00:00/P3M").graph,
        r#"
        <id/gregorian-interval/2016-06-30T00:00:00/P3M> a time:Interval, scv:Dimension ;
            rdfs:comment "An interval of 3 months beginning at the start of the 1st second of 1st minute of 1st hour of Thursday the 30th June 2016 in the Gregorian Calendar"@en ;
            time:hasEnd <id/gregorian-instant/2016-09-30T00:00:00> ;
            scv:max "2016-09-29T23:59:59"^^xsd:dateTime ;
            dgu:uriSet <id/gregorian-interval> .
        "#,
    );
    assert_contains(
        &document("doc/gregorian-instant/1752-09-14T00:00:00").graph,
        r#"<id/gregorian-instant/1752-09-14T00:00:00> dgu:uriSet <id/gregorian-instant> ;
               interval:ordinalDayOfYear 258 ."#,
    );
    // The Julian day the British calendar skipped, and the Gregorian day it fell on.
    assert_contains(
        &document("doc/julian-day/1752-09-05").graph,
        r#"<id/julian-day/1752-09-05> rdfs:label "Julian Day:1752-09-05"@en ;
               time:intervalEquals <id/gregorian-interval/1752-09-16T00:00:00/P1D> ."#,
    );
}

#[test]
fn a_point_is_described_for_geosparql() {
    let doc = document("doc/geopoint/wgs84/X-0.1276Y51.5072");
    assert_contains(
        &doc.graph,
        r#"
        <id/geopoint/wgs84/X-0.1276Y51.5072> a geo:Geometry, sf:Point, wgs84:Point ;
            rdfs:label "WGS84 Point: 51.5072° N, 0.1276° W"@en ;
            geo:asWKT "POINT(-0.1276 51.5072)"^^geo:wktLiteral ;
            wgs84:lat 51.5072 ;
            wgs84:long -0.1276 ;
            rdfs:seeAlso <geo:51.5072,-0.1276> ;
            dgu:uriSet <id/geopoint/wgs84> .
        "#,
    );
}

#[test]
fn sets_describe_themselves() {
    let doc = document("doc/gregorian-calendar");
    assert_contains(
        &doc.graph,
        r#"
        <id/gregorian-calendar> a dgu:URIset ;
            <http://rdfs.org/ns/void#subset> <id/gregorian-year>, <id/gregorian-second>,
                <id/gregorian-instant>, <id/gregorian-interval> .
        "#,
    );
    let doc = document("doc/year");
    assert_contains(
        &doc.graph,
        r#"
        <id/year> dgu:itemType interval:CalendarYear ;
            <http://rdfs.org/ns/void#uriRegexPattern> "^https://ref\\.example/id/year/([0-9]{4}|[1-9][0-9]{4,})$" ;
            <http://rdfs.org/ns/void#exampleResource> <id/year/1752> ;
            dcterms:source <http://www.legislation.gov.uk/id/apgb/Geo2/24/23> .
        "#,
    );
    document("doc/interval");
    document("doc/geopoint/wgs84");
}

#[test]
fn every_document_survives_every_serialisation() {
    for path in [
        "doc/year/2006",
        "doc/month/1752-09",
        "doc/gregorian-minute/2006-01-01T12:30",
        "doc/gregorian-interval/2006-01-01T00:00:00/P1Y2M3DT4H5M6S",
        "doc/geopoint/wgs84/X0Y0",
        "doc/uk-calendar",
    ] {
        let graph = document(path).graph;
        for format in [
            RdfFormat::Turtle,
            RdfFormat::NTriples,
            RdfFormat::RdfXml,
            RdfFormat::JsonLd {
                profile: oxrdfio::JsonLdProfileSet::empty(),
            },
        ] {
            let mut writer = RdfSerializer::from_format(format).for_writer(Vec::new());
            for triple in &graph {
                writer.serialize_triple(triple).expect("serialise");
            }
            let bytes = writer.finish().expect("finish");
            let back: Graph = RdfParser::from_format(format)
                .for_slice(&bytes)
                .map(|q| Triple::from(q.expect("parses back")))
                .collect();
            assert_eq!(back.len(), graph.len(), "{path} as {format:?}");
            for triple in &graph {
                let ground = matches!(triple.subject, oxrdf::NamedOrBlankNodeRef::NamedNode(_))
                    && !matches!(triple.object, TermRef::BlankNode(_));
                if ground {
                    assert!(back.contains(triple), "{path} as {format:?} lost {triple}");
                }
            }
        }
    }
}

#[test]
fn id_redirects_to_doc_and_odd_spellings_to_the_canonical_one() {
    let b = base();
    match resolve(&b, "id/quarter/2006-Q1") {
        Outcome::SeeOther(l) => assert_eq!(l, format!("{BASE}doc/quarter/2006-Q1")),
        other => panic!("{other:?}"),
    }
    match resolve(&b, "id/quarter/2006-Q1.ttl") {
        Outcome::SeeOther(l) => assert_eq!(l, format!("{BASE}doc/quarter/2006-Q1.ttl")),
        other => panic!("{other:?}"),
    }
    match resolve(&b, "doc/gregorian-interval/2006-01-01T00:00:00/P0Y3M.ttl") {
        Outcome::Moved(l) => {
            assert_eq!(
                l,
                format!("{BASE}doc/gregorian-interval/2006-01-01T00:00:00/P3M.ttl")
            );
        }
        other => panic!("{other:?}"),
    }
    match resolve(&b, "id/geopoint/wgs84/X-01.50Y51.250") {
        Outcome::Moved(l) => assert_eq!(l, format!("{BASE}id/geopoint/wgs84/X-1.5Y51.25")),
        other => panic!("{other:?}"),
    }
    let doc = document("doc/second/2006-01-01T00%3A00%3A00.json");
    assert_eq!(doc.format, Some(Format::JsonLd));
    assert_eq!(
        doc.location,
        format!("{BASE}doc/second/2006-01-01T00:00:00.json")
    );
    assert_eq!(doc.generic, format!("{BASE}doc/second/2006-01-01T00:00:00"));
    for missing in [
        "id/",
        "doc/year/2006.xml",
        "doc/government-year/2006",
        "other/year/2006",
        "doc/year/2006%ZZ",
    ] {
        assert!(
            matches!(resolve(&b, missing), Outcome::NotFound),
            "{missing}"
        );
    }
}

#[test]
fn bases_are_checked_and_normalised() {
    assert_eq!(
        Base::new("HTTPS://Ref.Example/ref").unwrap().as_str(),
        "https://ref.example/ref/"
    );
    assert_eq!(
        Base::new("https://ref.example/ref/").unwrap().path(),
        "/ref/"
    );
    assert_eq!(Base::new("http://ref.example").unwrap().path(), "/");
    assert_eq!(
        Base::for_host("Ref.Example:8443").unwrap().as_str(),
        "https://ref.example:8443/"
    );
    assert_eq!(
        Base::for_host("[::1]:7878").unwrap().as_str(),
        "https://[::1]:7878/"
    );
    for bad in [
        "ftp://ref.example/",
        "ref.example",
        "https://ref.example/?q",
        "https://bad host/",
        "https:///x",
    ] {
        assert!(Base::new(bad).is_err(), "{bad}");
    }
    for bad in [
        "",
        "evil/x",
        "a b",
        "ref.example:99999",
        "ref.example:",
        "-x.example",
        "[::1",
        "x>y",
    ] {
        assert!(Base::for_host(bad).is_err(), "{bad}");
    }
}
