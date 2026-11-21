//! SPIN and TopBraid's function library, `spif:`.
//!
//! Widely used in SHACL and SPIN rule sets, which is exactly where the validators used to
//! have none of them. The first twelve are HOLOS's, each written to TopBraid's documentation
//! at the time. Those added since follow RDF4J's implementation of the library
//! (`org.eclipse.rdf4j.spin.function.spif`), TopBraid's own documentation listing the
//! functions only inside its product: `spif:parseDate` and `spif:dateFormat` take Java
//! `SimpleDateFormat` patterns, `spif:replaceAll` is Java's `String.replaceAll`, `spif:mod`
//! is the truncated remainder in the arguments' common numeric type.
//!
//! `spif:hasAllObjects` reads the data and `spif:invoke` runs a SPIN function, so neither
//! can be a function of its arguments; [`crate::unsupported`] lists them.

use crate::format::{Dialect, format_number};
use crate::value::{
    Calendar, Numeric, boolean_out, double_out, lexical, percent_decode, percent_encode, string,
    text, typed,
};
use crate::{Function, Now, Plain, java, plain, pseudo_random};
use oxrdf::vocab::xsd;
use oxrdf::{Literal, NamedNode, Term};
use oxsdatatypes::{
    Boolean, Date, DateTime, DayTimeDuration, Decimal, Double, Duration, Float, Integer, Time,
    YearMonthDuration,
};
use regex::Regex;
use std::str::FromStr;
use std::sync::Arc;

/// SPIN / TopBraid.
pub const SPIF: &str = "http://spinrdf.org/spif#";

pub(crate) fn add(out: &mut Vec<(String, Function)>, now: &Now) {
    let spifs: &[(&str, Plain)] = &[
        ("trim", |a| string(text(a.first()?)?.trim())),
        ("indexOf", |a| {
            let (s, needle) = (text(a.first()?)?, text(a.get(1)?)?);
            // A character index, as every position here is.
            crate::value::integer_out(
                s.find(&needle)
                    .map_or(-1, |byte| s[..byte].chars().count() as i64),
            )
        }),
        ("lastIndexOf", |a| {
            let (s, needle) = (text(a.first()?)?, text(a.get(1)?)?);
            crate::value::integer_out(
                s.rfind(&needle)
                    .map_or(-1, |byte| s[..byte].chars().count() as i64),
            )
        }),
        ("buildString", |a| {
            string(fill_template(&text(a.first()?)?, &a[1..]))
        }),
        ("buildURI", build_uri),
        ("upperCase", |a| string(text(a.first()?)?.to_uppercase())),
        ("lowerCase", |a| string(text(a.first()?)?.to_lowercase())),
        ("titleCase", title_case),
        ("unCamelCase", un_camel_case),
        ("encodeURL", |a| {
            string(percent_encode(&text(a.first()?)?, |c| {
                c.is_ascii_alphanumeric() || "-_.~".contains(c)
            }))
        }),
        ("decodeURL", |a| string(percent_decode(&text(a.first()?)?))),
        ("generateUUID", |a| {
            a.is_empty()
                .then(|| NamedNode::new_unchecked(format!("urn:uuid:{}", pseudo_uuid())).into())
        }),
        // HOLOS's reading: the part after the last `#` or `/`. (TopBraid's looks for an
        // rdfs:label in the data first, which a function of its arguments cannot do.)
        ("name", |a| {
            let s = text(a.first()?)?;
            string(s.rfind(['#', '/']).map_or(s.as_str(), |i| &s[i + 1..]))
        }),
        ("replaceAll", replace_all),
        ("parseDate", |a| match a {
            [Term::Literal(value), Term::Literal(pattern)] => {
                java::parse_date(value.value(), pattern.value(), current_year())
            }
            _ => None,
        }),
        ("dateFormat", |a| match a {
            [value, Term::Literal(pattern)] => {
                string(java::format_date(Calendar::of(value)?, pattern.value())?)
            }
            _ => None,
        }),
        ("decimalFormat", |a| format_number(a, Dialect::Java)),
        ("isValidURI", |a| match a {
            [Term::Literal(l)] => boolean_out(NamedNode::new(l.value()).is_ok()),
            _ => None,
        }),
        ("timeMillis", |a| match a {
            [value] => millis_out(epoch_millis(Calendar::of(value)?)?),
            _ => None,
        }),
        ("mod", spif_mod),
        ("cast", cast),
        ("random", |a| {
            if a.is_empty() {
                double_out(pseudo_random())
            } else {
                None
            }
        }),
    ];
    for (name, f) in spifs {
        out.push((format!("{SPIF}{name}"), plain(*f)));
    }
    // The query's own time, as SPARQL's NOW() is: one value for the whole query.
    let millis = epoch_millis(Calendar::DateTime(now.date_time));
    out.push((
        format!("{SPIF}currentTimeMillis"),
        Arc::new(move |a: &[Term]| {
            if a.is_empty() {
                millis_out(millis?)
            } else {
                None
            }
        }),
    ));
}

