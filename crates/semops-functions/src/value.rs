//! Reading arguments and writing results.
//!
//! Every function takes the terms the evaluator hands it and answers `None` for anything it
//! cannot take, which SPARQL treats as an error: the expression is unbound, or a `FILTER`
//! is false. None of them panic, whatever they are called with.

use oxrdf::vocab::xsd;
use oxrdf::{Literal, NamedNodeRef, Term};
use oxsdatatypes::{
    Date, DateTime, DayTimeDuration, Decimal, Double, Duration, Float, Integer, Time,
};
use std::str::FromStr;

/// The lexical form of a term, whatever kind it is.
///
/// Deliberately permissive, as HOLOS's functions always were: `fn:string-length(<http://…>)`
/// is meaningful, and refusing it because the argument is an IRI would be pedantry. Functions
/// whose own specification refuses an IRI (Jena's `afn:` ones) check for themselves.
pub(crate) fn text(term: &Term) -> Option<String> {
    Some(match term {
        Term::Literal(l) => l.value().to_owned(),
        Term::NamedNode(n) => n.as_str().to_owned(),
        Term::BlankNode(b) => b.as_str().to_owned(),
        #[allow(unreachable_patterns)]
        _ => return None,
    })
}

/// A literal's lexical form, refusing IRIs and blank nodes.
pub(crate) fn lexical(term: &Term) -> Option<&str> {
    match term {
        Term::Literal(l) => Some(l.value()),
        _ => None,
    }
}

pub(crate) fn string(s: impl Into<String>) -> Option<Term> {
    Some(Literal::new_simple_literal(s.into()).into())
}

pub(crate) fn boolean_out(b: bool) -> Option<Term> {
    Some(Literal::from(b).into())
}

pub(crate) fn integer_out(n: i64) -> Option<Term> {
    Some(Literal::from(n).into())
}

/// An `xsd:double`, written as `oxsdatatypes` writes one: `2`, `0.5`, `INF`, `NaN`.
pub(crate) fn double_out(n: f64) -> Option<Term> {
    Some(Literal::from(Double::from(n)).into())
}

pub(crate) fn typed(lexical: impl Into<String>, datatype: NamedNodeRef<'_>) -> Option<Term> {
    Some(Literal::new_typed_literal(lexical.into(), datatype).into())
}

/// A truth value as XPath reads `xs:boolean`: `true`, `false`, `1` or `0`.
pub(crate) fn boolean(term: &Term) -> Option<bool> {
    match lexical(term)? {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

/// An integer argument, such as a position. XPath rounds a non-integral one rather than
/// refusing it.
pub(crate) fn integer(term: &Term) -> Option<i64> {
    let l = lexical(term)?;
    l.parse::<i64>().ok().or_else(|| {
        let f = l.parse::<f64>().ok()?;
        f.is_finite().then(|| f.round() as i64)
    })
}

const INTEGER_TYPES: [&str; 13] = [
    "integer",
    "long",
    "int",
    "short",
    "byte",
    "nonNegativeInteger",
    "positiveInteger",
    "nonPositiveInteger",
    "negativeInteger",
    "unsignedLong",
    "unsignedInt",
    "unsignedShort",
    "unsignedByte",
];

/// A number, in the XPath type it has.
///
/// Integer subtypes (`xsd:int`, `xsd:long` …) read as `xsd:integer`, which is the type XPath
/// gives their results. A plain string that spells a number reads as an `xsd:double`: that is
/// how XPath promotes an untyped value, and how queries written for other stores pass numbers
/// they read out of text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Numeric {
    Integer(Integer),
    Decimal(Decimal),
    Float(Float),
    Double(Double),
}

impl Numeric {
    pub(crate) fn of(term: &Term) -> Option<Self> {
        let Term::Literal(l) = term else { return None };
        let value = l.value().trim();
        let local = l
            .datatype()
            .as_str()
            .strip_prefix("http://www.w3.org/2001/XMLSchema#");
        match local {
            Some(t) if INTEGER_TYPES.contains(&t) => {
                Integer::from_str(value).ok().map(Self::Integer)
            }
            Some("decimal") => Decimal::from_str(value).ok().map(Self::Decimal),
            Some("float") => Float::from_str(value).ok().map(Self::Float),
            Some("double") => Double::from_str(value).ok().map(Self::Double),
            Some("string") if l.language().is_none() => {
                Double::from_str(value).ok().map(Self::Double)
            }
            _ => None,
        }
    }

    pub(crate) fn to_f64(self) -> f64 {
        match self {
            Self::Integer(i) => f64::from(Double::from(i)),
            Self::Decimal(d) => f64::from(Double::from(d)),
            Self::Float(f) => f64::from(f32::from(f)),
            Self::Double(d) => f64::from(d),
        }
    }

    /// A decimal, when the value is one (not NaN or infinite).
    pub(crate) fn to_decimal(self) -> Option<Decimal> {
        match self {
            Self::Integer(i) => Some(Decimal::from(i)),
            Self::Decimal(d) => Some(d),
            Self::Float(f) => Decimal::try_from(f).ok(),
            Self::Double(d) => Decimal::try_from(d).ok(),
        }
    }

    pub(crate) fn into_term(self) -> Term {
        match self {
            Self::Integer(i) => Literal::from(i).into(),
            Self::Decimal(d) => Literal::from(d).into(),
            Self::Float(f) => Literal::from(f).into(),
            Self::Double(d) => Literal::from(d).into(),
        }
    }

    /// The type two numbers are promoted to for arithmetic: the wider of the two, in the
    /// order integer, decimal, float, double.
    pub(crate) fn rank(self) -> u8 {
        match self {
            Self::Integer(_) => 0,
            Self::Decimal(_) => 1,
            Self::Float(_) => 2,
            Self::Double(_) => 3,
        }
    }
}

/// A number as `f64`, for the functions that work in doubles (`math:`, `afn:sqrt` …).
pub(crate) fn number(term: &Term) -> Option<f64> {
    Numeric::of(term).map(Numeric::to_f64)
}

/// A calendar value: what the `*-from-dateTime`, `*-from-date` and `*-from-time` families
/// and the formatting functions read.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Calendar {
    DateTime(DateTime),
    Date(Date),
    Time(Time),
}

