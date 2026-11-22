//! From `sh:ValidationResult`s to findings, and from several arms' findings to one list:
//! the suite's `checks/merge.py`, rule for rule, since the point is that the two report the
//! same rows.

use oxrdf::{Graph, NamedNodeRef, NamedOrBlankNodeRef, TermRef};
use std::collections::HashMap;

/// One finding: what the suite calls a result row.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Finding {
    /// `None` when no check in the registry claims the result.
    pub check_id: Option<String>,
    pub category: Option<String>,
    pub title: Option<String>,
    /// `Violation`, `Warning` or `Info`, or the severity IRI if it is none of those.
    pub severity: String,
    /// The focus node's IRI, or a blank node's label (see `focus_is_blank`).
    pub focus_node: String,
    /// A property path in SPARQL syntax; several paths are sorted and joined with `, `.
    pub path: Option<String>,
    /// The values, sorted and joined with `, `; the focus node when the result has none.
    pub value: Option<String>,
    pub message: String,
    pub remediation: Option<String>,
    /// Which arms reported it: `sparql`, `literal-typing`.
    pub sources: Vec<String>,
    /// Whether the focus node is a blank node, whose label names nothing a reader can
    /// look up and differs between runs.
    pub focus_is_blank: bool,
}

/// What a message calls a focus node with no name, as the portable queries do.
pub const ANONYMOUS_FOCUS: &str = "[a blank node]";

const SH: &str = "http://www.w3.org/ns/shacl#";

