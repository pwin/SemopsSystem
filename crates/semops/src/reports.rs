//! `semops check` reports for CI systems: JUnit XML, for test result views such as Azure
//! DevOps's Tests tab, and SARIF 2.1.0, for code scanning such as GitHub's.
//!
//! The quality checks find things about RDF nodes, not about lines of a file. JUnit has room
//! for that: each check is a test case, failed by the findings at or above the gate. SARIF
//! wants a place in a file, so each result points at the line where its focus node first
//! appears in the input, and names the node itself as the result's logical location.

use semops_checks::Registry;
use semops_checks::run::{Finding, Run};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

/// Text made safe for XML 1.0 content and attribute values. Characters XML 1.0 cannot
/// carry at all (most control characters) become U+FFFD.
fn xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
    out
}

fn finding_line(f: &Finding) -> String {
    let mut line = format!("{:<9} {}", f.severity, f.focus_node);
    if let Some(path) = &f.path {
        let _ = write!(line, "  path {path}");
    }
    if let Some(value) = &f.value {
        let _ = write!(line, "  value {value}");
    }
    let _ = write!(line, "\n          {}", f.message);
    line
}

/// JUnit XML: one test case per check in the registry, in the registry's order, and one for
/// any finding no check claims. `gate` is the rank at or below which a finding fails its
/// case (0 Violation, 1 Warning, 2 Info), or `None` when nothing fails.
#[must_use]
pub fn junit(report: &Run, registry: &Registry, gate: Option<u8>, rank: fn(&str) -> u8) -> String {
    let mut by_check: BTreeMap<Option<&str>, Vec<&Finding>> = BTreeMap::new();
    for f in &report.findings {
        by_check.entry(f.check_id.as_deref()).or_default().push(f);
    }
    let errors: BTreeMap<&str, &str> = report
        .queries
        .iter()
        .filter_map(|q| q.error.as_deref().map(|e| (q.id.as_str(), e)))
        .collect();

    let mut cases = String::new();
    let (mut tests, mut failures, mut error_count) = (0usize, 0usize, 0usize);
    let mut case = |classname: &str, name: &str, findings: &[&Finding], error: Option<&str>| {
        tests += 1;
        let (failing, passing): (Vec<&Finding>, Vec<&Finding>) = findings
            .iter()
            .partition(|f| gate.is_some_and(|g| rank(&f.severity) <= g));
        let _ = write!(
            cases,
            "    <testcase classname=\"{}\" name=\"{}\">",
            xml(classname),
            xml(name)
        );
        if let Some(error) = error {
            error_count += 1;
            let _ = write!(
                cases,
                "\n      <error type=\"NotRun\" message=\"the check's query did not run\">{}</error>",
                xml(error)
            );
        }
        if !failing.is_empty() {
            failures += 1;
            let worst = failing
                .iter()
                .map(|f| f.severity.as_str())
                .min_by_key(|s| rank(s))
                .unwrap_or("Violation");
            let body: Vec<String> = failing.iter().map(|f| finding_line(f)).collect();
            let _ = write!(
                cases,
                "\n      <failure type=\"{}\" message=\"{} finding{}\">{}</failure>",
                xml(worst),
                failing.len(),
                if failing.len() == 1 { "" } else { "s" },
                xml(&body.join("\n"))
            );
        }
        if !passing.is_empty() {
            let body: Vec<String> = passing.iter().map(|f| finding_line(f)).collect();
            let _ = write!(
                cases,
                "\n      <system-out>{}</system-out>",
                xml(&body.join("\n"))
            );
        }
        if error.is_some() || !findings.is_empty() {
            cases.push_str("\n    ");
        }
        cases.push_str("</testcase>\n");
    };

    for check in registry.checks() {
        let findings = by_check
            .remove(&Some(check.id.as_str()))
            .unwrap_or_default();
        case(
            &format!("semops.check.{}", check.category),
            &format!("{}: {}", check.id, check.title),
            &findings,
            errors.get(check.id.as_str()).copied(),
        );
    }
    // Findings whose check id is not in the registry, or that no check claims.
    let rest: Vec<&Finding> = by_check.into_values().flatten().collect();
    if !rest.is_empty() {
        case(
            "semops.check",
            "results no registered check claims",
            &rest,
            None,
        );
    }

    let counts = format!("tests=\"{tests}\" failures=\"{failures}\" errors=\"{error_count}\"");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <testsuites name=\"semops check\" {counts}>\n  \
         <testsuite name=\"semops check\" {counts} skipped=\"0\">\n{cases}  </testsuite>\n\
         </testsuites>\n"
    )
}

fn sarif_level(severity: &str) -> &'static str {
    match severity {
        "Violation" => "error",
        "Warning" => "warning",
        _ => "note",
    }
}

/// Where a node is first written in the input files: the file, as given on the command line
/// with `/` separators, and the line, 1-based. A full IRI in angle brackets is looked for
/// first, then a prefixed name ending in the IRI's local name.
struct Sources {
    files: Vec<(String, Vec<String>)>,
}

impl Sources {
    fn read(paths: &[PathBuf]) -> Self {
        let files = paths
            .iter()
            .map(|p| {
                let text = std::fs::read_to_string(p).unwrap_or_default();
                (
                    p.to_string_lossy().replace('\\', "/"),
                    text.lines().map(str::to_owned).collect(),
                )
            })
            .collect();
        Self { files }
    }

