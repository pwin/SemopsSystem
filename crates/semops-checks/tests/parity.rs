//! The native runner reports what the Python suite reports.
//!
//! `fixtures/expected/<set>.<engine>.json` is the suite's own output (ontology-quality-suite
//! 0.28.0, `pipeline.run_registry_suite_on_graph(engine=…)`, for `sparql`, `shacl` and
//! `both`) over the four fixture sets its `tests/test_check_firing_coverage.py` sweeps, which
//! between them make 35 of the 42 portable checks fire. To regenerate it, install the suite
//! at the commit `PROVENANCE.md` names and dump each row's fields in the same shape.
//!
//! Compared as multisets: rows with equal sort keys come out of the suite in an order that
//! depends on rdflib's hashing. A blank node's label is masked, since it is the engine's
//! private name for the node and differs between runs and between engines; the suite says
//! so itself and merges on a count instead of the label.

use semops_checks::run::{self, Finding};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct Row {
    check_id: Option<String>,
    severity: String,
    focus_node: String,
    focus_is_blank: bool,
    path: Option<String>,
    value: Option<String>,
    message: String,
    sources: Vec<String>,
}

/// A row with what may legitimately differ masked out.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Comparable {
    check_id: Option<String>,
    severity: String,
    focus: String,
    path: Option<String>,
    value: Option<String>,
    message: String,
    sources: Vec<String>,
}

impl From<&Finding> for Row {
    fn from(f: &Finding) -> Self {
        Self {
            check_id: f.check_id.clone(),
            severity: f.severity.clone(),
            focus_node: f.focus_node.clone(),
            focus_is_blank: f.focus_is_blank,
            path: f.path.clone(),
            value: f.value.clone(),
            message: f.message.clone(),
            sources: f.sources.clone(),
        }
    }
}

fn comparable(row: Row) -> Comparable {
    let (blank, focus) = (row.focus_is_blank, row.focus_node);
    let mask = |s: Option<String>| {
        s.map(|s| {
            if blank && s == focus {
                "_:".to_owned()
            } else {
                s
            }
        })
    };
    let ordered = ORDER_DEPENDENT.contains(&row.check_id.as_deref().unwrap_or(""));
    let canon = |s: String| if ordered { unordered(&s) } else { s };
    Comparable {
        value: mask(row.value).map(canon),
        message: canon(row.message),
        focus: if blank { "_:".to_owned() } else { focus },
        path: row.path,
        sources: row.sources,
        check_id: row.check_id,
        severity: row.severity,
    }
}

/// Checks whose finding text depends on the order the engine returns rows in, which no
/// two engines (or two load orders) need agree on: `LOG-004` writes its message from
/// whichever of two symmetric solutions comes first ("inverses (A and B)"), and `STY-005`
/// joins its separators with an unordered `GROUP_CONCAT` ("_|-"). For these the members
/// of such a list are compared in sorted order; for every other check the text must match
/// exactly.
const ORDER_DEPENDENT: &[&str] = &["LOG-004", "STY-005"];

/// Sorts the members of every `(… and …)` and `|`-joined list in `text`.
fn unordered(text: &str) -> String {
    fn sort_members(group: &str) -> String {
        for separator in [" and ", "|"] {
            if group.contains(separator) {
                let mut parts: Vec<&str> = group.split(separator).collect();
                parts.sort_unstable();
                return parts.join(separator);
            }
        }
        group.to_owned()
    }
    if !text.contains('(') {
        return sort_members(text);
    }
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        out.push_str(&rest[..=open]);
        out.push_str(&sort_members(&rest[open + 1..open + close]));
        out.push(')');
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

fn engine_name(engine: run::Engine) -> &'static str {
    match engine {
        run::Engine::Sparql => "sparql",
        run::Engine::Shacl => "shacl",
        run::Engine::Both => "both",
    }
}

fn expected(set: &str, engine: run::Engine) -> Vec<Comparable> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/expected")
        .join(format!("{set}.{}.json", engine_name(engine)));
    let rows: Vec<Row> =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("expected rows"))
            .expect("JSON");
    rows.into_iter().map(comparable).collect()
}

