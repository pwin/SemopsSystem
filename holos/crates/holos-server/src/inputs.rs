//! Checks on what a request asks the server to read, made before anything reads it.
//!
//! Two kinds of input reach code that was written for trusted callers:
//!
//! - **`LOAD <file:…>`** in a SPARQL update. The engine loads any file the process can
//!   read, which is right for the `holos` command run by the store's owner and wrong for a
//!   network endpoint: any client allowed to update could read the server's files into the
//!   store (and see fragments of non-RDF ones in parse errors), or `LOAD <file:///dev/zero>`
//!   and never return. [`check_loads`] refuses every `LOAD` unless the operator names a
//!   directory with `--load-dir`, and then admits only regular files inside it, after
//!   resolving symbolic links.
//! - **RDF/XML bodies.** The RDF/XML parser sits on `quick-xml` 0.37, which has two
//!   denial-of-service defects fixed only in 0.41 (RUSTSEC-2026-0194: checking a start tag
//!   for duplicate attributes takes time quadratic in their number; RUSTSEC-2026-0195:
//!   namespace declarations are kept without bound). No `oxrdfxml` release uses 0.41 yet.
//!   [`check_rdf_xml`] refuses a document with more attributes on one element, or more
//!   namespace declarations, than any real RDF/XML has, before the parser sees it.

use std::path::{Path, PathBuf};

/// Attributes one element may carry. RDF/XML puts a property on an element as an attribute
/// at most once per property; a few dozen is a large real node element.
pub const MAX_ATTRIBUTES: usize = 256;

/// Namespace declarations one document may make, across all its elements.
pub const MAX_NAMESPACE_DECLARATIONS: usize = 4096;

