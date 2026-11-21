//! XPath and XQuery Functions and Operators 3.1: `fn:` and `math:`.
//!
//! What SPARQL's own built-ins were derived from, and what queries ported from XQuery, or
//! written for Jena or GraphDB, call by these names. Each function follows the
//! specification's typing where SPARQL can express it: `fn:abs(-3)` is the integer 3, not
//! the double, and `fn:round(-2.5)` is -2, rounding half towards positive infinity as XPath
//! does, not away from zero as Rust's `f64::round` does.
//!
//! Functions that take or return a sequence (`fn:tokenize`, `fn:string-join`,
//! `fn:distinct-values` …) have no SPARQL form; [`crate::unsupported`] says what to use.

use crate::format;
use crate::value::{
    Calendar, Numeric, boolean, boolean_out, double_out, duration, integer, integer_out, lexical,
    number, percent_encode, string, text,
};
use crate::{Function, Now, Plain, plain};
use oxrdf::vocab::xsd;
use oxrdf::{Literal, Term};
use oxsdatatypes::{DayTimeDuration, Decimal, Double, Float, Integer, TimezoneOffset};
use regex::{Regex, RegexBuilder};
use std::str::FromStr;
use std::sync::Arc;

/// XPath and XQuery Functions and Operators.
pub const FN: &str = "http://www.w3.org/2005/xpath-functions#";
/// The XPath 3.1 math functions.
pub const MATH: &str = "http://www.w3.org/2005/xpath-functions/math#";

