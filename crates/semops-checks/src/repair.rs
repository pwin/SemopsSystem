//! Repairs: each check's quick fix, a SPARQL Update template from `resources/repairs/`,
//! bound to one finding and run against the document's own triples.
//!
//! A port of the Ontology Quality Suite's `checks/repair.py` (0.28.0, d9300e6), which ports
//! the VS Code extension's `repairEngine.ts`: the same manifest and templates, the same
//! `VALUES` row spliced into the template's `WHERE` clause, and the same split between
//! `insert` repairs, which only add triples, and `replace` repairs, which also remove them.
//! The update runs on HOLOS's engine where the suite uses rdflib's.
//!
//! The variables every template may use, bound by the one `VALUES` row (`UNDEF` where they
//! do not apply): `?focusNode`, `?path` and `?value` from the finding; `?derivedLabel`, the
//! focus node's local name made readable; and `?defaultLanguageTag`, `?categoryClass`,
//! `?defaultOntologyBaseIri` and `?defaultVersionInfo` from the project's [`Standards`].

use crate::run::{Error, Finding};
use holos_engine::Engine as Holos;
use holos_engine::QueryOptions;
use holos_security::Session;
use oxrdf::{GraphName, Quad, Triple};
use serde::Deserialize;
use spareval::QueryResults;
use std::collections::{BTreeMap, BTreeSet};

/// gist's ontology namespace, which the default category class is written in.
const GIST: &str = "https://w3id.org/semanticarts/ns/ontology/gist/";

/// The project's choices a repair draws on, with the suite's defaults.
#[derive(Clone, Debug)]
pub struct Standards {
    /// The language tag of a label a repair adds, or adds to an untagged one.
    pub default_language_tag: String,
    /// The class MDL-003's repair re-types a class as, as a CURIE or an IRI.
    pub category_class: String,
    /// Prefixes for `category_class`, beside the document's own.
    pub prefixes: BTreeMap<String, String>,
    /// MDL-002: what replaces a named-to-named `owl:equivalentClass` (`subClassOf` or
    /// `closeMatch`).
    pub equivalent_class_policy: String,
    /// LOG-003: which axiom to keep when `owl:equivalentClass` and `rdfs:subClassOf` are
    /// both asserted (`keepEquivalentClass` or `keepSubClassOf`).
    pub redundant_equivalence_policy: String,
    /// The base IRI of the `owl:Ontology` QUA-005's repair declares.
    pub default_ontology_base_iri: String,
    /// The `owl:versionInfo` QUA-002's repair adds.
    pub default_version_info: String,
}

impl Default for Standards {
    fn default() -> Self {
        Self {
            default_language_tag: "en".to_owned(),
            category_class: "gist:Category".to_owned(),
            prefixes: BTreeMap::from([("gist".to_owned(), GIST.to_owned())]),
            equivalent_class_policy: "subClassOf".to_owned(),
            redundant_equivalence_policy: "keepEquivalentClass".to_owned(),
            default_ontology_base_iri: "https://example.org/ontology/".to_owned(),
            default_version_info: "0.1.0".to_owned(),
        }
    }
}

/// A document's triples and the prefixes it declares, which a repair uses to resolve the
/// standards' CURIEs and to write what it adds.
///
/// # Errors
///
/// If the file cannot be read or does not parse.
pub fn load_document(
    path: &std::path::Path,
) -> Result<(Vec<Triple>, BTreeMap<String, String>), Error> {
    let (format, reader) = holos_engine::source::open(path)
        .map_err(|e| Error::new(format!("{}: {e}", path.display())))?;
    let mut parser = oxrdfio::RdfParser::from_format(format).for_reader(reader);
    let mut triples = Vec::new();
    for quad in parser.by_ref() {
        let quad = quad.map_err(|e| Error::new(format!("{}: {e}", path.display())))?;
        triples.push(Triple::from(quad));
    }
    let prefixes = parser
        .prefixes()
        .map(|(p, ns)| (p.to_owned(), ns.to_owned()))
        .filter(|(p, _)| !p.is_empty())
        .collect();
    Ok((triples, prefixes))
}

/// Whether a repair only adds triples, or also removes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Insert,
    Replace,
}