/// Refuses an RDF/XML document that would take the XML parser quadratic time or unbounded
/// memory: one with more than [`MAX_ATTRIBUTES`] attributes on an element, or more than
/// [`MAX_NAMESPACE_DECLARATIONS`] namespace declarations in all.
///
/// A lexical scan, not a parse: it finds start tags, skipping comments, processing
/// instructions, CDATA sections, the document type declaration and quoted attribute
/// values, and counts. Anything it cannot make sense of is left for the parser to reject.
///
/// # Errors
///
/// A message saying which limit was passed.
pub fn check_rdf_xml(document: &[u8]) -> Result<(), String> {
    let mut namespaces = 0usize;
    let mut i = 0;
    let n = document.len();
    let find = |from: usize, needle: &[u8]| -> usize {
        document[from..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map_or(n, |p| from + p + needle.len())
    };
    while i < n {
        if document[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &document[i..];
        if rest.starts_with(b"<!--") {
            i = find(i + 4, b"-->");
        } else if rest.starts_with(b"<![CDATA[") {
            i = find(i + 9, b"]]>");
        } else if rest.starts_with(b"<?") {
            i = find(i + 2, b"?>");
        } else if rest.starts_with(b"<!") {
            // A declaration, possibly with an internal subset in brackets.
            let mut depth = 0usize;
            i += 2;
            while i < n {
                match document[i] {
                    b'[' => depth += 1,
                    b']' => depth = depth.saturating_sub(1),
                    b'>' if depth == 0 => break,
                    b'"' | b'\'' => i = find(i + 1, &[document[i]]) - 1,
                    _ => {}
                }
                i += 1;
            }
        } else if rest.starts_with(b"</") {
            i = find(i + 2, b">");
        } else {
            // A start tag: the element name, then `name="value"` pairs until `>`.
            i += 1;
            while i < n && !matches!(document[i], b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/') {
                i += 1;
            }
            let mut attributes = 0usize;
            loop {
                while i < n && matches!(document[i], b' ' | b'\t' | b'\r' | b'\n' | b'/') {
                    i += 1;
                }
                if i >= n || document[i] == b'>' {
                    i += 1;
                    break;
                }
                let start = i;
                while i < n && !matches!(document[i], b'=' | b' ' | b'\t' | b'\r' | b'\n' | b'>') {
                    i += 1;
                }
                let name = &document[start..i];
                while i < n && matches!(document[i], b' ' | b'\t' | b'\r' | b'\n') {
                    i += 1;
                }
                if i < n && document[i] == b'=' {
                    i += 1;
                    while i < n && matches!(document[i], b' ' | b'\t' | b'\r' | b'\n') {
                        i += 1;
                    }
                    if i < n && matches!(document[i], b'"' | b'\'') {
                        i = find(i + 1, &[document[i]]);
                    }
                }
                if name.is_empty() {
                    i += 1;
                    continue;
                }
                attributes += 1;
                if name == b"xmlns" || name.starts_with(b"xmlns:") {
                    namespaces += 1;
                    if namespaces > MAX_NAMESPACE_DECLARATIONS {
                        return Err(format!(
                            "this RDF/XML declares more than {MAX_NAMESPACE_DECLARATIONS} \
                             namespaces, which this server does not accept"
                        ));
                    }
                }
                if attributes > MAX_ATTRIBUTES {
                    return Err(format!(
                        "an element in this RDF/XML has more than {MAX_ATTRIBUTES} attributes, \
                         which this server does not accept"
                    ));
                }
            }
        }
    }
    Ok(())
}

/// What the engine reads for a `LOAD` source IRI, by the same rules it applies.
fn load_path(iri: &str) -> Option<PathBuf> {
    let rest = iri
        .strip_prefix("file:///")
        .or_else(|| iri.strip_prefix("file://"))?;
    Some(PathBuf::from(percent_decode(rest)))
}

/// The engine's percent-decoding for `LOAD` sources, byte for byte, so the path checked is
/// the path it opens. (`+` stays `+`: this is an IRI, not a form.)
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    // On bytes, not on `&str` slices: `%` followed by a multi-byte character would put a
    // slice boundary inside it, and slicing there panics.
    let hex = |b: u8| char::from(b).to_digit(16);
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(u8::try_from(hi * 16 + lo).unwrap_or(b'?'));
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Refuses an update's `LOAD` operations unless each reads a regular file inside
/// `load_dir`. With no `load_dir`, every `LOAD` is refused.
///
/// # Errors
///
/// A message naming the source that was refused, and why, without saying anything about
/// what is or is not on the server's disk outside the directory.
pub fn check_loads(update: &spargebra::Update, load_dir: Option<&Path>) -> Result<(), String> {
    for operation in &update.operations {
        let spargebra::GraphUpdateOperation::Load { source, .. } = operation else {
            continue;
        };
        let iri = source.as_str();
        let Some(dir) = load_dir else {
            return Err(format!(
                "LOAD <{iri}> is not allowed on this server; it reads files only from a \
                 directory named with --load-dir"
            ));
        };
        let refused = || format!("LOAD <{iri}>: only regular files under --load-dir can be loaded");
        let path = load_path(iri).ok_or_else(refused)?;
        let root = dir.canonicalize().map_err(|_| refused())?;
        let resolved = if path.is_absolute() {
            path
        } else {
            root.join(path)
        };
        let resolved = resolved.canonicalize().map_err(|_| refused())?;
        if !resolved.starts_with(&root) || !resolved.is_file() {
            return Err(refused());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_rdf_xml_passes() {
        let doc = br#"<?xml version="1.0"?>
<!DOCTYPE rdf:RDF [ <!ENTITY ex "http://example.org/"> ]>
<!-- a comment with <tags a="1"> inside -->
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:ex="http://example.org/">
  <rdf:Description rdf:about="http://example.org/a" ex:p="x > y" ex:q='single'>
    <ex:r><![CDATA[<not a="tag">]]></ex:r>
  </rdf:Description>
</rdf:RDF>"#;
        assert_eq!(check_rdf_xml(doc), Ok(()));
    }

    #[test]
    fn an_element_with_too_many_attributes_is_refused() {
        let mut doc = String::from("<rdf:RDF xmlns:rdf=\"r\"><rdf:Description");
        for k in 0..=MAX_ATTRIBUTES {
            doc.push_str(&format!(" a{k}=\"v\""));
        }
        doc.push_str("/></rdf:RDF>");
        assert!(check_rdf_xml(doc.as_bytes()).is_err());
        // The same number spread over separate elements is fine.
        let spread: String = (0..=MAX_ATTRIBUTES)
            .map(|k| format!("<e a{k}=\"v\"/>"))
            .collect();
        assert_eq!(check_rdf_xml(spread.as_bytes()), Ok(()));
    }

    #[test]
    fn too_many_namespace_declarations_are_refused() {
        let doc: String = (0..=MAX_NAMESPACE_DECLARATIONS)
            .map(|k| format!("<e xmlns:n{k}=\"urn:{k}\"/>"))
            .collect();
        let error = check_rdf_xml(doc.as_bytes()).expect_err("refused");
        assert!(error.contains("namespaces"), "{error}");
    }

    fn update(text: &str) -> spargebra::Update {
        spargebra::SparqlParser::new()
            .parse_update(text)
            .expect("parses")
    }

    #[test]
    fn decoding_a_source_never_panics() {
        for s in ["%a\u{e9}", "%\u{e9}x", "%", "%4", "%41", "a%2Fb", "%zz"] {
            let _ = percent_decode(s);
        }
        assert_eq!(percent_decode("a%2Fb+c"), "a/b+c");
    }

    #[test]
    fn load_is_refused_without_a_load_dir() {
        let error = check_loads(&update("LOAD <file:///etc/passwd>"), None).expect_err("refused");
        assert!(error.contains("--load-dir"), "{error}");
        assert_eq!(
            check_loads(&update("INSERT DATA { <a:s> <a:p> <a:o> }"), None),
            Ok(())
        );
    }

    #[test]
    fn load_reads_only_files_inside_the_load_dir() {
        let scratch = std::env::temp_dir().join(format!("holos-load-{}", std::process::id()));
        let inside = scratch.join("data");
        std::fs::create_dir_all(&inside).expect("dirs");
        std::fs::write(inside.join("ok.ttl"), "<a:s> <a:p> <a:o> .").expect("write");
        std::fs::write(scratch.join("secret.ttl"), "<a:s> <a:p> <a:o> .").expect("write");
        let as_iri = |p: &Path| format!("file:///{}", p.display()).replace('\\', "/");

        let ok = as_iri(&inside.join("ok.ttl").canonicalize().expect("exists"));
        assert_eq!(
            check_loads(&update(&format!("LOAD <{ok}>")), Some(&inside)),
            Ok(())
        );
        for refused in [
            as_iri(&scratch.join("secret.ttl")),
            as_iri(&inside.join("../secret.ttl")),
            as_iri(&inside.join("missing.ttl")),
            as_iri(&inside),
            "http://example.org/data.ttl".to_owned(),
        ] {
            assert!(
                check_loads(&update(&format!("LOAD <{refused}>")), Some(&inside)).is_err(),
                "{refused}"
            );
        }
        let _ = std::fs::remove_dir_all(&scratch);
    }
}