pub(crate) fn add(out: &mut Vec<(String, Function)>, now: &Now) {
    let fns: &[(&str, Plain)] = &[
        // strings
        ("upper-case", |a| string(text(a.first()?)?.to_uppercase())),
        ("lower-case", |a| string(text(a.first()?)?.to_lowercase())),
        ("string-length", |a| {
            integer_out(text(a.first()?)?.chars().count() as i64)
        }),
        ("string", |a| string(text(a.first()?)?)),
        ("concat", fn_concat),
        ("substring", fn_substring),
        ("substring-before", |a| {
            let (s, sep) = (text(a.first()?)?, text(a.get(1)?)?);
            string(s.split_once(&sep).map_or("", |(b, _)| b))
        }),
        ("substring-after", |a| {
            let (s, sep) = (text(a.first()?)?, text(a.get(1)?)?);
            string(s.split_once(&sep).map_or("", |(_, x)| x))
        }),
        ("contains", |a| {
            boolean_out(text(a.first()?)?.contains(&text(a.get(1)?)?))
        }),
        ("starts-with", |a| {
            boolean_out(text(a.first()?)?.starts_with(&text(a.get(1)?)?))
        }),
        ("ends-with", |a| {
            boolean_out(text(a.first()?)?.ends_with(&text(a.get(1)?)?))
        }),
        ("normalize-space", fn_normalize_space),
        ("translate", fn_translate),
        ("compare", fn_compare),
        ("matches", fn_matches),
        ("replace", fn_replace),
        ("encode-for-uri", |a| {
            string(percent_encode(&text(a.first()?)?, |c| {
                c.is_ascii_alphanumeric() || "-_.~".contains(c)
            }))
        }),
        ("iri-to-uri", |a| {
            string(percent_encode(&text(a.first()?)?, |c| {
                (' '..='~').contains(&c) && !"<>\" {}|\\^`".contains(c)
            }))
        }),
        ("escape-html-uri", |a| {
            string(percent_encode(&text(a.first()?)?, |c| {
                (' '..='~').contains(&c)
            }))
        }),
        // numbers
        ("abs", |a| {
            unary_numeric(a, |n| match n {
                Numeric::Integer(i) => i.checked_abs().map(Numeric::Integer),
                Numeric::Decimal(d) => d.checked_abs().map(Numeric::Decimal),
                Numeric::Float(f) => Some(Numeric::Float(f.abs())),
                Numeric::Double(d) => Some(Numeric::Double(d.abs())),
            })
        }),
        ("ceiling", |a| {
            unary_numeric(a, |n| match n {
                Numeric::Integer(_) => Some(n),
                Numeric::Decimal(d) => d.checked_ceil().map(Numeric::Decimal),
                Numeric::Float(f) => Some(Numeric::Float(f.ceil())),
                Numeric::Double(d) => Some(Numeric::Double(d.ceil())),
            })
        }),
        ("floor", |a| {
            unary_numeric(a, |n| match n {
                Numeric::Integer(_) => Some(n),
                Numeric::Decimal(d) => d.checked_floor().map(Numeric::Decimal),
                Numeric::Float(f) => Some(Numeric::Float(f.floor())),
                Numeric::Double(d) => Some(Numeric::Double(d.floor())),
            })
        }),
        ("round", |a| fn_round(a, Rounding::HalfUp)),
        ("round-half-to-even", |a| fn_round(a, Rounding::HalfEven)),
        ("number", fn_number),
        ("format-number", |a| {
            format::format_number(a, format::Dialect::XPath)
        }),
        // booleans
        ("not", |a| boolean_out(!effective_boolean(a.first()?)?)),
        ("boolean", |a| boolean_out(effective_boolean(a.first()?)?)),
        ("true", |a| a.is_empty().then(|| Literal::from(true).into())),
        ("false", |a| {
            a.is_empty().then(|| Literal::from(false).into())
        }),
        // dates, times and durations
        ("year-from-dateTime", |a| {
            integer_out(Calendar::date_time(a.first()?)?.year())
        }),
        ("month-from-dateTime", |a| {
            integer_out(Calendar::date_time(a.first()?)?.month().into())
        }),
        ("day-from-dateTime", |a| {
            integer_out(Calendar::date_time(a.first()?)?.day().into())
        }),
        ("hours-from-dateTime", |a| {
            integer_out(Calendar::date_time(a.first()?)?.hour().into())
        }),
        ("minutes-from-dateTime", |a| {
            integer_out(Calendar::date_time(a.first()?)?.minute().into())
        }),
        ("seconds-from-dateTime", |a| {
            Some(Literal::from(Calendar::date_time(a.first()?)?.second()).into())
        }),
        ("timezone-from-dateTime", |a| {
            timezone_out(Calendar::DateTime(Calendar::date_time(a.first()?)?))
        }),
        ("year-from-date", |a| {
            integer_out(Calendar::date(a.first()?)?.year())
        }),
        ("month-from-date", |a| {
            integer_out(Calendar::date(a.first()?)?.month().into())
        }),
        ("day-from-date", |a| {
            integer_out(Calendar::date(a.first()?)?.day().into())
        }),
        ("timezone-from-date", |a| {
            timezone_out(Calendar::Date(Calendar::date(a.first()?)?))
        }),
        ("hours-from-time", |a| {
            integer_out(Calendar::time(a.first()?)?.hour().into())
        }),
        ("minutes-from-time", |a| {
            integer_out(Calendar::time(a.first()?)?.minute().into())
        }),
        ("seconds-from-time", |a| {
            Some(Literal::from(Calendar::time(a.first()?)?.second()).into())
        }),
        ("timezone-from-time", |a| {
            timezone_out(Calendar::Time(Calendar::time(a.first()?)?))
        }),
        ("years-from-duration", |a| {
            integer_out(duration(a.first()?)?.years())
        }),
        ("months-from-duration", |a| {
            integer_out(duration(a.first()?)?.months())
        }),
        ("days-from-duration", |a| {
            integer_out(duration(a.first()?)?.days())
        }),
        ("hours-from-duration", |a| {
            integer_out(duration(a.first()?)?.hours())
        }),
        ("minutes-from-duration", |a| {
            integer_out(duration(a.first()?)?.minutes())
        }),
        ("seconds-from-duration", |a| {
            Some(Literal::from(duration(a.first()?)?.seconds()).into())
        }),
        ("adjust-dateTime-to-timezone", |a| {
            let d = Calendar::date_time(a.first()?)?;
            Some(Literal::from(d.adjust(Some(offset_argument(a.get(1))?))?).into())
        }),
        ("adjust-date-to-timezone", |a| {
            let d = Calendar::date(a.first()?)?;
            Some(Literal::from(d.adjust(Some(offset_argument(a.get(1))?))?).into())
        }),
        ("adjust-time-to-timezone", |a| {
            let t = Calendar::time(a.first()?)?;
            Some(Literal::from(t.adjust(Some(offset_argument(a.get(1))?))?).into())
        }),
        ("implicit-timezone", |a| {
            a.is_empty()
                .then(|| Literal::from(DayTimeDuration::default()).into())
        }),
        ("format-dateTime", |a| {
            format::format_date_time(a, format::Want::DateTime)
        }),
        ("format-date", |a| {
            format::format_date_time(a, format::Want::Date)
        }),
        ("format-time", |a| {
            format::format_date_time(a, format::Want::Time)
        }),
    ];
    for (name, f) in fns {
        out.push((format!("{FN}{name}"), plain(*f)));
    }

    // The current date and time are the query's: fixed when the library was made for it,
    // as SPARQL's NOW() is.
    let at = now.date_time;
    out.push((
        format!("{FN}current-dateTime"),
        Arc::new(move |a: &[Term]| a.is_empty().then(|| Literal::from(at).into())),
    ));
    out.push((
        format!("{FN}current-date"),
        Arc::new(move |a: &[Term]| {
            if a.is_empty() {
                oxsdatatypes::Date::try_from(at)
                    .ok()
                    .map(|d| Literal::from(d).into())
            } else {
                None
            }
        }),
    ));
    out.push((
        format!("{FN}current-time"),
        Arc::new(move |a: &[Term]| {
            a.is_empty()
                .then(|| Literal::from(oxsdatatypes::Time::from(at)).into())
        }),
    ));

    let maths: &[(&str, Plain)] = &[
        ("pi", |a| {
            if a.is_empty() {
                double_out(std::f64::consts::PI)
            } else {
                None
            }
        }),
        ("exp", |a| math1(a, f64::exp)),
        ("exp10", |a| math1(a, |x| 10_f64.powf(x))),
        ("log", |a| math1(a, f64::ln)),
        ("log10", |a| math1(a, f64::log10)),
        ("sqrt", |a| math1(a, f64::sqrt)),
        ("sin", |a| math1(a, f64::sin)),
        ("cos", |a| math1(a, f64::cos)),
        ("tan", |a| math1(a, f64::tan)),
        ("asin", |a| math1(a, f64::asin)),
        ("acos", |a| math1(a, f64::acos)),
        ("atan", |a| math1(a, f64::atan)),
        ("pow", |a| match a {
            [x, y] => double_out(math_pow(number(x)?, number(y)?)),
            _ => None,
        }),
        ("atan2", |a| match a {
            [y, x] => double_out(number(y)?.atan2(number(x)?)),
            _ => None,
        }),
    ];
    for (name, f) in maths {
        out.push((format!("{MATH}{name}"), plain(*f)));
    }
}