#[derive(Deserialize)]
struct ManifestFile {
    checks: BTreeMap<String, Entry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    kind: String,
    title: String,
    template: Option<String>,
    templates_by_policy: Option<BTreeMap<String, String>>,
    policy_standards_key: Option<String>,
}

/// The repair a finding's check offers, if any: the template's text, its title and its kind.
#[must_use]
pub fn template(check_id: &str, standards: &Standards) -> Option<(&'static str, String, Kind)> {
    let manifest: ManifestFile =
        serde_json::from_str(crate::file("repairs/manifest.json")?).ok()?;
    let entry = manifest.checks.get(check_id)?;
    let name = match (
        &entry.template,
        &entry.templates_by_policy,
        &entry.policy_standards_key,
    ) {
        (Some(name), _, _) => name.clone(),
        (None, Some(by_policy), Some(key)) => {
            let policy = match key.as_str() {
                "equivalentClassPolicy" => &standards.equivalent_class_policy,
                "redundantEquivalencePolicy" => &standards.redundant_equivalence_policy,
                _ => return None,
            };
            by_policy.get(policy)?.clone()
        }
        _ => return None,
    };
    let text = crate::file(&format!("repairs/{name}"))?;
    let kind = if entry.kind == "replace" {
        Kind::Replace
    } else {
        Kind::Insert
    };
    Some((text, entry.title.clone(), kind))
}

/// An IRI's local name made readable, for a label a repair adds: `hasOwner` becomes
/// `has Owner`, `opening_hours` becomes `opening hours`.
#[must_use]
pub fn humanize_local_name(iri: &str) -> String {
    let mut local = iri;
    for separator in ['#', '/'] {
        if let Some((_, after)) = local.rsplit_once(separator) {
            local = after;
        }
    }
    // Python's re.sub(r"([a-z0-9])([A-Z])", r"\1 \2"): non-overlapping, left to right.
    let chars: Vec<char> = local.chars().collect();
    let mut spaced = String::with_capacity(local.len() + 4);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if (c.is_ascii_lowercase() || c.is_ascii_digit())
            && chars.get(i + 1).is_some_and(char::is_ascii_uppercase)
        {
            spaced.push(c);
            spaced.push(' ');
            spaced.push(chars[i + 1]);
            i += 2;
        } else {
            spaced.push(c);
            i += 1;
        }
    }
    // Then re.sub(r"[_-]+", " "), then strip().
    let mut out = String::with_capacity(spaced.len());
    let mut in_run = false;
    for c in spaced.chars() {
        if c == '_' || c == '-' {
            if !in_run {
                out.push(' ');
            }
            in_run = true;
        } else {
            out.push(c);
            in_run = false;
        }
    }
    out.trim().to_owned()
}

fn string_literal(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("\"{escaped}\"")
}

/// One absolute IRI, and nothing that would make it two terms: the suite's `ABSOLUTE_IRI`.
fn is_absolute_iri(text: &str) -> bool {
    let Some((scheme, rest)) = text.split_once(':') else {
        return false;
    };
    let mut scheme_chars = scheme.chars();
    scheme_chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && scheme_chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
        && !rest.is_empty()
        && !rest.chars().any(|c| {
            c.is_whitespace() || matches!(c, '<' | '>' | '"' | '{' | '}' | '|' | '^' | '`' | '\\')
        })
}

/// A finding's focus, path or value as a SPARQL IRI term, or `UNDEF` where it is not one
/// absolute IRI: a blank node's label, a path expression, or several values joined.
fn iri_or_undef(text: Option<&str>) -> String {
    match text {
        Some(t) if is_absolute_iri(t) => format!("<{t}>"),
        _ => "UNDEF".to_owned(),
    }
}

/// A CURIE in the standards, expanded with the standards' prefixes and the document's; an
/// IRI or anything else, unchanged.
fn expand(value: &str, prefixes: &BTreeMap<String, String>) -> String {
    if value.starts_with("http://") || value.starts_with("https://") {
        return value.to_owned();
    }
    match value.split_once(':') {
        Some((prefix, local)) => prefixes
            .get(prefix)
            .map_or_else(|| value.to_owned(), |ns| format!("{ns}{local}")),
        None => value.to_owned(),
    }
}

