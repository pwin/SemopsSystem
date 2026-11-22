//! The gist descriptions: what they say about calendar intervals, instants, intervals and
//! points; that every term they use is declared in gist 14.1.0 or the placetime ontology; and
//! that they conform to the placetime shapes, run by the SHACL engine.

use oxrdf::vocab::{rdf, rdfs};
use oxrdf::{Graph, NamedNode, NamedNodeRef, Term, TermRef, Triple};
use oxrdfio::{RdfFormat, RdfParser};
use semops_placetime::{
    Base, ONTOLOGY, Outcome, SHAPES, Vocabulary, describe_with, resolve, resolve_with,
};
use std::collections::BTreeSet;

const BASE: &str = "https://ref.example/";
const GIST: &str = "https://w3id.org/semanticarts/ns/ontology/gist/";
const GISTD: &str = "https://w3id.org/semanticarts/ns/data/gist/";
const PT: &str = "https://www.semantechs.co.uk/ns/ontology/placetime/";
const PTD: &str = "https://www.semantechs.co.uk/ns/data/placetime/";
const PTX: &str = "https://www.semantechs.co.uk/ns/taxonomy/placetime/";
const GIST_CORE: &str = include_str!("../ontology/gist/gistCore14.1.0.ttl");
/// The gist reference data's starter set, release 1.0.0: aspects, unit groups and units.
const GIST_REFERENCE_DATA: &str = include_str!("../ontology/gist/uomReferenceData-starter-set.ttl");

/// A path of every kind, in every calendar, with the edge cases: the British calendar's
/// change, a leap month, the Julian calendar's leap centuries, and sets.
const PATHS: &[&str] = &[
    "year/1752",
    "half/1752-H2",
    "quarter/1752-Q3",
    "month/1752-09",
    "week/1752-W36",
    "day/1752-09-14",
    "hour/1752-09-14T12",
    "minute/1752-09-14T12:30",
    "second/1752-09-14T12:30:59",
    "gregorian-year/2024",
    "gregorian-half/2024-H1",
    "gregorian-quarter/2024-Q4",
    "gregorian-month/2024-02",
    "gregorian-week/2024-W01",
    "gregorian-day/2024-02-29",
    "julian-year/1700",
    "julian-month/1700-02",
    "julian-day/1582-10-04",
    "gregorian-instant/2024-02-29T23:59:59",
    "gregorian-interval/2016-06-30T00:00:00/P3M",
    "gregorian-interval/2024-02-29T12:00:00/P1DT2H",
    "geopoint/wgs84/X-0.1276Y51.5072",
    "geopoint/wgs84/X180Y-90",
    "year",
    "gregorian-calendar",
    "interval",
    "gregorian-instant",
    "gregorian-interval",
    "geopoint/wgs84",
];

fn base() -> Base {
    Base::new(BASE).expect("base")
}

fn gist(path: &str) -> Graph {
    describe_with(&base(), path, Vocabulary::Gist)
        .unwrap_or_else(|| panic!("{path}: describes nothing"))
}

fn gist_document(path: &str) -> Graph {
    match resolve_with(&base(), &format!("doc/{path}"), Vocabulary::Gist) {
        Outcome::Document(d) => d.graph,
        other => panic!("{path}: expected a document, got {other:?}"),
    }
}

fn parse(turtle: &str) -> Vec<Triple> {
    RdfParser::from_format(RdfFormat::Turtle)
        .for_slice(turtle.as_bytes())
        .map(|q| Triple::from(q.expect("Turtle parses")))
        .collect()
}

fn iri(ns: &str, local: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("{ns}{local}"))
}

fn id(path: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("{BASE}id/{path}"))
}

fn has(graph: &Graph, s: &NamedNode, p: &NamedNode, o: impl Into<Term>) -> bool {
    graph.contains(&Triple::new(s.clone(), p.clone(), o.into()))
}

fn date_time(lexical: &str) -> Term {
    oxrdf::Literal::new_typed_literal(lexical, oxrdf::vocab::xsd::DATE_TIME).into()
}