fn current_year() -> i64 {
    DateTime::now().year()
}

/// `{?1}`, `{?2}` … filled with the arguments, counting from one.
fn fill_template(template: &str, args: &[Term]) -> String {
    let mut out = template.to_owned();
    for (i, arg) in args.iter().enumerate() {
        out = out.replace(&format!("{{?{}}}", i + 1), &text(arg).unwrap_or_default());
    }
    out
}

/// `spif:buildURI("<http://ex/{?1}>", …)`: a template in angle brackets, filled as
/// `spif:buildString` fills one. A template without the brackets is an error, as it is in
/// RDF4J, and so is a result that is not an IRI.
fn build_uri(a: &[Term]) -> Option<Term> {
    let template = lexical(a.first()?)?;
    let inner = template.strip_prefix('<')?.strip_suffix('>')?;
    NamedNode::new(fill_template(inner, &a[1..]))
        .ok()
        .map(Into::into)
}

/// Each word's first letter in upper case and the rest in lower case (HOLOS's reading of
/// TopBraid's description).
fn title_case(a: &[Term]) -> Option<Term> {
    let s = text(a.first()?)?;
    let mut out = String::with_capacity(s.len());
    let mut start_of_word = true;
    for c in s.chars() {
        if c.is_whitespace() {
            start_of_word = true;
            out.push(c);
        } else if start_of_word {
            out.extend(c.to_uppercase());
            start_of_word = false;
        } else {
            out.extend(c.to_lowercase());
        }
    }
    string(out)
}

fn un_camel_case(a: &[Term]) -> Option<Term> {
    let s = text(a.first()?)?;
    let mut out = String::with_capacity(s.len() + 8);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push(' ');
        }
        out.push(c);
    }
    string(out)
}

/// `spif:replaceAll(text, regex, replacement)`: Java's `String.replaceAll`. In the
/// replacement `$1` is a group (more digits while they still name one), `${name}` a named
/// group, and a backslash makes the next character itself.
fn replace_all(a: &[Term]) -> Option<Term> {
    let [Term::Literal(s), Term::Literal(re), replacement] = a else {
        return None;
    };
    let re = Regex::new(re.value()).ok()?;
    let rust = java_replacement(&text(replacement)?, re.captures_len() - 1)?;
    string(re.replace_all(s.value(), rust.as_str()).into_owned())
}

fn java_replacement(replacement: &str, groups: usize) -> Option<String> {
    let mut out = String::with_capacity(replacement.len() + 4);
    let mut chars = replacement.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next()? {
                '$' => out.push_str("$$"),
                c => out.push(c),
            },
            '$' => match chars.next()? {
                '{' => {
                    let mut name = String::new();
                    loop {
                        match chars.next()? {
                            '}' => break,
                            c => name.push(c),
                        }
                    }
                    out.push_str(&format!("${{{name}}}"));
                }
                d => {
                    let mut n = d.to_digit(10)? as usize;
                    if n > groups {
                        // Java throws IndexOutOfBoundsException: "No group n".
                        return None;
                    }
                    while let Some(next) = chars.peek().and_then(|c| c.to_digit(10)) {
                        let longer = n * 10 + next as usize;
                        if longer > groups {
                            break;
                        }
                        n = longer;
                        chars.next();
                    }
                    out.push_str(&format!("${{{n}}}"));
                }
            },
            c => out.push(c),
        }
    }
    Some(out)
}

fn epoch_millis(c: Calendar) -> Option<i64> {
    let (year, month, day) = (
        c.year().unwrap_or(1970),
        c.month().unwrap_or(1),
        c.day().unwrap_or(1),
    );
    let days = crate::format::days_from_civil(year, month, day);
    let seconds = c.second().unwrap_or_default();
    let millis_of_second: i64 = seconds
        .checked_mul(Decimal::from(1000))?
        .checked_floor()?
        .to_string()
        .parse()
        .ok()?;
    let offset = c.timezone().map_or(0, |tz| tz.hours() * 60 + tz.minutes());
    let minutes =
        days * 1440 + i64::from(c.hour().unwrap_or(0)) * 60 + i64::from(c.minute().unwrap_or(0))
            - offset;
    minutes.checked_mul(60_000)?.checked_add(millis_of_second)
}