fn math1(a: &[Term], f: fn(f64) -> f64) -> Option<Term> {
    match a {
        [x] => double_out(f(number(x)?)),
        _ => None,
    }
}

/// `math:pow`, which IEEE 754-2008 and XPath define for the cases `powf` leaves to the
/// platform: `pow(-1, ±INF)` is 1, as is anything to the power 0.
fn math_pow(x: f64, y: f64) -> f64 {
    if y == 0.0 || (x == -1.0 && y.is_infinite()) {
        1.0
    } else {
        x.powf(y)
    }
}

fn fn_concat(a: &[Term]) -> Option<Term> {
    let mut out = String::new();
    for t in a {
        out.push_str(&text(t)?);
    }
    string(out)
}

/// XPath positions are 1-based and rounded, and a character at position `p` is kept when
/// `round(start) <= p < round(start) + round(length)`: so `substring("12345", -3, 5)` is "1".
fn fn_substring(a: &[Term]) -> Option<Term> {
    let s = text(a.first()?)?;
    let start = round_position(a.get(1)?)?;
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len() as f64;
    let end = match a.get(2) {
        Some(len) => start + round_position(len)?,
        None => f64::INFINITY,
    };
    // A NaN position selects nothing, as XPath says. (Checked first: f64::max would turn a NaN
    // start into 1.)
    if start.is_nan() || end.is_nan() {
        return string("");
    }
    let from = start.max(1.0);
    let to = end.min(n + 1.0);
    if from.partial_cmp(&to) != Some(std::cmp::Ordering::Less) {
        return string("");
    }
    string(
        chars[(from as usize - 1)..(to as usize - 1)]
            .iter()
            .collect::<String>(),
    )
}