/// The template with the finding's `VALUES` row spliced in at the start of its `WHERE`
/// clause.
///
/// # Errors
///
/// If the template has no `WHERE` clause.
pub fn build_update(
    template: &str,
    finding: &Finding,
    standards: &Standards,
    document_prefixes: &BTreeMap<String, String>,
) -> Result<String, Error> {
    let insert_at = where_block(template)
        .ok_or_else(|| Error::new("the repair template has no WHERE clause"))?;
    let mut prefixes = standards.prefixes.clone();
    prefixes.extend(
        document_prefixes
            .iter()
            .map(|(k, v)| (k.clone(), v.clone())),
    );
    let focus = (!finding.focus_is_blank).then_some(finding.focus_node.as_str());
    let values = [
        iri_or_undef(focus),
        iri_or_undef(finding.path.as_deref()),
        iri_or_undef(finding.value.as_deref()),
        string_literal(&humanize_local_name(&finding.focus_node)),
        string_literal(&standards.default_language_tag),
        iri_or_undef(Some(&expand(&standards.category_class, &prefixes))),
        iri_or_undef(Some(&standards.default_ontology_base_iri)),
        string_literal(&standards.default_version_info),
    ];
    let clause = format!(
        "VALUES (?focusNode ?path ?value ?derivedLabel ?defaultLanguageTag ?categoryClass \
         ?defaultOntologyBaseIri ?defaultVersionInfo) {{\n    ({})\n  }}",
        values.join(" ")
    );
    Ok(format!(
        "{}\n  {clause}\n{}",
        &template[..insert_at],
        &template[insert_at..]
    ))
}

