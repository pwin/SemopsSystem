//! Version comparison reports what the suite's `version-diff` reports.
//!
//! `fixtures/expected/diff/<case>.json` and `.txt` are the suite's own output
//! (ontology-quality-suite 0.28.0, `version-diff OLD NEW --exclude-imports --json`), run
//! from `fixtures/diff/`, for the pairs below. Between them they make every section of the
//! report appear, and `cycle` compares domains and ranges across a subclass cycle. The JSON
//! is compared with every list sorted, since the suite writes edges and pairs in set order;
//! the text is compared line for line.

use semops_checks::diff;
use semops_checks::run;
use serde_json::Value;
use std::path::PathBuf;

const CASES: &[(&str, &str, &str)] = &[
    ("same", "v1.ttl", "v1.ttl"),
    ("patch", "v1.ttl", "v2-patch.ttl"),
    ("minor", "v1.ttl", "v2-minor.ttl"),
    ("major", "v1.ttl", "v2-major.ttl"),
    ("reverse", "v2-major.ttl", "v1.ttl"),
    ("cycle", "cycle-old.ttl", "cycle-new.ttl"),
];

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn load(name: &str) -> Vec<oxrdf::Triple> {
    let path = fixtures().join("diff").join(name);
    run::load(&[path.as_path()]).expect("the fixture parses")
}

/// The value with every array sorted, recursively, so set order does not matter.
fn normalised(value: Value) -> Value {
    match value {
        Value::Array(items) => {
            let mut items: Vec<Value> = items.into_iter().map(normalised).collect();
            items.sort_by_key(|v| v.to_string());
            Value::Array(items)
        }
        Value::Object(map) => {
            Value::Object(map.into_iter().map(|(k, v)| (k, normalised(v))).collect())
        }
        other => other,
    }
}

fn expected(case: &str, extension: &str) -> String {
    let path: PathBuf = fixtures()
        .join("expected/diff")
        .join(format!("{case}.{extension}"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n")
}

#[test]
fn the_json_is_the_suites() {
    for (case, old, new) in CASES {
        let (d, bump) = diff::diff(&load(old), &load(new));
        let ours = normalised(diff::to_json(&d, bump));
        let theirs = normalised(serde_json::from_str(&expected(case, "json")).expect("JSON"));
        assert_eq!(ours, theirs, "{case}");
    }
}

#[test]
fn the_text_report_is_the_suites() {
    for (case, old, new) in CASES {
        let (d, bump) = diff::diff(&load(old), &load(new));
        assert_eq!(
            diff::report(&d, bump, old, new),
            expected(case, "txt"),
            "{case}"
        );
    }
}

#[test]
fn each_case_calls_for_the_bump_it_was_written_for() {
    let bump = |old: &str, new: &str| diff::diff(&load(old), &load(new)).1;
    assert_eq!(bump("v1.ttl", "v1.ttl"), diff::Bump::None);
    assert_eq!(bump("v1.ttl", "v2-patch.ttl"), diff::Bump::Patch);
    assert_eq!(bump("v1.ttl", "v2-minor.ttl"), diff::Bump::Minor);
    assert_eq!(bump("v1.ttl", "v2-major.ttl"), diff::Bump::Major);
    // Undoing a breaking change can break too: `sells` moves from range Ledger back to
    // Book, which are incomparable.
    assert_eq!(bump("v2-major.ttl", "v1.ttl"), diff::Bump::Major);
}

#[test]
fn a_blank_node_class_expression_is_not_compared() {
    let restriction = |n: u32| {
        format!(
            "@prefix owl: <http://www.w3.org/2002/07/owl#> .
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
             <urn:C> a owl:Class ; rdfs:subClassOf [ a owl:Restriction ;
               owl:onProperty <urn:p> ; owl:minCardinality {n} ] ."
        )
    };
    let parse = |text: String| {
        oxrdfio::RdfParser::from_format(oxrdfio::RdfFormat::Turtle)
            .for_slice(text.as_bytes())
            .map(|q| oxrdf::Triple::from(q.expect("Turtle")))
            .collect::<Vec<_>>()
    };
    let (_, bump) = diff::diff(&parse(restriction(1)), &parse(restriction(2)));
    assert_eq!(
        bump,
        diff::Bump::None,
        "as in the suite, anonymous expressions are out of scope"
    );
}
