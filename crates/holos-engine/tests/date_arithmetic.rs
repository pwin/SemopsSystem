//! Adding a duration to a date, time or dateTime gives an answer for every value.
//!
//! 0.26.3 turned on spareval's `sep-0002`, which brings date and duration arithmetic, and the
//! sum of a dateTime and a duration was then unbound for some values and not others: of the
//! thirty pairs below, the native build bound 7 and the WebAssembly build 24. sparopt's
//! optimizer rebuilds every `+` through `order_pair`, which orders the two operands by hash,
//! and spareval only accepted the temporal value first, so whenever the duration hashed lower
//! the result was unbound. XPath's operator mapping defines `+` with the duration on either
//! side, and the copy in `vendor/spareval` accepts both. These tests go through the whole
//! engine, optimizer included, and compare each sum with `oxsdatatypes` computing it directly.

use holos_engine::Engine;
use holos_security::Session;
use oxrdfio::RdfFormat;
use oxsdatatypes::{Date, DateTime, Duration, Time};
use spareval::QueryResults;
use std::str::FromStr;

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

fn value(expression: &str) -> Option<String> {
    let mut e = Engine::new();
    e.bulk_load(&b""[..], RdfFormat::Turtle, None)
        .expect("load");
    let session = Session::unrestricted(e.store()).expect("session");
    let view = e.view(&session);
    let query = format!("PREFIX xsd: <{XSD}> SELECT ?v WHERE {{ BIND({expression} AS ?v) }}");
    let results = Engine::query(&view, &query, None).expect("query");
    let QueryResults::Solutions(mut iter) = results else {
        panic!("expected a solutions result");
    };
    let solution = iter.next().expect("one row").expect("solution");
    solution.get("v").map(|t| match t {
        oxrdf::Term::Literal(l) => l.value().to_owned(),
        other => other.to_string(),
    })
}

const DATE_TIMES: [&str; 6] = [
    "2025-03-08T00:00:00Z",
    "2024-01-01T12:00:00Z",
    "2026-11-18T09:30:00",
    "1999-12-31T23:59:59Z",
    "2025-07-15T00:00:00",
    "2030-02-28T10:00:00Z",
];

const DURATIONS: [(&str, &str); 5] = [
    ("P1D", "dayTimeDuration"),
    ("PT1H", "dayTimeDuration"),
    ("P1M", "yearMonthDuration"),
    ("P2Y", "yearMonthDuration"),
    ("P1DT2H", "duration"),
];

#[test]
fn a_duration_adds_to_a_date_time_in_either_order() {
    let mut unbound = Vec::new();
    for dt in DATE_TIMES {
        for (dur, ty) in DURATIONS {
            let expected = DateTime::from_str(dt)
                .expect("dateTime")
                .checked_add_duration(Duration::from_str(dur).expect("duration"))
                .expect("in range")
                .to_string();
            let lit_dt = format!("\"{dt}\"^^xsd:dateTime");
            let lit_dur = format!("\"{dur}\"^^xsd:{ty}");
            for (expression, order) in [
                (format!("{lit_dt} + {lit_dur}"), "dateTime + duration"),
                (format!("{lit_dur} + {lit_dt}"), "duration + dateTime"),
            ] {
                match value(&expression) {
                    Some(got) => assert_eq!(got, expected, "{expression} ({order})"),
                    None => unbound.push(expression),
                }
            }
        }
    }
    assert!(unbound.is_empty(), "unbound: {unbound:#?}");
}

#[test]
fn known_sums() {
    let cases = [
        (
            "\"2025-03-08T00:00:00Z\"^^xsd:dateTime + \"P2Y\"^^xsd:yearMonthDuration",
            "2027-03-08T00:00:00Z",
        ),
        (
            "\"P2Y\"^^xsd:yearMonthDuration + \"2025-03-08T00:00:00Z\"^^xsd:dateTime",
            "2027-03-08T00:00:00Z",
        ),
        (
            "\"2025-03-08\"^^xsd:date + \"P1M\"^^xsd:yearMonthDuration",
            "2025-04-08",
        ),
        (
            "\"P1M\"^^xsd:yearMonthDuration + \"2025-03-08\"^^xsd:date",
            "2025-04-08",
        ),
        (
            "\"10:00:00\"^^xsd:time + \"PT1H\"^^xsd:dayTimeDuration",
            "11:00:00",
        ),
        (
            "\"PT1H\"^^xsd:dayTimeDuration + \"10:00:00\"^^xsd:time",
            "11:00:00",
        ),
    ];
    for (expression, expected) in cases {
        assert_eq!(value(expression).as_deref(), Some(expected), "{expression}");
    }
}

#[test]
fn dates_and_times_take_a_duration_in_either_order() {
    for (dur, ty) in DURATIONS {
        let date = Date::from_str("2025-03-08").expect("date");
        if let Some(expected) =
            date.checked_add_duration(Duration::from_str(dur).expect("duration"))
        {
            let expected = expected.to_string();
            let lit = format!("\"{dur}\"^^xsd:{ty}");
            assert_eq!(
                value(&format!("\"2025-03-08\"^^xsd:date + {lit}")).as_deref(),
                Some(&*expected)
            );
            assert_eq!(
                value(&format!("{lit} + \"2025-03-08\"^^xsd:date")).as_deref(),
                Some(&*expected)
            );
        }
    }
    let time = Time::from_str("10:00:00").expect("time");
    let expected = time
        .checked_add_duration(Duration::from_str("PT90M").expect("duration"))
        .expect("in range")
        .to_string();
    assert_eq!(
        value("\"10:00:00\"^^xsd:time + \"PT90M\"^^xsd:dayTimeDuration").as_deref(),
        Some(&*expected)
    );
    assert_eq!(
        value("\"PT90M\"^^xsd:dayTimeDuration + \"10:00:00\"^^xsd:time").as_deref(),
        Some(&*expected)
    );
}

#[test]
fn subtraction_is_not_commutative() {
    // A duration less a dateTime means nothing, in XPath and here.
    assert_eq!(
        value("\"2025-03-08T00:00:00Z\"^^xsd:dateTime - \"P1D\"^^xsd:dayTimeDuration").as_deref(),
        Some("2025-03-07T00:00:00Z")
    );
    assert_eq!(
        value("\"P1D\"^^xsd:dayTimeDuration - \"2025-03-08T00:00:00Z\"^^xsd:dateTime"),
        None
    );
}
