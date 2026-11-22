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
