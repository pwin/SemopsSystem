//! `DAT-001` over every literal: is its lexical form in its datatype's lexical space?
//!
//! The suite's `checks/literal_typing.py` does this with rdflib's conversion to Python
//! values, to cover what its `DAT-001` queries' regular expressions cannot. Here the
//! lexical forms are checked against XML Schema directly, through `oxsdatatypes`, for the
//! datatypes rdflib converts; a literal of any other datatype is passed over, as there.
//! Results are `sh:ValidationResult`s, so they merge with the queries' under one key.

use oxrdf::vocab::rdf;
use oxrdf::{BlankNode, Graph, Literal, NamedNodeRef, Term, Triple, TripleRef};
use oxsdatatypes::{
    Boolean, Date, DateTime, DayTimeDuration, Decimal, Double, Duration, Float, GDay, GMonth,
    GMonthDay, GYear, GYearMonth, Integer, Time, YearMonthDuration,
};
use std::str::FromStr;

pub const SOURCE: &str = "literal-typing";

const SH: &str = "http://www.w3.org/ns/shacl#";

/// Whether a lexical form is valid for a datatype: `Some(true)` or `Some(false)` for a
/// datatype this knows, `None` for one it does not.
fn valid(lexical: &str, datatype: &str) -> Option<bool> {
    let local = datatype.strip_prefix("http://www.w3.org/2001/XMLSchema#")?;
    let integer = |min: Option<i128>, max: Option<i128>| {
        Integer::from_str(lexical).is_ok()
            && lexical
                .trim()
                .parse::<i128>()
                .map_or(min.is_none() && max.is_none(), |v| {
                    min.is_none_or(|m| v >= m) && max.is_none_or(|m| v <= m)
                })
    };
    Some(match local {
        "boolean" => Boolean::from_str(lexical).is_ok(),
        "decimal" => Decimal::from_str(lexical).is_ok(),
        "float" => Float::from_str(lexical).is_ok(),
        "double" => Double::from_str(lexical).is_ok(),
        "integer" => Integer::from_str(lexical).is_ok(),
        "long" => integer(Some(i128::from(i64::MIN)), Some(i128::from(i64::MAX))),
        "int" => integer(Some(i128::from(i32::MIN)), Some(i128::from(i32::MAX))),
        "short" => integer(Some(i128::from(i16::MIN)), Some(i128::from(i16::MAX))),
        "byte" => integer(Some(i128::from(i8::MIN)), Some(i128::from(i8::MAX))),
        "nonNegativeInteger" => integer(Some(0), None),
        "positiveInteger" => integer(Some(1), None),
        "nonPositiveInteger" => integer(None, Some(0)),
        "negativeInteger" => integer(None, Some(-1)),
        "unsignedLong" => integer(Some(0), Some(i128::from(u64::MAX))),
        "unsignedInt" => integer(Some(0), Some(i128::from(u32::MAX))),
        "unsignedShort" => integer(Some(0), Some(i128::from(u16::MAX))),
        "unsignedByte" => integer(Some(0), Some(i128::from(u8::MAX))),
        "date" => Date::from_str(lexical).is_ok(),
        "dateTime" => DateTime::from_str(lexical).is_ok(),
        "time" => Time::from_str(lexical).is_ok(),
        "duration" => Duration::from_str(lexical).is_ok(),
        "dayTimeDuration" => DayTimeDuration::from_str(lexical).is_ok(),
        "yearMonthDuration" => YearMonthDuration::from_str(lexical).is_ok(),
        "gYear" => GYear::from_str(lexical).is_ok(),
        "gYearMonth" => GYearMonth::from_str(lexical).is_ok(),
        "gMonth" => GMonth::from_str(lexical).is_ok(),
        "gDay" => GDay::from_str(lexical).is_ok(),
        "gMonthDay" => GMonthDay::from_str(lexical).is_ok(),
        "hexBinary" => {
            let t = lexical.trim();
            t.len().is_multiple_of(2) && t.bytes().all(|b| b.is_ascii_hexdigit())
        }
        "base64Binary" => base64(lexical),
        _ => return None,
    })
}

/// The `xs:base64Binary` lexical space, whitespace allowed between characters.
fn base64(lexical: &str) -> bool {
    let chars: Vec<u8> = lexical
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    if !chars.len().is_multiple_of(4) {
        return false;
    }
    let padding = chars.iter().rev().take_while(|&&b| b == b'=').count();
    padding <= 2
        && chars[..chars.len() - padding]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'+' || *b == b'/')
}

/// A results graph with one `DAT-001` finding per literal outside its datatype's lexical
/// space.
pub fn check(triples: &[Triple]) -> Graph {
    let mut results = Graph::new();
    let iri = |local: &str| oxrdf::NamedNode::new_unchecked(format!("{SH}{local}"));
    let (kind, severity, focus, path, value, component, message, violation) = (
        rdf::TYPE,
        iri("resultSeverity"),
        iri("focusNode"),
        iri("resultPath"),
        iri("value"),
        iri("sourceConstraintComponent"),
        iri("resultMessage"),
        iri("Violation"),
    );
    let check = oxrdf::NamedNode::new_unchecked(format!("{}DAT-001", crate::NAMESPACE));
    for triple in triples {
        let Term::Literal(literal) = &triple.object else {
            continue;
        };
        if valid(literal.value(), literal.datatype().as_str()) != Some(false) {
            continue;
        }
        let result = BlankNode::default();
        let subject = match &triple.subject {
            oxrdf::NamedOrBlankNode::NamedNode(n) => n.as_str().to_owned(),
            oxrdf::NamedOrBlankNode::BlankNode(b) => b.as_str().to_owned(),
        };
        let text = Literal::new_simple_literal(format!(
            "Literal '{}' on {subject} (via {}) is not a valid {} -- it does not parse into \
             that datatype's value space.",
            literal.value(),
            triple.predicate.as_str(),
            literal.datatype().as_str()
        ));
        let validation_result =
            NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#ValidationResult");
        for (p, o) in [
            (kind, oxrdf::TermRef::from(validation_result)),
            (severity.as_ref(), violation.as_ref().into()),
            (focus.as_ref(), triple.subject.as_ref().into()),
            (path.as_ref(), triple.predicate.as_ref().into()),
            (value.as_ref(), literal.as_ref().into()),
            (component.as_ref(), check.as_ref().into()),
            (message.as_ref(), text.as_ref().into()),
        ] {
            results.insert(TripleRef::new(&result, p, o));
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::valid;

    const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

    #[test]
    fn lexical_spaces() {
        for (lexical, local, expected) in [
            ("12", "integer", Some(true)),
            ("12x", "integer", Some(false)),
            ("yesplz", "boolean", Some(false)),
            ("true", "boolean", Some(true)),
            ("31-13-2020", "date", Some(false)),
            ("2020-12-31", "date", Some(true)),
            ("128", "byte", Some(false)),
            ("-1", "nonNegativeInteger", Some(false)),
            ("0A1b", "hexBinary", Some(true)),
            ("0A1", "hexBinary", Some(false)),
            ("aGVsbG8=", "base64Binary", Some(true)),
            ("aGVsbG8", "base64Binary", Some(false)),
            ("anything", "string", None),
        ] {
            assert_eq!(
                valid(lexical, &format!("{XSD}{local}")),
                expected,
                "{lexical} {local}"
            );
        }
        assert_eq!(valid("x", "http://example.org/custom"), None);
    }
}