/// Where the first `WHERE {` (any case, a word on its own, any space before the brace)
/// ends: the suite's `\bWHERE\s*\{`.
fn where_block(text: &str) -> Option<usize> {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut from = 0;
    while let Some(found) = lower[from..].find("where") {
        let start = from + found;
        let end = start + "where".len();
        let boundary = start == 0 || !is_word(bytes[start - 1]);
        if boundary {
            let mut i = end;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if bytes.get(i) == Some(&b'{') {
                return Some(i + 1);
            }
        }
        from = end;
    }
    None
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// What one repair did.
#[derive(Clone, Debug)]
pub struct Outcome {
    pub check_id: String,
    pub title: String,
    pub kind: Kind,
    pub added: Vec<Triple>,
    pub removed: Vec<Triple>,
    /// The document's triples after the repair.
    pub result: Vec<Triple>,
    /// The update that was run.
    pub update: String,
}

/// Runs the repair for `finding` against `document`, the document's own triples, and
/// returns what changed. `None` when the finding's check offers no repair.
///
/// # Errors
///
/// If the update does not parse or fails to run.
pub fn compute(
    document: &[Triple],
    finding: &Finding,
    standards: &Standards,
    document_prefixes: &BTreeMap<String, String>,
) -> Result<Option<Outcome>, Error> {
    let Some(check_id) = finding.check_id.as_deref() else {
        return Ok(None);
    };
    let Some((template, title, kind)) = template(check_id, standards) else {
        return Ok(None);
    };
    let update = build_update(template, finding, standards, document_prefixes)?;

    let mut store = holos_store::Store::new();
    for t in document {
        store
            .insert(
                Quad::new(
                    t.subject.clone(),
                    t.predicate.clone(),
                    t.object.clone(),
                    GraphName::DefaultGraph,
                )
                .as_ref(),
            )
            .map_err(|e| Error::new(format!("loading the document: {e}")))?;
    }
    let mut engine = Holos::with_store(store);
    let mut session = Session::unrestricted(engine.store())
        .map_err(|e| Error::new(format!("opening a session: {e}")))?;
    holos_engine::update::update(&mut engine, &mut session, &update, None)
        .map_err(|e| Error::new(format!("{check_id}'s repair did not run: {e}")))?;
    let view = engine.view(&session);
    let (results, _) = Holos::query_with(
        &view,
        "CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }",
        &QueryOptions::new(),
    )
    .map_err(|e| Error::new(e.to_string()))?;
    let QueryResults::Graph(triples) = results else {
        return Err(Error::new("reading the repaired document back"));
    };
    let result: Vec<Triple> = triples
        .collect::<Result<_, _>>()
        .map_err(|e| Error::new(e.to_string()))?;

    let before: BTreeSet<String> = document.iter().map(ToString::to_string).collect();
    let after: BTreeSet<String> = result.iter().map(ToString::to_string).collect();
    let mut added: Vec<Triple> = result
        .iter()
        .filter(|t| !before.contains(&t.to_string()))
        .cloned()
        .collect();
    let mut removed: Vec<Triple> = document
        .iter()
        .filter(|t| !after.contains(&t.to_string()))
        .cloned()
        .collect();
    added.sort_by_key(ToString::to_string);
    removed.sort_by_key(ToString::to_string);
    Ok(Some(Outcome {
        check_id: check_id.to_owned(),
        title,
        kind,
        added,
        removed,
        result,
        update,
    }))
}

/// Triples as a Turtle block to append to a file: grouped by subject, IRIs shortened with
/// the document's prefixes where they can be, a blank line before each subject.
#[must_use]
pub fn render_added(triples: &[Triple], prefixes: &BTreeMap<String, String>) -> String {
    let shorten = |iri: &str| -> String {
        prefixes
            .iter()
            .filter(|(_, ns)| iri.starts_with(ns.as_str()))
            .max_by_key(|(_, ns)| ns.len())
            .and_then(|(prefix, ns)| {
                let local = &iri[ns.len()..];
                (!local.is_empty()
                    && local
                        .chars()
                        .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-')))
                .then(|| format!("{prefix}:{local}"))
            })
            .unwrap_or_else(|| format!("<{iri}>"))
    };
    let term = |t: &oxrdf::Term| -> String {
        match t {
            oxrdf::Term::NamedNode(n) => shorten(n.as_str()),
            other => other.to_string(),
        }
    };
    let mut by_subject: Vec<(String, Vec<&Triple>)> = Vec::new();
    for t in triples {
        let subject = match &t.subject {
            oxrdf::NamedOrBlankNode::NamedNode(n) => shorten(n.as_str()),
            other => other.to_string(),
        };
        match by_subject.iter_mut().find(|(s, _)| *s == subject) {
            Some((_, list)) => list.push(t),
            None => by_subject.push((subject, vec![t])),
        }
    }
    let mut out = String::new();
    for (subject, list) in by_subject {
        out.push('\n');
        out.push_str(&subject);
        out.push('\n');
        for (i, t) in list.iter().enumerate() {
            let predicate =
                if t.predicate.as_str() == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" {
                    "a".to_owned()
                } else {
                    shorten(t.predicate.as_str())
                };
            let end = if i + 1 == list.len() { "." } else { ";" };
            out.push_str(&format!("  {predicate} {} {end}\n", term(&t.object)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_names_read_as_the_suite_reads_them() {
        assert_eq!(
            humanize_local_name("https://example.org/o#hasOwner"),
            "has Owner"
        );
        assert_eq!(
            humanize_local_name("https://example.org/o/opening_hours"),
            "opening hours"
        );
        assert_eq!(
            humanize_local_name("https://example.org/o#hasURL"),
            "has URL"
        );
        assert_eq!(humanize_local_name("https://example.org/o#aBcD"), "a Bc D");
        assert_eq!(humanize_local_name("https://example.org/o#--x--"), "x");
    }

    #[test]
    fn only_one_absolute_iri_is_bound() {
        assert_eq!(
            iri_or_undef(Some("https://example.org/a")),
            "<https://example.org/a>"
        );
        assert_eq!(iri_or_undef(Some("b12")), "UNDEF");
        assert_eq!(
            iri_or_undef(Some("https://e.org/a, https://e.org/b")),
            "UNDEF"
        );
        assert_eq!(iri_or_undef(Some("(rdfs:subClassOf)+")), "UNDEF");
        assert_eq!(iri_or_undef(None), "UNDEF");
    }

    #[test]
    fn the_values_row_goes_in_at_the_where_brace() {
        let template = "INSERT { ?focusNode <urn:p> ?x . }\nwhere\n{ BIND(1 AS ?x) }";
        let at = where_block(template).expect("a WHERE");
        assert_eq!(&template[at - 1..at], "{");
        assert!(where_block("INSERT DATA { <urn:a> <urn:b> <urn:c> }").is_none());
        // `elsewhere {` is not a WHERE clause.
        assert!(
            where_block("# elsewhere { }\nINSERT {} WHERE{}")
                .is_some_and(|i| { "# elsewhere { }\nINSERT {} WHERE{}"[..i].ends_with("WHERE{") })
        );
    }
}
