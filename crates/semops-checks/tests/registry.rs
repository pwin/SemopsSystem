//! The embedded registry is whole, consistent with itself, and the same as the suite's.

use oxrdf::{Graph, NamedOrBlankNodeRef, TermRef, Triple};
use oxrdfio::{RdfFormat, RdfParser};
use semops_checks::{Registry, file, files, queries, shapes};
use std::path::PathBuf;

#[test]
fn the_registry_parses_with_its_61_checks() {
    let registry = Registry::builtin();
    assert_eq!(registry.checks().len(), 61);
    assert_eq!(registry.namespace(), semops_checks::NAMESPACE);
    let str001 = registry.get("STR-001").expect("STR-001");
    assert_eq!(str001.category, "structural");
    assert_eq!(str001.default_severity, "Violation");
    assert_eq!(
        registry.categories()[..2],
        ["structural", "logical"],
        "categories in first-seen order"
    );
    assert!(Registry::from_json(r#"{"namespace":"x","checks":[]}"#).is_ok());
}

#[test]
fn a_registry_that_names_a_check_twice_is_refused() {
    let check = r#"{"id":"X-1","category":"c","metric":"m","default_severity":"Info","title":"t",
        "description":"d","remediation":"r","cucumber_feature":"f","cucumber_scenario":"s"}"#;
    let text = format!(r#"{{"namespace":"x","checks":[{check},{check}]}}"#);
    let error = Registry::from_json(&text).expect_err("duplicate id");
    assert!(error.to_string().contains("X-1"), "{error}");
}

#[test]
fn every_query_reports_a_check_the_registry_has() {
    let registry = Registry::builtin();
    let all = queries();
    assert_eq!(all.len(), 44);
    for query in &all {
        assert!(
            registry.get(query.id).is_some(),
            "{} names no check",
            query.path
        );
        assert!(
            query.text.contains(&format!("oq:{}", query.id)),
            "{} does not tag its results with its own id",
            query.path
        );
    }
    let specific: Vec<_> = all
        .iter()
        .filter(|q| q.subject_specific)
        .map(|q| q.id)
        .collect();
    assert_eq!(specific, ["TQL-004", "TQL-005"]);
}

fn shapes_graph() -> Graph {
    let mut graph = Graph::new();
    for shape_file in shapes() {
        for quad in RdfParser::from_format(RdfFormat::Turtle).for_slice(shape_file.text.as_bytes())
        {
            let quad = quad.unwrap_or_else(|e| panic!("{}: {e}", shape_file.path));
            graph.insert(&Triple::from(quad));
        }
    }
    graph
}

#[test]
fn every_shape_resolves_to_a_registered_check() {
    let registry = Registry::builtin();
    let graph = shapes_graph();
    assert_eq!(shapes().count(), 6);
    let node_shape = oxrdf::NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#NodeShape");
    let mut resolved = std::collections::BTreeSet::new();
    for shape in graph.subjects_for_predicate_object(oxrdf::vocab::rdf::TYPE, node_shape) {
        let id = registry
            .resolve_in_shapes(None, Some(shape), &graph)
            .unwrap_or_else(|| panic!("{shape} resolves to no check"));
        resolved.insert(id.to_owned());
    }
    assert!(resolved.len() >= 20, "{resolved:?}");
    // A nested property shape resolves through the node shape that holds it.
    let property = oxrdf::NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#property");
    let nested = graph
        .triples_for_predicate(property)
        .find_map(|t| match t.object {
            TermRef::BlankNode(b) => Some(NamedOrBlankNodeRef::from(b)),
            _ => None,
        })
        .expect("a nested property shape");
    assert!(
        registry
            .resolve_in_shapes(None, Some(nested), &graph)
            .is_some()
    );
}

#[test]
fn check_ids_resolve_from_components_and_shapes() {
    let registry = Registry::builtin();
    let oq = |id: &str| format!("{}{id}", semops_checks::NAMESPACE);
    assert_eq!(
        registry.resolve_check_id(Some(&oq("STR-001")), None),
        Some("STR-001")
    );
    assert_eq!(
        registry.resolve_check_id(
            Some("http://www.w3.org/ns/shacl#MinCountConstraintComponent"),
            Some(&oq("QUA-001"))
        ),
        Some("QUA-001")
    );
    assert_eq!(registry.resolve_check_id(Some(&oq("NOPE-1")), None), None);
}

#[test]
fn the_repair_manifest_names_templates_that_exist() {
    let manifest: serde_json::Value =
        serde_json::from_str(file("repairs/manifest.json").expect("manifest")).expect("JSON");
    let checks = manifest["checks"].as_object().expect("checks");
    let mut templates = 0;
    for (id, entry) in checks {
        let named: Vec<&str> = entry["template"]
            .as_str()
            .into_iter()
            .chain(
                entry["templatesByPolicy"]
                    .as_object()
                    .into_iter()
                    .flat_map(|m| m.values().filter_map(|v| v.as_str())),
            )
            .collect();
        assert!(!named.is_empty(), "{id} names no template");
        for template in named {
            assert!(
                file(&format!("repairs/{template}")).is_some(),
                "{id}: {template} missing"
            );
            templates += 1;
        }
    }
    assert_eq!(templates, 18);
}

/// The suite's checkout, if this machine has one: `ONTOLOGY_SUITE`, or the layout the
/// workspace is developed in, with the upstream clones beside it.
fn suite_resources() -> Option<PathBuf> {
    let candidates = std::env::var_os("ONTOLOGY_SUITE")
        .map(PathBuf::from)
        .into_iter()
        .chain(std::iter::once(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../upstream/consolidated-ontology-quality-suite-python"),
        ));
    candidates
        .map(|root| root.join("ontology_suite/resources"))
        .find(|p| p.join("registry.json").is_file())
}

/// The copy here and the suite's are the same, character for character but for line
/// endings. Skipped where there is no checkout, which is CI: this is a test for whoever
/// edits both, which is who breaks it.
#[test]
fn the_embedded_copy_matches_the_suite_checkout() {
    let Some(root) = suite_resources() else {
        eprintln!("no ontology suite checkout; set ONTOLOGY_SUITE to compare");
        return;
    };
    let normalise = |s: &str| s.replace("\r\n", "\n");
    let mut theirs = Vec::new();
    for sub in ["registry.json", "shapes", "sparql", "repairs"] {
        let path = root.join(sub);
        if path.is_file() {
            theirs.push(sub.to_owned());
            continue;
        }
        let mut stack = vec![path];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("readable") {
                let entry = entry.expect("entry").path();
                if entry.is_dir() {
                    stack.push(entry);
                } else {
                    let relative = entry.strip_prefix(&root).expect("under root");
                    theirs.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    theirs.sort();
    let ours: Vec<&str> = files().iter().map(|f| f.path).collect();
    assert_eq!(ours, theirs, "the two copies hold different files");
    for path in theirs {
        let text = std::fs::read_to_string(root.join(&path)).expect("readable");
        assert_eq!(
            normalise(file(&path).expect("embedded")),
            normalise(&text),
            "{path} differs from the suite's"
        );
    }
}