fn sh(local: &'static str) -> NamedNodeRef<'static> {
    match local {
        "resultSeverity" => {
            NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#resultSeverity")
        }
        "focusNode" => NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#focusNode"),
        "resultPath" => NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#resultPath"),
        "value" => NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#value"),
        "resultMessage" => NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#resultMessage"),
        "sourceConstraintComponent" => {
            NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#sourceConstraintComponent")
        }
        "sourceShape" => NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#sourceShape"),
        "inversePath" => NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#inversePath"),
        "alternativePath" => {
            NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#alternativePath")
        }
        "zeroOrMorePath" => {
            NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#zeroOrMorePath")
        }
        "oneOrMorePath" => NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#oneOrMorePath"),
        "zeroOrOnePath" => NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#zeroOrOnePath"),
        _ => unreachable!("not a SHACL term this module uses: {local}"),
    }
}

/// A term as Python's `str()` of an rdflib term gives it: an IRI without brackets, a
/// literal's lexical form, a blank node's bare label.
fn text(term: TermRef<'_>) -> String {
    match term {
        TermRef::NamedNode(n) => n.as_str().to_owned(),
        TermRef::BlankNode(b) => b.as_str().to_owned(),
        TermRef::Literal(l) => l.value().to_owned(),
        #[allow(unreachable_patterns)]
        other => other.to_string(),
    }
}

fn as_subject(term: TermRef<'_>) -> Option<NamedOrBlankNodeRef<'_>> {
    match term {
        TermRef::NamedNode(n) => Some(n.into()),
        TermRef::BlankNode(b) => Some(b.into()),
        _ => None,
    }
}

/// The members of the RDF list starting at `head`.
fn collection<'a>(graph: &'a Graph, head: TermRef<'a>) -> Vec<TermRef<'a>> {
    let mut out = Vec::new();
    let mut node = head;
    while let Some(cell) = as_subject(node) {
        if out.len() > 10_000 {
            break;
        }
        let Some(first) = graph.object_for_subject_predicate(cell, oxrdf::vocab::rdf::FIRST) else {
            break;
        };
        out.push(first);
        match graph.object_for_subject_predicate(cell, oxrdf::vocab::rdf::REST) {
            Some(rest) => node = rest,
            None => break,
        }
    }
    out
}

/// A `sh:resultPath` value as a SPARQL property path: a blank-node path structure rendered
/// rather than named by its label, which changes between runs.
fn path_expression(graph: &Graph, node: TermRef<'_>, depth: usize) -> String {
    let Some(subject) = as_subject(node).filter(|s| s.is_blank_node() && depth <= 10) else {
        return text(node);
    };
    for (operator, suffix) in [
        ("zeroOrMorePath", "*"),
        ("oneOrMorePath", "+"),
        ("zeroOrOnePath", "?"),
    ] {
        if let Some(inner) = graph.object_for_subject_predicate(subject, sh(operator)) {
            return format!("({}){suffix}", path_expression(graph, inner, depth + 1));
        }
    }
    if let Some(inverse) = graph.object_for_subject_predicate(subject, sh("inversePath")) {
        return format!("^({})", path_expression(graph, inverse, depth + 1));
    }
    if let Some(alternatives) = graph.object_for_subject_predicate(subject, sh("alternativePath")) {
        return collection(graph, alternatives)
            .into_iter()
            .map(|m| path_expression(graph, m, depth + 1))
            .collect::<Vec<_>>()
            .join("|");
    }
    if graph
        .object_for_subject_predicate(subject, oxrdf::vocab::rdf::FIRST)
        .is_some()
    {
        return collection(graph, node)
            .into_iter()
            .map(|m| path_expression(graph, m, depth + 1))
            .collect::<Vec<_>>()
            .join("/");
    }
    text(node)
}

/// Several values as one string, sorted and deduplicated so one finding always renders
/// the same.
fn joined(mut values: Vec<String>) -> Option<String> {
    if values.is_empty() {
        return None;
    }
    values.sort();
    values.dedup();
    Some(values.join(", "))
}

/// A message with a blank node label taken back out, in either spelling.
fn without_blank_label(message: &str, label: &str) -> String {
    let bare = label.strip_prefix("_:").unwrap_or(label);
    if bare.is_empty() {
        return message.to_owned();
    }
    let mut message = message.replace(&format!("_:{bare}"), ANONYMOUS_FOCUS);
    if bare.len() >= 8 {
        message = message.replace(bare, ANONYMOUS_FOCUS);
    }
    message
}

/// Fills in a message's `{$this}`, `{$value}` and `{$path}`, and keeps a blank focus node's
/// label out of it.
fn substitute(
    message: &str,
    focus: Option<&str>,
    path: Option<&str>,
    value: Option<&str>,
    focus_is_blank: bool,
) -> String {
    if message.is_empty() {
        return String::new();
    }
    let mut message = message.to_owned();
    let mut focus = focus.map(str::to_owned);
    let mut value = value.map(str::to_owned);
    if focus_is_blank && let Some(label) = focus.as_deref().filter(|f| !f.is_empty()) {
        message = without_blank_label(&message, label);
        if value.as_deref() == Some(label) {
            value = Some(ANONYMOUS_FOCUS.to_owned());
        }
        focus = Some(ANONYMOUS_FOCUS.to_owned());
    }
    if !message.contains('{') {
        return message;
    }
    for (name, replacement) in [
        ("this", focus),
        ("value", value),
        ("path", path.map(str::to_owned)),
    ] {
        if let Some(replacement) = replacement {
            message = message
                .replace(&format!("{{${name}}}"), &replacement)
                .replace(&format!("{{?{name}}}"), &replacement);
        }
    }
    message
}

fn severity_label(iri: &str) -> String {
    match iri.strip_prefix(SH) {
        Some(local @ ("Violation" | "Warning" | "Info")) => local.to_owned(),
        _ => iri.to_owned(),
    }
}

/// Every `sh:ValidationResult` in a results graph, as findings from one arm.
pub fn extract(results: &Graph, registry: &crate::Registry, source: &str) -> Vec<Finding> {
    let mut subjects: Vec<NamedOrBlankNodeRef<'_>> = Vec::new();
    for triple in results.triples_for_predicate(sh("resultSeverity")) {
        if !subjects.contains(&triple.subject) {
            subjects.push(triple.subject);
        }
    }
    subjects
        .into_iter()
        .map(|result| {
            let one = |p: &'static str| results.object_for_subject_predicate(result, sh(p));
            let all = |p: &'static str| results.objects_for_subject_predicate(result, sh(p));
            let severity = one("resultSeverity").map(text).unwrap_or_default();
            let focus = one("focusNode");
            let focus_is_blank = matches!(focus, Some(TermRef::BlankNode(_)));
            let focus_text = focus.map(text);
            let path = joined(
                all("resultPath")
                    .map(|p| path_expression(results, p, 0))
                    .collect(),
            );
            let mut value = joined(all("value").map(text).collect());
            if value.is_none() {
                value.clone_from(&focus_text);
            }
            let message = one("resultMessage").map(text).unwrap_or_default();
            let component = one("sourceConstraintComponent").map(text);
            let shape = one("sourceShape").map(text);
            let check_id = registry
                .resolve_check_id(component.as_deref(), shape.as_deref())
                .map(str::to_owned);
            let check = check_id.as_deref().and_then(|id| registry.get(id));
            Finding {
                category: check.map(|c| c.category.clone()),
                title: check.map(|c| c.title.clone()),
                remediation: check.map(|c| c.remediation.clone()),
                check_id,
                severity: severity_label(&severity),
                message: substitute(
                    &message,
                    focus_text.as_deref(),
                    path.as_deref(),
                    value.as_deref(),
                    focus_is_blank,
                ),
                focus_node: focus_text.unwrap_or_default(),
                path,
                value,
                sources: vec![source.to_owned()],
                focus_is_blank,
            }
        })
        .collect()
}

/// An arm, a check id, a path and a value: where anonymous findings are counted.
type Bucket = (String, Option<String>, Option<String>, Option<String>);

#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Named(Option<String>, String, Option<String>, Option<String>),
    /// A blank focus node's label is not comparable between arms, so the nth anonymous
    /// finding an arm reports for a (check, path, value) pairs with the nth another reports.
    Anonymous(Option<String>, Option<String>, Option<String>, usize),
}

/// Merges several arms' findings: one finding reached by two arms is one finding listing
/// both, keeping the first arm's message; then orders them by severity, category, check id
/// and focus node.
pub fn unify(arms: Vec<Vec<Finding>>) -> Vec<Finding> {
    let mut merged: Vec<Finding> = Vec::new();
    let mut index: HashMap<Key, usize> = HashMap::new();
    let mut anonymous: HashMap<Bucket, usize> = HashMap::new();
    for finding in arms.into_iter().flatten() {
        let key = if finding.focus_is_blank {
            let arm = finding.sources.first().cloned().unwrap_or_default();
            let bucket = (
                arm,
                finding.check_id.clone(),
                finding.path.clone(),
                finding.value.clone(),
            );
            let ordinal = anonymous.entry(bucket).or_insert(0);
            let key = Key::Anonymous(
                finding.check_id.clone(),
                finding.path.clone(),
                finding.value.clone(),
                *ordinal,
            );
            *ordinal += 1;
            key
        } else {
            Key::Named(
                finding.check_id.clone(),
                finding.focus_node.clone(),
                finding.path.clone(),
                finding.value.clone(),
            )
        };
        match index.get(&key) {
            Some(&i) => {
                let sources = &mut merged[i].sources;
                sources.extend(finding.sources);
                sources.sort();
                sources.dedup();
            }
            None => {
                index.insert(key, merged.len());
                merged.push(finding);
            }
        }
    }
    let rank = |s: &str| match s {
        "Violation" => 0,
        "Warning" => 1,
        "Info" => 2,
        _ => 9,
    };
    merged.sort_by(|a, b| {
        (
            rank(&a.severity),
            a.category.as_deref().unwrap_or("zzz"),
            a.check_id.as_deref().unwrap_or("zzz"),
            &a.focus_node,
        )
            .cmp(&(
                rank(&b.severity),
                b.category.as_deref().unwrap_or("zzz"),
                b.check_id.as_deref().unwrap_or("zzz"),
                &b.focus_node,
            ))
    });
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_filled_and_blank_labels_kept_out() {
        assert_eq!(
            substitute(
                "{$this} is odd at {$path}",
                Some("http://x/a"),
                Some("http://x/p"),
                None,
                false
            ),
            "http://x/a is odd at http://x/p"
        );
        assert_eq!(
            substitute(
                "A label on _:b0 has no tag; {$value}",
                Some("b0"),
                None,
                Some("b0"),
                true
            ),
            "A label on [a blank node] has no tag; [a blank node]"
        );
        assert_eq!(
            substitute("{$maxCount} left alone", Some("x"), None, None, false),
            "{$maxCount} left alone"
        );
    }
}