/// The topic's start, end and duration in seconds.
fn span(graph: &Graph, topic: &NamedNode) -> (String, String, i64) {
    let value = |p: &str| match graph.object_for_subject_predicate(topic, &iri(GIST, p)) {
        Some(TermRef::Literal(l)) => l.value().to_owned(),
        other => panic!("{topic} {p}: {other:?}"),
    };
    let magnitude = match graph.object_for_subject_predicate(topic, &iri(GIST, "hasMagnitude")) {
        Some(TermRef::BlankNode(b)) => b.into_owned(),
        other => panic!("{topic} hasMagnitude: {other:?}"),
    };
    let seconds = match graph.object_for_subject_predicate(&magnitude, &iri(GIST, "numericValue")) {
        Some(TermRef::Literal(l)) => l.value().parse().expect("an integer"),
        other => panic!("{topic} duration: {other:?}"),
    };
    (value("startDateTime"), value("endDateTime"), seconds)
}

#[test]
fn a_leap_february_in_the_gregorian_calendar() {
    let g = gist_document("gregorian-month/2024-02");
    let topic = id("gregorian-month/2024-02");
    assert!(has(
        &g,
        &topic,
        &rdf::TYPE.into_owned(),
        iri(PT, "CalendarMonth")
    ));
    assert_eq!(
        span(&g, &topic),
        (
            "2024-02-01T00:00:00".to_owned(),
            "2024-03-01T00:00:00".to_owned(),
            29 * 86_400
        )
    );
    assert!(has(
        &g,
        &topic,
        &iri(GIST, "conformsTo"),
        iri(PTD, "_Calendar_Gregorian")
    ));
    assert!(has(
        &g,
        &topic,
        &iri(GIST, "isCategorizedBy"),
        iri(PTX, "_MonthOfYear_February")
    ));
    let quarter = id("gregorian-quarter/2024-Q1");
    assert!(has(
        &g,
        &topic,
        &iri(GIST, "isDirectPartOf"),
        quarter.clone()
    ));
    assert!(has(
        &g,
        &topic,
        &iri(GIST, "isPartOf"),
        id("gregorian-half/2024-H1")
    ));
    assert!(has(
        &g,
        &topic,
        &iri(GIST, "isPartOf"),
        id("gregorian-year/2024")
    ));
    assert!(has(
        &g,
        &topic,
        &iri(GIST, "isMemberOf"),
        id("gregorian-month")
    ));
    assert!(has(
        &g,
        &id("gregorian-month/2024-01"),
        &iri(GIST, "precedesDirectly"),
        topic.clone()
    ));
    assert!(has(
        &g,
        &topic,
        &iri(GIST, "precedesDirectly"),
        id("gregorian-month/2024-03")
    ));
    let days = g
        .triples_for_predicate(&iri(GIST, "isDirectPartOf"))
        .filter(|t| t.object == TermRef::NamedNode(topic.as_ref()))
        .count();
    assert_eq!(days, 29, "each day of the month is directly part of it");
    // The quarter the month is named in carries its own span: every interval a document
    // names is complete.
    assert_eq!(
        span(&g, &quarter),
        (
            "2024-01-01T00:00:00".to_owned(),
            "2024-04-01T00:00:00".to_owned(),
            91 * 86_400
        )
    );
}

#[test]
fn the_british_year_of_the_calendar_change_is_355_days() {
    let g = gist("year/1752");
    let topic = id("year/1752");
    // Julian 1 January 1752 is Gregorian 12 January; the year ends as the Gregorian 1753 begins.
    assert_eq!(
        span(&g, &topic),
        (
            "1752-01-12T00:00:00".to_owned(),
            "1753-01-01T00:00:00".to_owned(),
            355 * 86_400
        )
    );
    assert!(has(
        &g,
        &topic,
        &iri(GIST, "conformsTo"),
        iri(PTD, "_Calendar_British")
    ));
}

#[test]
fn the_day_after_wednesday_2_september_1752_is_thursday_the_14th() {
    let g = gist("day/1752-09-14");
    let topic = id("day/1752-09-14");
    assert!(has(
        &g,
        &id("day/1752-09-02"),
        &iri(GIST, "precedesDirectly"),
        topic.clone()
    ));
    assert!(has(
        &g,
        &topic,
        &iri(GIST, "isCategorizedBy"),
        iri(PTX, "_DayOfWeek_Thursday")
    ));
    assert!(has(
        &g,
        &topic,
        &iri(GIST, "isDirectPartOf"),
        id("month/1752-09")
    ));
    // Week 36 runs from Monday 31 August to Sunday 17 September: seven days across the gap.
    assert!(has(
        &g,
        &topic,
        &iri(GIST, "isDirectPartOf"),
        id("week/1752-W36")
    ));
    assert_eq!(span(&g, &topic).2, 86_400);
}