fn round_position(t: &Term) -> Option<f64> {
    let x = number(t)?;
    Some(if x.is_finite() { round_half_up(x) } else { x })
}

fn round_half_up(x: f64) -> f64 {
    let r = x.round();
    if x.fract() == -0.5 { r + 1.0 } else { r }
}

/// The four XML whitespace characters only: `fn:normalize-space` does not touch U+00A0 or
/// the other Unicode spaces, which `split_whitespace` would.
fn fn_normalize_space(a: &[Term]) -> Option<Term> {
    let s = text(a.first()?)?;
    let parts: Vec<&str> = s
        .split([' ', '\t', '\n', '\r'])
        .filter(|p| !p.is_empty())
        .collect();
    string(parts.join(" "))
}

/// Character for character; a character with no replacement is removed.
fn fn_translate(a: &[Term]) -> Option<Term> {
    let (s, from, to) = (text(a.first()?)?, text(a.get(1)?)?, text(a.get(2)?)?);
    let from: Vec<char> = from.chars().collect();
    let to: Vec<char> = to.chars().collect();
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match from.iter().position(|f| *f == c) {
            None => out.push(c),
            Some(i) => out.extend(to.get(i)),
        }
    }
    string(out)
}

/// By code point, which is the order Rust compares UTF-8 strings in.
fn fn_compare(a: &[Term]) -> Option<Term> {
    let (x, y) = (text(a.first()?)?, text(a.get(1)?)?);
    integer_out(match x.cmp(&y) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    })
}

/// An XPath regular expression and its flags (`s`, `m`, `i`, `x`, `q`) as a Rust one.
///
/// The two languages agree on everything common queries use. XPath's back-references and
/// character-class subtraction have no Rust form: such a pattern does not compile, and the
/// call is an error rather than a match against something else.
pub(crate) fn xpath_regex(pattern: &str, flags: &str) -> Option<Regex> {
    let mut builder_pattern = pattern.to_owned();
    let (mut s, mut m, mut i) = (false, false, false);
    for f in flags.chars() {
        match f {
            's' => s = true,
            'm' => m = true,
            'i' => i = true,
            'x' => builder_pattern = strip_regex_whitespace(&builder_pattern),
            'q' => {}
            _ => return None,
        }
    }
    if flags.contains('q') {
        builder_pattern = regex::escape(pattern);
    }
    RegexBuilder::new(&builder_pattern)
        .dot_matches_new_line(s)
        .multi_line(m)
        .case_insensitive(i)
        .build()
        .ok()
}

/// The `x` flag: whitespace in the pattern is removed, except inside a character class.
fn strip_regex_whitespace(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let (mut in_class, mut escaped) = (false, false);
    for c in pattern.chars() {
        if escaped {
            out.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' => {
                escaped = true;
                out.push(c);
            }
            '[' => {
                in_class = true;
                out.push(c);
            }
            ']' => {
                in_class = false;
                out.push(c);
            }
            ' ' | '\t' | '\n' | '\r' if !in_class => {}
            _ => out.push(c),
        }
    }
    out
}

fn flags_of(a: &[Term], at: usize) -> Option<String> {
    match a.get(at) {
        None => Some(String::new()),
        Some(t) => lexical(t).map(ToOwned::to_owned),
    }
}

fn fn_matches(a: &[Term]) -> Option<Term> {
    if a.len() > 3 {
        return None;
    }
    let input = text(a.first()?)?;
    let re = xpath_regex(lexical(a.get(1)?)?, &flags_of(a, 2)?)?;
    boolean_out(re.is_match(&input))
}

