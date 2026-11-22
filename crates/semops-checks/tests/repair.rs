//! Repairs do what the suite's repairs do.
//!
//! `fixtures/expected/repair-shop.json` is the suite's own output (ontology-quality-suite
//! 0.28.0): `pipeline.run_registry_suite_on_graph` over `fixtures/repair/shop.ttl`, then
//! `checks.repair.compute_repair` for every finding whose check has a template, each against
//! the document as it stands, with the default project standards. Each repair is compared by
//! the triples it adds and removes.

use semops_checks::repair::{self, Standards};
use semops_checks::run;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Expected {
    check_id: String,
    focus_node: String,
    path: Option<String>,
    value: Option<String>,
    kind: String,
    title: String,
    added: Vec<String>,
    removed: Vec<String>,
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn nt(triples: &[oxrdf::Triple]) -> Vec<String> {
    let mut lines: Vec<String> = triples.iter().map(|t| format!("{t} .")).collect();
    lines.sort();
    lines
}

#[test]
fn each_repair_changes_what_the_suites_does() {
    let (document, prefixes) =
        repair::load_document(&fixtures().join("repair/shop.ttl")).expect("the fixture parses");
    let standards = Standards::default();
    let report = run::check(&document).expect("the checks run");

    let mut ours = BTreeMap::new();
    for finding in &report.findings {
        let Some(outcome) =
            repair::compute(&document, finding, &standards, &prefixes).expect("the repair runs")
        else {
            continue;
        };
        let key = (
            outcome.check_id.clone(),
            finding.focus_node.clone(),
            finding.path.clone(),
            finding.value.clone(),
        );
        let kind = match outcome.kind {
            repair::Kind::Insert => "insert",
            repair::Kind::Replace => "replace",
        };
        ours.insert(
            key,
            (
                kind,
                outcome.title,
                nt(&outcome.added),
                nt(&outcome.removed),
            ),
        );
    }

    let text = std::fs::read_to_string(fixtures().join("expected/repair-shop.json")).expect("read");
    let expected: Vec<Expected> = serde_json::from_str(&text).expect("JSON");
    assert_eq!(ours.len(), expected.len(), "one repair per fixable finding");
    for e in expected {
        let key = (
            e.check_id.clone(),
            e.focus_node.clone(),
            e.path.clone(),
            e.value.clone(),
        );
        let (kind, title, added, removed) = ours
            .get(&key)
            .unwrap_or_else(|| panic!("no repair for {key:?}"));
        assert_eq!(*kind, e.kind, "{key:?}");
        assert_eq!(*title, e.title, "{key:?}");
        assert_eq!(*added, e.added, "{key:?}: added");
        assert_eq!(*removed, e.removed, "{key:?}: removed");
    }
}

#[test]
fn a_policy_chooses_the_template() {
    let mut standards = Standards::default();
    let (text, _, kind) = repair::template("LOG-003", &standards).expect("LOG-003 has a repair");
    assert_eq!(kind, repair::Kind::Replace);
    standards.redundant_equivalence_policy = "keepSubClassOf".to_owned();
    let (other, _, _) = repair::template("LOG-003", &standards).expect("the other policy");
    assert_ne!(text, other);
    assert!(
        repair::template("STR-004", &standards).is_none(),
        "not every check has a repair"
    );
}

#[test]
fn an_added_block_reads_back_as_the_triples_it_adds() {
    let (document, prefixes) =
        repair::load_document(&fixtures().join("repair/shop.ttl")).expect("the fixture parses");
    let report = run::check(&document).expect("the checks run");
    let added: Vec<oxrdf::Triple> = report
        .findings
        .iter()
        .filter_map(|f| {
            repair::compute(&document, f, &Standards::default(), &prefixes)
                .ok()
                .flatten()
        })
        .filter(|o| o.kind == repair::Kind::Insert)
        .flat_map(|o| o.added)
        .collect();
    let block = repair::render_added(&added, &prefixes);
    let header: String = prefixes
        .iter()
        .map(|(p, ns)| format!("@prefix {p}: <{ns}> .\n"))
        .collect();
    let parsed: Vec<oxrdf::Triple> = oxrdfio::RdfParser::from_format(oxrdfio::RdfFormat::Turtle)
        .for_slice(format!("{header}{block}").as_bytes())
        .map(|q| oxrdf::Triple::from(q.expect("the block is Turtle")))
        .collect();
    assert_eq!(nt(&parsed), nt(&added));
}