#[test]
fn a_julian_leap_century() {
    let g = gist("julian-year/1700");
    // 1700 is a leap year in the Julian calendar and not in the Gregorian: the Julian year
    // starts ten days behind and ends eleven days behind.
    assert_eq!(
        span(&g, &id("julian-year/1700")),
        (
            "1700-01-11T00:00:00".to_owned(),
            "1701-01-12T00:00:00".to_owned(),
            366 * 86_400
        )
    );
}

#[test]
fn an_instant_is_a_time_interval_of_no_length() {
    let g = gist_document("gregorian-instant/2024-02-29T23:59:59");
    let topic = id("gregorian-instant/2024-02-29T23:59:59");
    assert!(has(&g, &topic, &rdf::TYPE.into_owned(), iri(PT, "Instant")));
    assert_eq!(
        span(&g, &topic),
        (
            "2024-02-29T23:59:59".to_owned(),
            "2024-02-29T23:59:59".to_owned(),
            0
        )
    );
    assert!(has(
        &g,
        &topic,
        &iri(GIST, "isMemberOf"),
        id("gregorian-instant")
    ));
}

#[test]
fn an_interval_of_any_duration() {
    let g = gist("gregorian-interval/2016-06-30T00:00:00/P3M");
    let topic = id("gregorian-interval/2016-06-30T00:00:00/P3M");
    assert!(has(
        &g,
        &topic,
        &rdf::TYPE.into_owned(),
        iri(GIST, "TimeInterval")
    ));
    assert_eq!(
        span(&g, &topic),
        (
            "2016-06-30T00:00:00".to_owned(),
            "2016-09-30T00:00:00".to_owned(),
            92 * 86_400
        )
    );
}

#[test]
fn a_point() {
    let g = gist("geopoint/wgs84/X-0.1276Y51.5072");
    let topic = id("geopoint/wgs84/X-0.1276Y51.5072");
    assert!(has(
        &g,
        &topic,
        &rdf::TYPE.into_owned(),
        iri(GIST, "GeoPoint")
    ));
    let double = |v: &str| -> Term {
        oxrdf::Literal::new_typed_literal(v, oxrdf::vocab::xsd::DOUBLE).into()
    };
    assert!(has(&g, &topic, &iri(GIST, "latitude"), double("51.5072")));
    assert!(has(&g, &topic, &iri(GIST, "longitude"), double("-0.1276")));
}

#[test]
fn the_default_vocabulary_is_unchanged() {
    // resolve() is the reference.data.gov.uk vocabulary, and says nothing in gist.
    let Outcome::Document(d) = resolve(&base(), "doc/gregorian-month/2024-02") else {
        panic!("a document");
    };
    assert!(
        d.graph
            .iter()
            .all(|t| !t.predicate.as_str().starts_with(GIST))
    );
    assert!(gist("gregorian-month/2024-02").iter().all(|t| {
        !t.predicate
            .as_str()
            .starts_with("http://reference.data.gov.uk/")
    }));
    assert_eq!(Vocabulary::parse("gist"), Some(Vocabulary::Gist));
    assert_eq!(
        Vocabulary::parse("data.gov.uk"),
        Some(Vocabulary::DataGovUk)
    );
    assert_eq!(Vocabulary::parse("time"), None);
    assert_eq!(Vocabulary::default(), Vocabulary::DataGovUk);
}

#[test]
fn starts_and_ends_are_xsd_date_times_in_the_gregorian_calendar() {
    // The British day 3 September 1752 does not exist; its neighbours meet across the gap.
    let g = gist("day/1752-09-02");
    assert!(has(
        &g,
        &id("day/1752-09-02"),
        &iri(GIST, "endDateTime"),
        date_time("1752-09-14T00:00:00")
    ));
}

/// Every IRI declared as something in a Turtle document.
fn declared(triples: &[Triple]) -> BTreeSet<String> {
    triples
        .iter()
        .filter(|t| t.predicate == rdf::TYPE)
        .filter_map(|t| match &t.subject {
            oxrdf::NamedOrBlankNode::NamedNode(n) => Some(n.as_str().to_owned()),
            oxrdf::NamedOrBlankNode::BlankNode(_) => None,
        })
        .collect()
}