/// `fn:replace(input, pattern, replacement [, flags])`. `$N` in the replacement is group N
/// (as many digits as name a group), `\$` and `\\` are the characters, and a pattern that
/// matches the empty string is an error, as XPath 3.1 has it.
fn fn_replace(a: &[Term]) -> Option<Term> {
    if a.len() > 4 {
        return None;
    }
    let input = text(a.first()?)?;
    let flags = flags_of(a, 3)?;
    let re = xpath_regex(lexical(a.get(1)?)?, &flags)?;
    if re.is_match("") {
        return None;
    }
    let replacement = lexical(a.get(2)?)?;
    if flags.contains('q') {
        return string(
            re.replace_all(&input, regex::NoExpand(replacement))
                .into_owned(),
        );
    }
    let groups = re.captures_len() - 1;
    let rust = xpath_replacement(replacement, groups)?;
    string(re.replace_all(&input, rust.as_str()).into_owned())
}

/// An XPath replacement string in Rust's `regex` syntax.
pub(crate) fn xpath_replacement(replacement: &str, groups: usize) -> Option<String> {
    let mut out = String::with_capacity(replacement.len() + 4);
    let mut chars = replacement.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next()? {
                '\\' => out.push('\\'),
                '$' => out.push_str("$$"),
                _ => return None,
            },
            '$' => {
                let first = chars.next()?.to_digit(10)? as usize;
                let mut n = first;
                while let Some(d) = chars.peek().and_then(|c| c.to_digit(10)) {
                    let longer = n * 10 + d as usize;
                    if longer > groups {
                        break;
                    }
                    n = longer;
                    chars.next();
                }
                if n <= groups {
                    out.push_str(&format!("${{{n}}}"));
                }
            }
            c => out.push(c),
        }
    }
    Some(out)
}