impl Calendar {
    /// Reads the type a literal says it has. A plain string is read as a date-time, then as a
    /// date, then as a time, which is what HOLOS's functions did with ISO text.
    pub(crate) fn of(term: &Term) -> Option<Self> {
        let Term::Literal(l) = term else { return None };
        let v = l.value();
        let dt = l.datatype();
        if dt == xsd::DATE_TIME || dt == xsd::DATE_TIME_STAMP {
            DateTime::from_str(v).ok().map(Self::DateTime)
        } else if dt == xsd::DATE {
            Date::from_str(v).ok().map(Self::Date)
        } else if dt == xsd::TIME {
            Time::from_str(v).ok().map(Self::Time)
        } else if dt == xsd::STRING && l.language().is_none() {
            DateTime::from_str(v)
                .map(Self::DateTime)
                .or_else(|_| Date::from_str(v).map(Self::Date))
                .or_else(|_| Time::from_str(v).map(Self::Time))
                .ok()
        } else {
            None
        }
    }

    pub(crate) fn date_time(term: &Term) -> Option<DateTime> {
        match Self::of(term)? {
            Self::DateTime(d) => Some(d),
            _ => None,
        }
    }

    pub(crate) fn date(term: &Term) -> Option<Date> {
        match Self::of(term)? {
            Self::Date(d) => Some(d),
            _ => None,
        }
    }

    pub(crate) fn time(term: &Term) -> Option<Time> {
        match Self::of(term)? {
            Self::Time(t) => Some(t),
            _ => None,
        }
    }

    pub(crate) fn year(self) -> Option<i64> {
        match self {
            Self::DateTime(d) => Some(d.year()),
            Self::Date(d) => Some(d.year()),
            Self::Time(_) => None,
        }
    }

    pub(crate) fn month(self) -> Option<u8> {
        match self {
            Self::DateTime(d) => Some(d.month()),
            Self::Date(d) => Some(d.month()),
            Self::Time(_) => None,
        }
    }

    pub(crate) fn day(self) -> Option<u8> {
        match self {
            Self::DateTime(d) => Some(d.day()),
            Self::Date(d) => Some(d.day()),
            Self::Time(_) => None,
        }
    }

    pub(crate) fn hour(self) -> Option<u8> {
        match self {
            Self::DateTime(d) => Some(d.hour()),
            Self::Time(t) => Some(t.hour()),
            Self::Date(_) => None,
        }
    }

    pub(crate) fn minute(self) -> Option<u8> {
        match self {
            Self::DateTime(d) => Some(d.minute()),
            Self::Time(t) => Some(t.minute()),
            Self::Date(_) => None,
        }
    }

    pub(crate) fn second(self) -> Option<Decimal> {
        match self {
            Self::DateTime(d) => Some(d.second()),
            Self::Time(t) => Some(t.second()),
            Self::Date(_) => None,
        }
    }

    pub(crate) fn timezone(self) -> Option<DayTimeDuration> {
        match self {
            Self::DateTime(d) => d.timezone(),
            Self::Date(d) => d.timezone(),
            Self::Time(t) => t.timezone(),
        }
    }
}

/// Any duration: `xsd:duration`, `xsd:yearMonthDuration` or `xsd:dayTimeDuration`.
pub(crate) fn duration(term: &Term) -> Option<Duration> {
    let Term::Literal(l) = term else { return None };
    let dt = l.datatype().as_str();
    let ok = dt == xsd::DURATION.as_str()
        || dt == xsd::YEAR_MONTH_DURATION.as_str()
        || dt == xsd::DAY_TIME_DURATION.as_str()
        || (l.datatype() == xsd::STRING && l.language().is_none());
    ok.then(|| Duration::from_str(l.value()).ok()).flatten()
}