#[test]
fn every_gist_and_placetime_term_used_is_declared() {
    let mut known = declared(&parse(GIST_CORE));
    known.extend(declared(&parse(GIST_REFERENCE_DATA)));
    known.extend(declared(&parse(ONTOLOGY)));
    let ours = |s: &str| {
        [GIST, GISTD, PT, PTD, PTX]
            .iter()
            .any(|ns| s.starts_with(ns))
    };
    let mut undeclared = BTreeSet::new();
    for path in PATHS {
        for t in gist_document(path).iter() {
            let mut used = vec![t.predicate.as_str()];
            if let TermRef::NamedNode(o) = t.object {
                used.push(o.as_str());
            }
            for term in used {
                if ours(term) && !known.contains(term) {
                    undeclared.insert(format!("{term} (in {path})"));
                }
            }
        }
    }
    assert!(undeclared.is_empty(), "undeclared: {undeclared:#?}");
}

#[test]
fn the_unit_and_aspect_of_a_duration_are_the_reference_datas() {
    let data = parse(GIST_REFERENCE_DATA);
    let says =
        |s: &NamedNode, p: &str, o: Term| data.contains(&Triple::new(s.clone(), iri(GIST, p), o));
    let second = iri(GISTD, "_UnitOfMeasure_second");
    let group = iri(GISTD, "_UnitGroup_duration");
    let decimal = |v: &str| -> Term {
        oxrdf::Literal::new_typed_literal(v, oxrdf::vocab::xsd::DECIMAL).into()
    };
    // The second is the reference data's base unit of duration, so a duration in seconds
    // needs no conversion.
    assert!(data.contains(&Triple::new(
        second.clone(),
        rdf::TYPE,
        iri(GIST, "UnitOfMeasure")
    )));
    assert!(says(&second, "isMemberOf", group.clone().into()));
    assert!(says(&second, "conversionFactor", decimal("1.0")));
    assert!(says(&group, "exponentOfSecond", decimal("1.0")));
    assert!(says(
        &iri(GISTD, "_Aspect_duration"),
        "hasUnitGroup",
        group.into()
    ));
    // What the ontology repeats about the second, the reference data says too: the copy
    // adds only its own annotations.
    let annotations = [
        "http://www.w3.org/2000/01/rdf-schema#seeAlso",
        "http://www.w3.org/2004/02/skos/core#editorialNote",
    ];
    for t in parse(ONTOLOGY) {
        if t.subject == oxrdf::NamedOrBlankNode::from(second.clone())
            && !annotations.contains(&t.predicate.as_str())
        {
            assert!(data.contains(&t), "not in the reference data: {t}");
        }
    }
}

#[test]
fn the_ontology_follows_gist_conventions() {
    let triples = parse(ONTOLOGY);
    let pref = NamedNodeRef::new_unchecked("http://www.w3.org/2004/02/skos/core#prefLabel");
    let definition = NamedNodeRef::new_unchecked("http://www.w3.org/2004/02/skos/core#definition");
    let class = NamedNodeRef::new_unchecked("http://www.w3.org/2002/07/owl#Class");
    let count = |s: &oxrdf::NamedOrBlankNode, p: NamedNodeRef<'_>| {
        triples
            .iter()
            .filter(|t| &t.subject == s && t.predicate == p)
            .count()
    };
    let mut classes = 0;
    for t in &triples {
        if t.predicate == rdf::TYPE
            && t.object == Term::from(class.into_owned())
            && matches!(t.subject, oxrdf::NamedOrBlankNode::NamedNode(_))
        {
            classes += 1;
            assert_eq!(
                count(&t.subject, pref),
                1,
                "{}: one skos:prefLabel",
                t.subject
            );
            assert_eq!(
                count(&t.subject, definition),
                1,
                "{}: one skos:definition",
                t.subject
            );
        }
        // gist uses SKOS annotations, never rdfs:label or rdfs:comment.
        assert!(
            t.predicate != rdfs::LABEL && t.predicate != rdfs::COMMENT,
            "{t}"
        );
    }
    assert_eq!(classes, 16);
    // The ontology declares no properties of its own: gist's serve.
    for kind in ["ObjectProperty", "DatatypeProperty", "AnnotationProperty"] {
        let kind = Term::from(iri("http://www.w3.org/2002/07/owl#", kind));
        assert!(
            !triples
                .iter()
                .any(|t| t.predicate == rdf::TYPE && t.object == kind),
            "a new {kind}"
        );
    }
}