fn unary_numeric(a: &[Term], f: impl Fn(Numeric) -> Option<Numeric>) -> Option<Term> {
    match a {
        [x] => f(Numeric::of(x)?).map(Numeric::into_term),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Rounding {
    HalfUp,
    HalfEven,
}

/// `fn:round` and `fn:round-half-to-even`, with XPath 3.1's optional precision: the number
/// of decimal places, negative for tens, hundreds and so on. The result keeps the argument's
/// type.
fn fn_round(a: &[Term], mode: Rounding) -> Option<Term> {
    let n = Numeric::of(a.first()?)?;
    let precision = match a.get(1) {
        None => 0,
        Some(p) => i32::try_from(integer(p)?).ok()?,
    };
    if a.len() > 2 {
        return None;
    }
    Some(
        match n {
            Numeric::Integer(i) if precision >= 0 => Numeric::Integer(i),
            Numeric::Integer(i) => Numeric::Integer(
                Integer::try_from(round_decimal(Decimal::from(i), precision, mode)?).ok()?,
            ),
            Numeric::Decimal(d) => Numeric::Decimal(round_decimal(d, precision, mode)?),
            Numeric::Float(f) => {
                Numeric::Float(Float::from(
                    round_f64(f64::from(f32::from(f)), precision, mode) as f32,
                ))
            }
            Numeric::Double(d) => {
                Numeric::Double(Double::from(round_f64(f64::from(d), precision, mode)))
            }
        }
        .into_term(),
    )
}

fn ten_to(p: u32) -> Option<Decimal> {
    let mut d = Decimal::from(1);
    for _ in 0..p {
        d = d.checked_mul(Decimal::from(10))?;
    }
    Some(d)
}

fn round_decimal(d: Decimal, precision: i32, mode: Rounding) -> Option<Decimal> {
    let scale = ten_to(precision.unsigned_abs())?;
    let shifted = if precision >= 0 {
        d.checked_mul(scale)?
    } else {
        d.checked_div(scale)?
    };
    let rounded = match mode {
        Rounding::HalfUp => shifted.checked_round()?,
        Rounding::HalfEven => {
            let floor = shifted.checked_floor()?;
            let fraction = shifted.checked_sub(floor)?;
            let half = Decimal::from_str("0.5").ok()?;
            if fraction > half {
                floor.checked_add(Decimal::from(1))?
            } else if fraction < half {
                floor
            } else {
                // a tie: to the even neighbour
                let two = Decimal::from(2);
                if floor.checked_rem(two)? == Decimal::from(0) {
                    floor
                } else {
                    floor.checked_add(Decimal::from(1))?
                }
            }
        }
    };
    if precision >= 0 {
        rounded.checked_div(scale)
    } else {
        rounded.checked_mul(scale)
    }
}

fn round_f64(x: f64, precision: i32, mode: Rounding) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let scale = 10_f64.powi(precision);
    let shifted = x * scale;
    let rounded = match mode {
        Rounding::HalfUp => round_half_up(shifted),
        Rounding::HalfEven => {
            let r = shifted.round();
            if (shifted - shifted.trunc()).abs() == 0.5 {
                2.0 * (shifted / 2.0).round()
            } else {
                r
            }
        }
    };
    let out = rounded / scale;
    // -0.4 rounds to negative zero, as XPath says.
    if out == 0.0 && x.is_sign_negative() {
        -0.0
    } else {
        out
    }
}

/// `fn:number`: a double, or NaN when the argument does not convert.
fn fn_number(a: &[Term]) -> Option<Term> {
    let [x] = a else { return None };
    let value = Numeric::of(x).map(Numeric::to_f64).or_else(|| match x {
        Term::Literal(l) if l.datatype() == xsd::BOOLEAN => {
            boolean(x).map(|b| if b { 1.0 } else { 0.0 })
        }
        Term::Literal(l) => Double::from_str(l.value().trim()).ok().map(f64::from),
        _ => None,
    });
    double_out(value.unwrap_or(f64::NAN))
}

/// The XPath effective boolean value of one item: a boolean is itself, a number is false
/// when zero or NaN, a string when empty.
fn effective_boolean(t: &Term) -> Option<bool> {
    let Term::Literal(l) = t else { return None };
    if l.datatype() == xsd::BOOLEAN {
        return boolean(t);
    }
    if let Some(n) = Numeric::of(t).filter(|_| l.datatype() != xsd::STRING) {
        let f = n.to_f64();
        return Some(f != 0.0 && !f.is_nan());
    }
    Some(!l.value().is_empty())
}

fn timezone_out(c: Calendar) -> Option<Term> {
    Some(Literal::from(c.timezone()?).into())
}

/// The `$timezone` argument of the `adjust-*` functions: a day-time duration, or the implicit
/// timezone when it is left out. The implicit timezone is UTC here, everywhere.
fn offset_argument(t: Option<&Term>) -> Option<TimezoneOffset> {
    match t {
        None => Some(TimezoneOffset::UTC),
        Some(Term::Literal(l)) => {
            let d = DayTimeDuration::from_str(l.value()).ok()?;
            TimezoneOffset::try_from(d).ok()
        }
        Some(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacements_translate_to_rust() {
        assert_eq!(xpath_replacement("[$1]", 1).as_deref(), Some("[${1}]"));
        assert_eq!(xpath_replacement("$12", 1).as_deref(), Some("${1}2"));
        assert_eq!(xpath_replacement("$12", 12).as_deref(), Some("${12}"));
        assert_eq!(xpath_replacement("\\$x", 0).as_deref(), Some("$$x"));
        assert_eq!(xpath_replacement("$", 0), None);
        assert_eq!(xpath_replacement("\\n", 0), None);
        // a group that does not exist is the empty string
        assert_eq!(xpath_replacement("a$3b", 1).as_deref(), Some("ab"));
    }

    #[test]
    fn rounding_follows_xpath() {
        assert_eq!(round_f64(-2.5, 0, Rounding::HalfUp), -2.0);
        assert_eq!(round_f64(2.5, 0, Rounding::HalfUp), 3.0);
        assert_eq!(round_f64(2.5, 0, Rounding::HalfEven), 2.0);
        assert_eq!(round_f64(3.5, 0, Rounding::HalfEven), 4.0);
        assert_eq!(round_f64(1.125, 2, Rounding::HalfEven), 1.12);
        assert_eq!(round_f64(1234.0, -2, Rounding::HalfUp), 1200.0);
    }
}