    fn locate(&self, focus: &str, blank: bool) -> (Option<&str>, Option<usize>) {
        let first = self.files.first().map(|(uri, _)| uri.as_str());
        if blank || focus.is_empty() {
            return (first, None);
        }
        let bracketed = format!("<{focus}>");
        let local = focus
            .rsplit(['#', '/'])
            .next()
            .filter(|l| !l.is_empty())
            .map(|l| format!(":{l}"));
        let is_name = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-' | '.');
        let prefixed = |line: &str, needle: &str| {
            line.match_indices(needle).any(|(i, _)| {
                let after = line[i + needle.len()..].chars().next();
                !after.is_some_and(|c| is_name(c) && c != '.')
            })
        };
        for (uri, lines) in &self.files {
            if let Some(n) = lines.iter().position(|l| l.contains(&bracketed)) {
                return (Some(uri), Some(n + 1));
            }
        }
        if let Some(local) = &local {
            for (uri, lines) in &self.files {
                if let Some(n) = lines.iter().position(|l| prefixed(l, local)) {
                    return (Some(uri), Some(n + 1));
                }
            }
        }
        (first, None)
    }
}

/// A stable identifier for a finding across runs, so that code scanning can tell a finding
/// it has seen from a new one (FNV-1a over the check, node, path and value).
fn fingerprint(f: &Finding) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in [
        f.check_id.as_deref().unwrap_or(""),
        if f.focus_is_blank {
            "_:"
        } else {
            &f.focus_node
        },
        f.path.as_deref().unwrap_or(""),
        f.value.as_deref().unwrap_or(""),
    ] {
        for byte in part.bytes().chain([0]) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

/// SARIF 2.1.0: the registry's checks as the tool's rules, each finding as a result.
#[must_use]
pub fn sarif(report: &Run, registry: &Registry, files: &[PathBuf]) -> Value {
    let rules: Vec<Value> = registry
        .checks()
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "name": c.title,
                "shortDescription": { "text": c.title },
                "fullDescription": { "text": c.description },
                "help": { "text": c.remediation },
                "defaultConfiguration": { "level": sarif_level(&c.default_severity) },
                "properties": { "category": c.category, "metric": c.metric, "tags": [c.category] },
            })
        })
        .collect();
    let index: BTreeMap<&str, usize> = registry
        .checks()
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();
    let sources = Sources::read(files);
    let results: Vec<Value> = report
        .findings
        .iter()
        .map(|f| {
            let (uri, line) = sources.locate(&f.focus_node, f.focus_is_blank);
            let mut physical = json!({ "artifactLocation": { "uri": uri.unwrap_or("") } });
            if let Some(line) = line {
                physical["region"] = json!({ "startLine": line });
            }
            let mut text = f.message.clone();
            if let Some(path) = &f.path {
                let _ = write!(text, " (path {path})");
            }
            let mut result = json!({
                "ruleId": f.check_id.as_deref().unwrap_or("semops"),
                "level": sarif_level(&f.severity),
                "message": { "text": text },
                "locations": [{
                    "physicalLocation": physical,
                    "logicalLocations": [{
                        "fullyQualifiedName": f.focus_node,
                        // SARIF's recommended kinds: an IRI names a resource, a blank node is an object.
                        "kind": if f.focus_is_blank { "object" } else { "resource" },
                    }],
                }],
                "partialFingerprints": { "semopsFinding/v1": fingerprint(f) },
                "properties": { "severity": f.severity, "sources": f.sources },
            });
            if let Some(i) = f.check_id.as_deref().and_then(|id| index.get(id)) {
                result["ruleIndex"] = json!(i);
            }
            result
        })
        .collect();
    let notifications: Vec<Value> = report
        .queries
        .iter()
        .filter_map(|q| {
            q.error.as_ref().map(|e| {
                json!({
                    "level": "error",
                    "message": { "text": format!("check {} did not run: {e}", q.id) },
                    "descriptor": { "id": q.id },
                })
            })
        })
        .collect();
    json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": { "driver": {
                "name": "semops",
                "version": env!("CARGO_PKG_VERSION"),
                "informationUri": "https://github.com/pwin/SemopsSystem",
                "rules": rules,
            }},
            "invocations": [{
                "executionSuccessful": report.complete(),
                "toolExecutionNotifications": notifications,
            }],
            "results": results,
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_escapes_markup_and_drops_what_xml_cannot_hold() {
        assert_eq!(
            xml("a<b & \"c\" 'd'>"),
            "a&lt;b &amp; &quot;c&quot; &apos;d&apos;&gt;"
        );
        assert_eq!(xml("bell\u{7}"), "bell\u{FFFD}");
        assert_eq!(xml("tab\tnewline\n"), "tab\tnewline\n");
    }

    #[test]
    fn a_node_is_found_by_its_iri_or_its_local_name() {
        let sources = Sources {
            files: vec![(
                "o.ttl".into(),
                vec![
                    "@prefix ex: <https://example.org/o#> .".into(),
                    "ex:Shopkeeper a owl:Class .".into(),
                    "ex:Shop a owl:Class .".into(),
                    "<https://example.org/other> a owl:Class .".into(),
                ],
            )],
        };
        // ex:Shop is not matched inside ex:Shopkeeper.
        assert_eq!(
            sources.locate("https://example.org/o#Shop", false),
            (Some("o.ttl"), Some(3))
        );
        assert_eq!(
            sources.locate("https://example.org/other", false),
            (Some("o.ttl"), Some(4))
        );
        assert_eq!(sources.locate("b0", true), (Some("o.ttl"), None));
        assert_eq!(
            sources.locate("https://example.org/absent", false),
            (Some("o.ttl"), None)
        );
    }
}