mod shapes {
    //! The placetime shapes, compiled and run by the SHACL engine over each description
    //! with the ontology beside it.

    use super::*;
    use shacl::model::{GraphBuilder, scope};
    use shacl::{TermStore, Vocab};

    fn intern(
        store: &mut TermStore,
        triples: impl IntoIterator<Item = Triple>,
        scope: u32,
    ) -> shacl::Graph {
        let mut builder = GraphBuilder::new();
        for t in triples {
            let s = store.intern_oxrdf(Term::from(t.subject).as_ref(), scope);
            let p = store.intern_oxrdf(Term::from(t.predicate).as_ref(), scope);
            let o = store.intern_oxrdf(t.object.as_ref(), scope);
            builder.push(s, p, o);
        }
        builder.build()
    }

    /// The messages of every result validating `data`, with the ontology, against the shapes.
    fn violations(data: impl IntoIterator<Item = Triple>) -> Vec<String> {
        let mut store = TermStore::new();
        let vocab = Vocab::new(&mut store);
        let shapes = intern(&mut store, parse(SHAPES), scope::SHAPES);
        let compiled =
            shacl::shapes::Shapes::compile(&shapes, &store, &vocab).expect("the shapes compile");
        let mut all = parse(ONTOLOGY);
        all.extend(data);
        let data = intern(&mut store, all, scope::DATA);
        let report = shacl::validate::validate_in_with(
            &data,
            &compiled,
            &shapes,
            &mut store,
            &vocab,
            shacl::validate::Options {
                max_results: None,
                blocking: None,
                threads: 0,
            },
        )
        .expect("validation runs");
        report
            .results
            .iter()
            .map(|r| {
                let focus = store.to_oxrdf(r.focus_node).to_string();
                let message = r
                    .messages
                    .iter()
                    .map(|m| store.to_oxrdf(*m).to_string())
                    .collect::<Vec<_>>()
                    .join("; ");
                format!("{focus}: {message}")
            })
            .collect()
    }

    #[test]
    fn every_description_conforms() {
        for path in PATHS {
            let found = violations(gist_document(path).iter().map(|t| t.into_owned()));
            assert!(found.is_empty(), "{path}: {found:#?}");
        }
    }

    #[test]
    fn a_wrong_duration_is_caught() {
        let g = gist("gregorian-day/2024-02-29");
        let numeric = iri(GIST, "numericValue");
        let tampered: Vec<Triple> = g
            .iter()
            .map(|t| {
                let t = t.into_owned();
                if t.predicate == numeric {
                    Triple::new(
                        t.subject,
                        t.predicate,
                        oxrdf::Literal::new_typed_literal("86401", oxrdf::vocab::xsd::INTEGER),
                    )
                } else {
                    t
                }
            })
            .collect();
        let found = violations(tampered);
        assert!(
            found
                .iter()
                .any(|v| v.contains("is not the end less the start")),
            "{found:#?}"
        );
    }

    #[test]
    fn a_missing_calendar_and_category_are_caught() {
        let g = gist("gregorian-day/2024-02-29");
        let dropped = [iri(GIST, "conformsTo"), iri(GIST, "isCategorizedBy")];
        let found = violations(
            g.iter()
                .map(|t| t.into_owned())
                .filter(|t| !dropped.contains(&t.predicate)),
        );
        assert!(
            found.iter().any(|v| v.contains("conforms to one calendar")),
            "{found:#?}"
        );
        assert!(
            found.iter().any(|v| v.contains("day of the week")),
            "{found:#?}"
        );
    }

    #[test]
    fn an_instant_with_length_is_caught() {
        let g = gist("gregorian-instant/2024-02-29T23:59:59");
        let end = iri(GIST, "endDateTime");
        let found = violations(g.iter().map(|t| t.into_owned()).map(|t| {
            if t.predicate == end {
                Triple::new(t.subject, t.predicate, date_time("2024-03-01T00:00:00"))
            } else {
                t
            }
        }));
        assert!(
            found
                .iter()
                .any(|v| v.contains("starts and ends at the same moment")),
            "{found:#?}"
        );
    }
}