/// Splits an IRI where Jena does (`SplitIRI.splitXML`, which `afn:localname`,
/// `afn:namespace` and `apf:splitIRI` use): before the longest XML NCName at its end that
/// starts with a name start character, never inside a `%xx` escape, and not straight after
/// `mailto:`. Gives the IRI's length when there is no local name (`…/123`, `…/`).
pub fn split_xml(iri: &str) -> usize {
    let chars: Vec<(usize, char)> = iri.char_indices().collect();
    let len = chars.len();
    if len == 0 {
        return 0;
    }
    // back from the end over name characters, down to the second character at most
    let mut i = len - 1;
    while i >= 1 && is_ncname_char(chars[i].1) {
        i -= 1;
    }
    let mut j = i + 1;
    if j >= len {
        return iri.len();
    }
    // not inside a %-escape
    if j >= 2 && chars[j - 2].1 == '%' {
        j += 1;
    }
    if j >= 1 && chars[j - 1].1 == '%' {
        j += 2;
        if j > len {
            return iri.len();
        }
    }
    // forward to a name start character
    while j < len {
        if is_ncname_start(chars[j].1) && !(j == 7 && iri.starts_with("mailto:")) {
            break;
        }
        j += 1;
    }
    chars.get(j).map_or(iri.len(), |(at, _)| *at)
}

/// XML 1.1's NameStartChar, without the colon.
fn is_ncname_start(c: char) -> bool {
    matches!(c,
        'A'..='Z' | '_' | 'a'..='z' | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}'
        | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}' | '\u{200C}'..='\u{200D}' | '\u{2070}'..='\u{218F}'
        | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}' | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}'
        | '\u{10000}'..='\u{EFFFF}')
}

/// XML 1.1's NameChar, without the colon.
fn is_ncname_char(c: char) -> bool {
    is_ncname_start(c)
        || matches!(c, '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
}

/// Percent-encodes every byte of `s` that `keep` does not accept, in upper-case hex.
pub(crate) fn percent_encode(s: &str, keep: impl Fn(char) -> bool) -> String {
    let mut out = String::with_capacity(s.len());
    let mut buf = [0_u8; 4];
    for c in s.chars() {
        if keep(c) {
            out.push(c);
        } else {
            for byte in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    out
}

/// Undoes `%xx` escapes, leaving a malformed one as it is.
pub(crate) fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(byte) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iris_split_before_an_xml_name() {
        // As Jena 4.8 splits them (TARQL 1.3, checked).
        let parts = |iri: &str| {
            let at = split_xml(iri);
            (iri[..at].to_owned(), iri[at..].to_owned())
        };
        assert_eq!(
            parts("http://example.com/ns#thing"),
            ("http://example.com/ns#".into(), "thing".into())
        );
        assert_eq!(
            parts("http://example.com/a/b"),
            ("http://example.com/a/".into(), "b".into())
        );
        assert_eq!(
            parts("http://example.com/item/123"),
            ("http://example.com/item/123".into(), "".into())
        );
        assert_eq!(
            parts("urn:isbn:0451450523"),
            ("urn:isbn:0451450523".into(), "".into())
        );
        assert_eq!(
            parts("mailto:me@example.com"),
            ("mailto:me@".into(), "example.com".into())
        );
        assert_eq!(
            parts("http://ex.org/a-1.b"),
            ("http://ex.org/".into(), "a-1.b".into())
        );
        assert_eq!(
            parts("http://example.com/"),
            ("http://example.com/".into(), "".into())
        );
        assert_eq!(
            parts("http://ex.org/p%20q"),
            ("http://ex.org/p%20".into(), "q".into())
        );
    }

    #[test]
    fn percent_decoding_reaches_the_last_escape() {
        assert_eq!(percent_decode("a%20b%41"), "a bA");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }

    #[test]
    fn numbers_keep_their_type() {
        let lit = |v: &str, t: NamedNodeRef<'_>| Term::from(Literal::new_typed_literal(v, t));
        assert!(matches!(
            Numeric::of(&lit("3", xsd::INT)),
            Some(Numeric::Integer(_))
        ));
        assert!(matches!(
            Numeric::of(&lit("3.5", xsd::DECIMAL)),
            Some(Numeric::Decimal(_))
        ));
        assert!(matches!(
            Numeric::of(&lit("3.5", xsd::DOUBLE)),
            Some(Numeric::Double(_))
        ));
        assert!(matches!(
            Numeric::of(&Literal::new_simple_literal("2.5").into()),
            Some(Numeric::Double(_))
        ));
        assert_eq!(
            Numeric::of(&Literal::new_simple_literal("two").into()),
            None
        );
    }
}