fn millis_out(n: i64) -> Option<Term> {
    typed(n.to_string(), xsd::LONG)
}

/// `spif:mod(a, b)`: the remainder of truncating division, in the wider of the two numeric
/// types, as RDF4J computes it.
fn spif_mod(a: &[Term]) -> Option<Term> {
    let [x, y] = a else { return None };
    let (x, y) = (Numeric::of(x)?, Numeric::of(y)?);
    Some(
        match x.rank().max(y.rank()) {
            0 => {
                let (Numeric::Integer(x), Numeric::Integer(y)) = (x, y) else {
                    return None;
                };
                Numeric::Integer(x.checked_rem(y)?)
            }
            1 => Numeric::Decimal(x.to_decimal()?.checked_rem(y.to_decimal()?)?),
            2 => Numeric::Float(Float::from((x.to_f64() as f32) % (y.to_f64() as f32))),
            _ => Numeric::Double(Double::from(x.to_f64() % y.to_f64())),
        }
        .into_term(),
    )
}

/// `spif:cast(value, datatype)`: an XML Schema constructor where there is one, which checks
/// the value and writes it canonically; otherwise the lexical form with the datatype.
fn cast(a: &[Term]) -> Option<Term> {
    let [Term::Literal(value), Term::NamedNode(datatype)] = a else {
        return None;
    };
    let v = value.value().trim();
    let canonical = |l: Option<Literal>| l.map(Term::from);
    let Some(local) = datatype
        .as_str()
        .strip_prefix("http://www.w3.org/2001/XMLSchema#")
    else {
        return Some(Literal::new_typed_literal(value.value(), datatype.clone()).into());
    };
    match local {
        "string" => string(value.value()),
        "boolean" => canonical(Boolean::from_str(v).ok().map(Literal::from)),
        "integer" => canonical(Integer::from_str(v).ok().map(Literal::from)),
        "decimal" => canonical(Decimal::from_str(v).ok().map(Literal::from)),
        "float" => canonical(Float::from_str(v).ok().map(Literal::from)),
        "double" => canonical(Double::from_str(v).ok().map(Literal::from)),
        "dateTime" => canonical(DateTime::from_str(v).ok().map(Literal::from)),
        "date" => canonical(Date::from_str(v).ok().map(Literal::from)),
        "time" => canonical(Time::from_str(v).ok().map(Literal::from)),
        "duration" => canonical(Duration::from_str(v).ok().map(Literal::from)),
        "dayTimeDuration" => canonical(DayTimeDuration::from_str(v).ok().map(Literal::from)),
        "yearMonthDuration" => canonical(YearMonthDuration::from_str(v).ok().map(Literal::from)),
        _ => Some(Literal::new_typed_literal(value.value(), datatype.clone()).into()),
    }
}

/// A version-4-shaped identifier. No `uuid` dependency for this one use: the shape and the
/// variability are what matter, and it is documented as pseudo-random rather than claimed to
/// be a conforming UUID.
fn pseudo_uuid() -> String {
    let (a, b) = (crate::pseudo_u64(), crate::pseudo_u64());
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        a >> 32,
        (a >> 16) & 0xFFFF,
        a & 0x0FFF,
        0x8000 | (b & 0x3FFF),
        b >> 16
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_replacements_translate_to_rust() {
        assert_eq!(java_replacement("[$1]", 1).as_deref(), Some("[${1}]"));
        assert_eq!(java_replacement("$12", 1).as_deref(), Some("${1}2"));
        assert_eq!(java_replacement("\\$1", 1).as_deref(), Some("$$1"));
        assert_eq!(java_replacement("${year}", 1).as_deref(), Some("${year}"));
        assert_eq!(java_replacement("$2", 1), None, "no group 2");
    }

    #[test]
    fn millis_count_from_the_epoch_in_utc() {
        let c = |v: &str| Calendar::DateTime(DateTime::from_str(v).unwrap());
        assert_eq!(epoch_millis(c("1970-01-01T00:00:00Z")), Some(0));
        assert_eq!(epoch_millis(c("1970-01-01T01:00:00+01:00")), Some(0));
        assert_eq!(
            epoch_millis(c("2026-08-25T13:05:07.25Z")),
            Some(1_787_663_107_250)
        );
    }
}
