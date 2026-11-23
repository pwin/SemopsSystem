//! `semops check` as a CI gate sees it: exit status, and the three report formats.

use std::path::PathBuf;
use std::process::Command;

fn fixture(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../semops-checks/tests/fixtures")
        .join(path)
}

fn semops(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_semops"))
        .args(args)
        .output()
        .expect("semops runs");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8(output.stdout).expect("utf8"),
        String::from_utf8(output.stderr).expect("utf8"),
    )
}

#[test]
fn findings_at_the_threshold_fail_the_run() {
    let domain = fixture("ontology/domain.ttl");
    let domain = domain.to_str().expect("path");
    let (code, stdout, _) = semops(&["check", domain]);
    assert_eq!(code, 1, "domain.ttl has violations");
    assert!(stdout.contains("64 findings:"), "{stdout}");
    let (code, _, _) = semops(&["check", domain, "--fail-on", "never"]);
    assert_eq!(code, 0);
}

#[test]
fn a_clean_graph_passes() {
    let dir = std::env::temp_dir().join(format!("semops-check-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let clean = dir.join("clean.ttl");
    std::fs::write(
        &clean,
        "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
         <https://example.org/o> a owl:Ontology ; owl:versionInfo \"1\" ;\n\
           owl:versionIRI <https://example.org/o/1> ; rdfs:label \"O\"@en ;\n\
           rdfs:comment \"An ontology.\"@en .\n",
    )
    .expect("write");
    let (code, stdout, stderr) = semops(&[
        "check",
        clean.to_str().expect("path"),
        "--fail-on",
        "Violation",
    ]);
    assert_eq!(code, 0, "{stdout}{stderr}");
}

#[test]
fn json_and_csv_carry_every_finding() {
    let data = fixture("gist_patterns/data.ttl");
    let ontology = fixture("gist_patterns/ontology.ttl");
    let files = [
        ontology.to_str().expect("path"),
        data.to_str().expect("path"),
    ];
    let (_, json, _) = semops(&["check", files[0], files[1], "--format", "json"]);
    let findings: Vec<serde_json::Value> = serde_json::from_str(&json).expect("a JSON array");
    assert_eq!(findings.len(), 15);
    assert!(findings.iter().all(|f| f["check_id"].is_string()));
    let (_, csv, _) = semops(&["check", files[0], files[1], "--format", "csv"]);
    let mut lines = csv.lines();
    assert!(
        lines
            .next()
            .expect("header")
            .starts_with("check_id,category,title,severity,")
    );
    assert_eq!(csv.lines().count(), 16);
}

#[test]
fn each_engine_finds_what_the_suite_finds_with_it() {
    let domain = fixture("ontology/domain.ttl");
    let domain = domain.to_str().expect("path");
    for engine in ["both", "sparql", "shacl"] {
        let (code, stdout, stderr) = semops(&["check", domain, "--engine", engine]);
        assert_eq!(code, 1, "{engine}: {stderr}");
        let expected = if engine == "shacl" {
            "54 findings:"
        } else {
            "64 findings:"
        };
        assert!(stdout.contains(expected), "{engine}: {stdout}");
    }
}

#[test]
fn a_file_that_cannot_be_read_is_exit_2() {
    let (code, _, stderr) = semops(&["check", "no-such-file.ttl"]);
    assert_eq!(code, 2);
    assert!(stderr.contains("no-such-file.ttl"), "{stderr}");
}

#[test]
fn the_registry_can_be_listed() {
    let (code, stdout, _) = semops(&["check", "--list"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("STR-001"));
    assert!(stdout.contains("61 checks"));
}

#[test]
fn junit_has_a_case_per_check_failed_by_findings_at_the_gate() {
    let domain = fixture("ontology/domain.ttl");
    let domain = domain.to_str().expect("path");
    let (code, xml, _) = semops(&["check", domain, "--format", "junit"]);
    assert_eq!(
        code, 1,
        "the exit status follows --fail-on, whatever the format"
    );
    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert_eq!(xml.matches("<testcase ").count(), 61, "one case per check");
    assert!(xml.contains("<testsuites name=\"semops check\" tests=\"61\""));
    // Every <failure> is a Violation at the default gate; warnings go to system-out.
    let failures = xml.matches("<failure ").count();
    assert!(failures > 0);
    assert!(xml.matches("<failure type=\"Violation\"").count() == failures);
    assert!(xml.contains("<system-out>"));
    assert!(xml.contains(&format!("failures=\"{failures}\"")));
    // With no gate, nothing fails.
    let (code, xml, _) = semops(&["check", domain, "--format", "junit", "--fail-on", "never"]);
    assert_eq!(code, 0);
    assert_eq!(xml.matches("<failure ").count(), 0);
}

#[test]
fn sarif_carries_every_finding_with_a_rule_and_a_place() {
    let data = fixture("gist_patterns/data.ttl");
    let ontology = fixture("gist_patterns/ontology.ttl");
    let files = [
        ontology.to_str().expect("path"),
        data.to_str().expect("path"),
    ];
    let (_, out, _) = semops(&["check", files[0], files[1], "--format", "sarif"]);
    let sarif: serde_json::Value = serde_json::from_str(&out).expect("SARIF is JSON");
    assert_eq!(sarif["version"], "2.1.0");
    let run = &sarif["runs"][0];
    assert_eq!(run["tool"]["driver"]["name"], "semops");
    let rules = run["tool"]["driver"]["rules"].as_array().expect("rules");
    assert_eq!(rules.len(), 61);
    assert_eq!(run["invocations"][0]["executionSuccessful"], true);
    let results = run["results"].as_array().expect("results");
    assert_eq!(results.len(), 15, "the same findings --format json gives");
    for r in results {
        assert!(["error", "warning", "note"].contains(&r["level"].as_str().expect("level")));
        let rule = r["ruleIndex"].as_u64().expect("a registered rule") as usize;
        assert_eq!(rules[rule]["id"], r["ruleId"]);
        let location = &r["locations"][0];
        let uri = location["physicalLocation"]["artifactLocation"]["uri"]
            .as_str()
            .expect("uri");
        assert!(uri.ends_with(".ttl") && !uri.contains('\\'), "{uri}");
        assert!(location["logicalLocations"][0]["fullyQualifiedName"].is_string());
        assert!(r["partialFingerprints"]["semopsFinding/v1"].is_string());
    }
    // Named nodes are placed on the line that first writes them.
    assert!(
        results
            .iter()
            .any(|r| r["locations"][0]["physicalLocation"]["region"]["startLine"].is_u64())
    );
}

fn diff_fixture(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../semops-checks/tests/fixtures/diff")
        .join(name)
        .to_str()
        .expect("path")
        .to_owned()
}

#[test]
fn diff_suggests_a_bump_and_gates_on_it() {
    let (v1, major) = (diff_fixture("v1.ttl"), diff_fixture("v2-major.ttl"));
    let (code, text, _) = semops(&["diff", &v1, &major]);
    assert_eq!(code, 0, "--fail-on defaults to never, as in the suite");
    assert!(text.contains("Removed classes [MAJOR]:"), "{text}");
    assert!(
        text.trim_end().ends_with("Suggested version bump: MAJOR"),
        "{text}"
    );
    let (code, _, _) = semops(&["diff", &v1, &major, "--fail-on", "major"]);
    assert_eq!(code, 1);
    let patch = diff_fixture("v2-patch.ttl");
    let (code, json, _) = semops(&[
        "diff",
        &v1,
        &patch,
        "--format",
        "json",
        "--fail-on",
        "minor",
    ]);
    assert_eq!(code, 0, "a patch is below minor");
    let value: serde_json::Value = serde_json::from_str(&json).expect("JSON");
    assert_eq!(value["bump"], "patch");
    let (code, _, stderr) = semops(&["diff", &v1, "no-such-file.ttl"]);
    assert_eq!(code, 2, "{stderr}");
}

#[test]
fn repair_appends_what_it_adds_and_rewrites_only_when_it_must() {
    let dir = std::env::temp_dir().join(format!("semops-repair-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../semops-checks/tests/fixtures/repair/shop.ttl");
    let file = dir.join("shop.ttl");
    std::fs::copy(&source, &file).expect("copy");
    let file = file.to_str().expect("path");

    // Only the labels: insert repairs, appended after the file's own text, untouched.
    let (code, out, err) = semops(&["repair", file, "--check", "QUA-001"]);
    assert_eq!(code, 0, "{err}");
    let original = std::fs::read_to_string(&source).expect("read");
    assert!(
        out.replace("\r\n", "\n")
            .starts_with(&original.replace("\r\n", "\n"))
    );
    assert!(out.contains("rdfs:label \"Bookshop\"@en"), "{out}");
    assert!(err.contains("2 repairs: 2 insert, 0 replace"), "{err}");

    // Every repair, written back: STY-003 replaces a label, so the file is rewritten, and
    // the checks it fixed no longer fire.
    let (code, _, err) = semops(&["repair", file, "--write"]);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("written out whole"), "{err}");
    let (_, json, _) = semops(&["check", file, "--format", "json"]);
    let findings: Vec<serde_json::Value> = serde_json::from_str(&json).expect("JSON");
    for fixed in ["QUA-001", "QUA-002", "QUA-007", "STY-003", "LOG-003"] {
        assert!(
            !findings.iter().any(|f| f["check_id"] == fixed),
            "{fixed} still fires after its repair"
        );
    }
}