fn actual(files: &[&str], engine: run::Engine) -> (Vec<Comparable>, run::Run) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let paths: Vec<_> = files.iter().map(|f| root.join(f)).collect();
    let refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
    let triples = run::load(&refs).expect("fixtures load");
    let registry = semops_checks::Registry::builtin();
    let report =
        run::check_with(&triples, registry, &semops_checks::queries(), engine).expect("run");
    let rows = report
        .findings
        .iter()
        .map(|f| comparable(Row::from(f)))
        .collect();
    (rows, report)
}

/// Asserts the two multisets are equal, listing what each has that the other lacks.
fn assert_same(set: &str, mut ours: Vec<Comparable>, mut theirs: Vec<Comparable>) {
    ours.sort();
    theirs.sort();
    let mut only_ours = Vec::new();
    let mut only_theirs = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < ours.len() || j < theirs.len() {
        match (ours.get(i), theirs.get(j)) {
            (Some(a), Some(b)) if a == b => {
                i += 1;
                j += 1;
            }
            (Some(a), Some(b)) if a < b => {
                only_ours.push(a.clone());
                i += 1;
            }
            (Some(_), Some(b)) => {
                only_theirs.push(b.clone());
                j += 1;
            }
            (Some(a), None) => {
                only_ours.push(a.clone());
                i += 1;
            }
            (None, Some(b)) => {
                only_theirs.push(b.clone());
                j += 1;
            }
            (None, None) => unreachable!(),
        }
    }
    assert!(
        only_ours.is_empty() && only_theirs.is_empty(),
        "{set}: {} rows only here, {} only in the suite\nonly here: {only_ours:#?}\nonly in the suite: {only_theirs:#?}",
        only_ours.len(),
        only_theirs.len()
    );
}

/// One fixture set under one engine, against the suite's rows for the same.
fn parity(set: &str, files: &[&str], engine: run::Engine) {
    let (mut ours, report) = actual(files, engine);
    assert!(report.complete(), "{:?}", report.queries);
    let mut theirs = expected(set, engine);
    if set == "stress" {
        // The one deliberate difference. rdflib rewrites "yesplz"^^xsd:boolean to "false" as
        // it parses, so the suite's finding quotes "false" and only its literal check sees
        // it. Read as written, every formulation sees the authored value, and the finding
        // quotes it.
        let item = "https://example.org/stress/boolItemOne";
        let about = |r: &Comparable| r.focus == item && r.check_id.as_deref() == Some("DAT-001");
        let before = theirs.len();
        theirs.retain(|r| !about(r));
        assert_eq!(
            theirs.len(),
            before - 1,
            "the suite reports the rewritten boolean once"
        );
        let found: Vec<_> = ours.iter().filter(|r| about(r)).cloned().collect();
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].value.as_deref(), Some("yesplz"));
        let sources: &[&str] = match engine {
            run::Engine::Sparql => &["literal-typing", "sparql"],
            run::Engine::Shacl => &["literal-typing", "shacl"],
            run::Engine::Both => &["literal-typing", "shacl", "sparql"],
        };
        assert_eq!(found[0].sources, sources);
        ours.retain(|r| !about(r));
    }
    assert_same(&format!("{set} ({})", engine_name(engine)), ours, theirs);
}

const SETS: &[(&str, &[&str])] = &[
    ("domain", &["ontology/domain.ttl"]),
    ("property_axioms", &["property_axioms/ontology.ttl"]),
    (
        "gist_patterns",
        &["gist_patterns/ontology.ttl", "gist_patterns/data.ttl"],
    ),
    (
        "stress",
        &[
            "checks_stress_test/stress-ontology.ttl",
            "checks_stress_test/stress-data.ttl",
        ],
    ),
];

#[test]
fn the_portable_queries_match_the_suite() {
    for (set, files) in SETS {
        parity(set, files, run::Engine::Sparql);
    }
}

#[test]
fn the_shapes_match_the_suite() {
    for (set, files) in SETS {
        parity(set, files, run::Engine::Shacl);
    }
}

#[test]
fn both_formulations_merged_match_the_suite() {
    for (set, files) in SETS {
        parity(set, files, run::Engine::Both);
    }
}
