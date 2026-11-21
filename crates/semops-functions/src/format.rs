//! Picture strings: XPath's `fn:format-number` and `fn:format-dateTime` (with `-date` and
//! `-time`), and the decimal pattern Java's `DecimalFormat` reads, which `spif:decimalFormat`
//! uses. The two number languages are nearly one language, so they share an implementation
//! and differ only where [`Dialect`] says.
//!
//! What is here is the part of each specification that has an unambiguous answer without
//! locale data: English names, the Gregorian calendar, ASCII digits. A picture that asks for
//! more (another calendar, a language other than English, an exponent) is an error rather
//! than a guess.

use crate::value::{Calendar, Numeric, lexical, string};
use oxrdf::Term;
use oxsdatatypes::Decimal;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dialect {
    /// `fn:format-number`: `Infinity`, and grouping separators exactly where the picture
    /// puts them unless they are evenly spaced.
    XPath,
    /// `java.text.DecimalFormat`: `∞`, and the last grouping interval repeated.
    Java,
}

/// `format-number(value, picture)` and `spif:decimalFormat(value, pattern)`.
pub(crate) fn format_number(a: &[Term], dialect: Dialect) -> Option<Term> {
    if a.len() < 2 || (dialect == Dialect::Java && a.len() > 2) || a.len() > 3 {
        return None;
    }
    if a.len() == 3 {
        // A named decimal format: none is defined here, so any name is an unknown one.
        return None;
    }
    let n = Numeric::of(a.first()?)?;
    let picture = lexical(a.get(1)?)?;
    string(number_with_picture(n, picture, dialect)?)
}

struct SubPicture {
    prefix: String,
    suffix: String,
    /// Mandatory and optional digit positions before the decimal separator.
    min_integer: usize,
    /// Distances of the grouping separators from the decimal separator, in digits.
    grouping: Vec<usize>,
    min_fraction: usize,
    max_fraction: usize,
    scale: i32,
}

fn is_digit_char(c: char) -> bool {
    c == '#' || c.is_ascii_digit()
}

fn is_active(c: char) -> bool {
    is_digit_char(c) || c == '.' || c == ','
}

fn sub_picture(p: &str) -> Option<SubPicture> {
    let chars: Vec<char> = p.chars().collect();
    let first = chars.iter().position(|c| is_active(*c))?;
    let last = chars.iter().rposition(|c| is_active(*c))?;
    let prefix: String = chars[..first].iter().collect();
    let suffix: String = chars[last + 1..].iter().collect();
    let body = &chars[first..=last];
    if body.iter().any(|c| !is_active(*c)) || body.iter().filter(|c| **c == '.').count() > 1 {
        return None;
    }
    let percent = prefix.contains('%') || suffix.contains('%');
    let mille = prefix.contains('‰') || suffix.contains('‰');
    if percent && mille {
        return None;
    }
    let dot = body.iter().position(|c| *c == '.');
    let (int_part, frac_part) = match dot {
        Some(d) => (&body[..d], &body[d + 1..]),
        None => (body, &body[body.len()..]),
    };
    if frac_part.contains(&',') {
        return None;
    }
    // An optional digit after a mandatory one in the integer part, or before one in the
    // fraction, is not a picture.
    let digits: Vec<char> = int_part
        .iter()
        .copied()
        .filter(|c| is_digit_char(*c))
        .collect();
    if digits
        .windows(2)
        .any(|w| w[0].is_ascii_digit() && w[1] == '#')
    {
        return None;
    }
    let mut min_integer = digits.iter().filter(|c| c.is_ascii_digit()).count();
    let mut grouping = Vec::new();
    let mut count = 0;
    for c in int_part.iter().rev() {
        if *c == ',' {
            if count == 0 {
                return None;
            }
            grouping.push(count);
        } else {
            count += 1;
        }
    }
    let min_fraction = frac_part.iter().filter(|c| c.is_ascii_digit()).count();
    let max_fraction = frac_part.len();
    if frac_part
        .windows(2)
        .any(|w| w[0] == '#' && w[1].is_ascii_digit())
    {
        return None;
    }
    if min_integer == 0 && max_fraction == 0 {
        min_integer = 1;
    }
    Some(SubPicture {
        prefix,
        suffix,
        min_integer,
        grouping,
        min_fraction,
        max_fraction,
        scale: if percent {
            2
        } else if mille {
            3
        } else {
            0
        },
    })
}

/// The decimal digits of a non-negative number: those before the point and after it.
fn digits_of(n: Numeric) -> Option<(String, String)> {
    let text = match n {
        Numeric::Integer(i) => i.to_string(),
        Numeric::Decimal(d) => d.to_string(),
        Numeric::Float(_) | Numeric::Double(_) => {
            let f = n.to_f64();
            if !f.is_finite() {
                return None;
            }
            format!("{f}")
        }
    };
    let text = text.trim_start_matches('-');
    Some(match text.split_once('.') {
        Some((i, f)) => (i.to_owned(), f.to_owned()),
        None => (text.to_owned(), String::new()),
    })
}

/// Moves the decimal point `by` places right (for per-cent and per-mille).
fn shift(int: &mut String, frac: &mut String, by: i32) {
    for _ in 0..by {
        let c = if frac.is_empty() { '0' } else { frac.remove(0) };
        int.push(c);
    }
}

/// Rounds the digit strings half to even at `places` decimal places.
pub(crate) fn round_half_even(int: &mut String, frac: &mut String, places: usize) {
    if frac.len() <= places {
        return;
    }
    let rest: Vec<u8> = frac.as_bytes()[places..].to_vec();
    frac.truncate(places);
    let first = rest[0];
    let beyond = rest[1..].iter().any(|b| *b != b'0');
    let last_kept = frac
        .as_bytes()
        .last()
        .or(int.as_bytes().last())
        .copied()
        .unwrap_or(b'0');
    let up = first > b'5' || (first == b'5' && (beyond || (last_kept - b'0') % 2 == 1));
    if !up {
        return;
    }
    let mut all: Vec<u8> = int.bytes().chain(frac.bytes()).collect();
    let mut i = all.len();
    loop {
        if i == 0 {
            all.insert(0, b'1');
            break;
        }
        i -= 1;
        if all[i] == b'9' {
            all[i] = b'0';
        } else {
            all[i] += 1;
            break;
        }
    }
    let split = all.len() - frac.len();
    *int = String::from_utf8_lossy(&all[..split]).into_owned();
    *frac = String::from_utf8_lossy(&all[split..]).into_owned();
}

pub(crate) fn number_with_picture(n: Numeric, picture: &str, dialect: Dialect) -> Option<String> {
    let mut parts = picture.split(';');
    let positive = sub_picture(parts.next()?)?;
    let negative = parts.next().map(sub_picture);
    if parts.next().is_some() {
        return None;
    }
    let negative = match negative {
        Some(Some(p)) => Some(p),
        Some(None) => return None,
        None => None,
    };
    let f = n.to_f64();
    if f.is_nan() {
        return Some("NaN".into());
    }
    let is_negative = f < 0.0
        || (f == 0.0
            && f.is_sign_negative()
            && matches!(n, Numeric::Double(_) | Numeric::Float(_)));
    let pic = match (&negative, is_negative) {
        (Some(neg), true) => neg,
        _ => &positive,
    };
    let minus = if is_negative && negative.is_none() {
        "-"
    } else {
        ""
    };
    if f.is_infinite() {
        let infinity = if dialect == Dialect::XPath {
            "Infinity"
        } else {
            "∞"
        };
        return Some(format!("{minus}{}{infinity}{}", pic.prefix, pic.suffix));
    }
    let scale = positive.scale.max(pic.scale);
    let (int, mut frac) = match n {
        // A double is rounded from its exact binary value, as XPath says it is: 0.35e0 is
        // 0.34999…, so "0.0" makes it 0.3. Rust's fixed-precision formatting rounds that
        // exact value correctly.
        Numeric::Float(_) | Numeric::Double(_) => {
            let x = f.abs() * 10_f64.powi(scale);
            let s = format!("{x:.prec$}", prec = pic.max_fraction);
            match s.split_once('.') {
                Some((i, f)) => (i.to_owned(), f.to_owned()),
                None => (s, String::new()),
            }
        }
        _ => {
            let (mut int, mut frac) = digits_of(n)?;
            shift(&mut int, &mut frac, scale);
            round_half_even(&mut int, &mut frac, pic.max_fraction);
            (int, frac)
        }
    };
    let int = int.trim_start_matches('0').to_owned();
    while frac.len() > pic.min_fraction && frac.ends_with('0') {
        frac.pop();
    }
    while frac.len() < pic.min_fraction {
        frac.push('0');
    }
    let mut int = if int.len() < pic.min_integer {
        format!("{}{int}", "0".repeat(pic.min_integer - int.len()))
    } else {
        int
    };
    if int.is_empty() && frac.is_empty() {
        int.push('0');
    }
    let grouped = group(&int, &pic.grouping, dialect);
    let body = if frac.is_empty() {
        grouped
    } else {
        format!("{grouped}.{frac}")
    };
    Some(format!("{minus}{}{body}{}", pic.prefix, pic.suffix))
}

fn group(int: &str, positions: &[usize], dialect: Dialect) -> String {
    if positions.is_empty() {
        return int.to_owned();
    }
    let first = positions[0];
    let regular = match dialect {
        Dialect::Java => true,
        Dialect::XPath => positions
            .iter()
            .enumerate()
            .all(|(i, p)| *p == first * (i + 1)),
    };
    let digits: Vec<char> = int.chars().collect();
    let mut out = Vec::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.iter().rev().enumerate() {
        let at = i;
        let separate = at > 0
            && if regular {
                at % first == 0
            } else {
                positions.contains(&at)
            };
        if separate {
            out.push(',');
        }
        out.push(*c);
    }
    out.into_iter().rev().collect()
}

// ---------------------------------------------------------------------------------------
// format-dateTime
// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Want {
    DateTime,
    Date,
    Time,
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// The day of the week, 1 for Monday to 7 for Sunday (ISO 8601).
pub(crate) fn weekday(year: i64, month: u8, day: u8) -> u8 {
    let days = days_from_civil(year, month, day);
    // 1970-01-01 was a Thursday.
    ((days + 3).rem_euclid(7) + 1) as u8
}

/// Days since 1970-01-01 in the proleptic Gregorian calendar (Howard Hinnant's algorithm).
pub(crate) fn days_from_civil(year: i64, month: u8, day: u8) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub(crate) fn day_of_year(year: i64, month: u8, day: u8) -> i64 {
    days_from_civil(year, month, day) - days_from_civil(year, 1, 1) + 1
}

/// The ISO 8601 week number.
fn iso_week(year: i64, month: u8, day: u8) -> i64 {
    let doy = day_of_year(year, month, day);
    let wd = i64::from(weekday(year, month, day));
    let week = (doy - wd + 10) / 7;
    if week < 1 {
        let prev = year - 1;
        let dec31 = day_of_year(prev, 12, 31);
        let wd31 = i64::from(weekday(prev, 12, 31));
        (dec31 - wd31 + 10) / 7
    } else if week == 53 {
        let wd_jan1_next = i64::from(weekday(year + 1, 1, 1));
        if wd_jan1_next <= 4 { 1 } else { 53 }
    } else {
        week
    }
}

pub(crate) fn format_date_time(a: &[Term], want: Want) -> Option<Term> {
    if a.len() != 2 && a.len() != 5 {
        return None;
    }
    let value = Calendar::of(a.first()?)?;
    let ok = matches!(
        (want, value),
        (Want::DateTime, Calendar::DateTime(_))
            | (Want::Date, Calendar::Date(_))
            | (Want::Time, Calendar::Time(_))
    );
    if !ok {
        return None;
    }
    if a.len() == 5 {
        // language, calendar, place: English, Gregorian and the value's own timezone are all
        // that is offered, so anything else asked for is refused.
        let language = lexical(&a[2])?;
        let calendar = lexical(&a[3])?;
        if !(language.is_empty() || language.starts_with("en"))
            || !(calendar.is_empty() || calendar == "AD" || calendar == "ISO")
        {
            return None;
        }
    }
    let picture = lexical(a.get(1)?)?;
    string(date_time_with_picture(value, picture)?)
}

fn date_time_with_picture(value: Calendar, picture: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = picture.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '[' if chars.peek() == Some(&'[') => {
                chars.next();
                out.push('[');
            }
            ']' if chars.peek() == Some(&']') => {
                chars.next();
                out.push(']');
            }
            ']' => return None,
            '[' => {
                let mut marker = String::new();
                loop {
                    match chars.next()? {
                        ']' => break,
                        c if c.is_whitespace() => {}
                        c => marker.push(c),
                    }
                }
                out.push_str(&component(value, &marker)?);
            }
            c => out.push(c),
        }
    }
    Some(out)
}

fn component(value: Calendar, marker: &str) -> Option<String> {
    let mut chars = marker.chars();
    let letter = chars.next()?;
    let rest: String = chars.collect();
    let (presentation, width) = match rest.split_once(',') {
        Some((p, w)) => (p.to_owned(), Some(w.to_owned())),
        None => (rest, None),
    };
    let (min_width, max_width) = match &width {
        None => (None, None),
        Some(w) => {
            let (lo, hi) = w
                .split_once('-')
                .map_or((w.as_str(), None), |(l, h)| (l, Some(h)));
            let parse = |s: &str| {
                if s == "*" {
                    Some(None)
                } else {
                    s.parse::<usize>().ok().map(Some)
                }
            };
            let max = match hi {
                None => None,
                Some(h) => parse(h)?,
            };
            (parse(lo)?, max)
        }
    };
    let ordinal = presentation.ends_with('o') && presentation.len() > 1;
    let presentation = presentation.trim_end_matches(['o', 't', 'c']).to_owned();

    let number = |n: i64, default: &str| -> Option<String> {
        let p = if presentation.is_empty() {
            default.to_owned()
        } else {
            presentation.clone()
        };
        let mut s = match p.as_str() {
            "I" => roman(n)?,
            "i" => roman(n)?.to_lowercase(),
            "W" | "w" | "Ww" | "N" | "n" | "Nn" => return None,
            digits
                if digits
                    .chars()
                    .all(|c| c == '0' || c == '1' || c == '#' || c == ',')
                    && digits.contains('1') =>
            {
                let min = digits.chars().filter(|c| c.is_ascii_digit()).count();
                let min = min_width.unwrap_or(0).max(min);
                let raw = n.unsigned_abs().to_string();
                let padded = if raw.len() < min {
                    format!("{}{raw}", "0".repeat(min - raw.len()))
                } else {
                    raw
                };
                if n < 0 { format!("-{padded}") } else { padded }
            }
            _ => return None,
        };
        if ordinal {
            s.push_str(match (n % 100, n % 10) {
                (11..=13, _) => "th",
                (_, 1) => "st",
                (_, 2) => "nd",
                (_, 3) => "rd",
                _ => "th",
            });
        }
        Some(s)
    };
    let name = |full: &str, default: &str| -> Option<String> {
        let p = if presentation.is_empty() {
            default
        } else {
            presentation.as_str()
        };
        let mut s: String = full.to_owned();
        if let Some(max) = max_width {
            s = s.chars().take(max.max(1)).collect();
        }
        Some(match p {
            "N" => s.to_uppercase(),
            "n" => s.to_lowercase(),
            "Nn" => s,
            _ => return None,
        })
    };

    match letter {
        'Y' => {
            let y = value.year()?;
            // A two-digit year picture, or a maximum width of 2, keeps the last two digits.
            let digits = presentation.chars().filter(|c| c.is_ascii_digit()).count();
            if max_width == Some(2) || (digits == 2 && max_width.is_none() && width.is_none()) {
                number(y.rem_euclid(100), "1")
                    .map(|s| if s.len() < 2 { format!("0{s}") } else { s })
            } else {
                number(y, "1")
            }
        }
        'M' => {
            let m = value.month()?;
            if presentation.starts_with('N') || presentation.starts_with('n') {
                name(MONTHS[usize::from(m) - 1], "Nn")
            } else {
                number(m.into(), "1")
            }
        }
        'D' => number(value.day()?.into(), "1"),
        'd' => number(
            day_of_year(value.year()?, value.month()?, value.day()?),
            "1",
        ),
        'F' => {
            let wd = weekday(value.year()?, value.month()?, value.day()?);
            if presentation.is_empty()
                || presentation.starts_with('N')
                || presentation.starts_with('n')
            {
                name(DAYS[usize::from(wd) - 1], "n")
            } else {
                number(wd.into(), "1")
            }
        }
        'W' => number(iso_week(value.year()?, value.month()?, value.day()?), "1"),
        'H' => number(value.hour()?.into(), "1"),
        'h' => {
            let h = value.hour()? % 12;
            number(if h == 0 { 12 } else { h.into() }, "1")
        }
        'P' => {
            let pm = value.hour()? >= 12;
            name(if pm { "pm" } else { "am" }, "n").map(|s| match presentation.as_str() {
                "N" => s.to_uppercase(),
                _ => s,
            })
        }
        'm' => number(value.minute()?.into(), "01"),
        's' => {
            let whole = value.second()?.checked_floor()?;
            number(whole.to_string().parse().ok()?, "01")
        }
        'f' => fraction_digits(value.second()?, &presentation),
        'Z' | 'z' => timezone(value, letter, &presentation),
        _ => None,
    }
}

fn fraction_digits(second: Decimal, presentation: &str) -> Option<String> {
    let text = second.to_string();
    let frac = text.split_once('.').map_or("", |(_, f)| f);
    let p = if presentation.is_empty() {
        "1"
    } else {
        presentation
    };
    if !p.chars().all(|c| c.is_ascii_digit() || c == '#') {
        return None;
    }
    let min = p.chars().filter(|c| c.is_ascii_digit()).count();
    let max = p.len();
    let mut int = String::new();
    let mut digits = frac.to_owned();
    if p.contains('#') || min > 1 {
        round_half_even(&mut int, &mut digits, max);
    }
    while digits.len() > min && digits.ends_with('0') {
        digits.pop();
    }
    while digits.len() < min {
        digits.push('0');
    }
    Some(digits)
}

fn timezone(value: Calendar, letter: char, presentation: &str) -> Option<String> {
    let Some(tz) = value.timezone() else {
        return Some(String::new());
    };
    let minutes = tz.hours() * 60 + tz.minutes();
    let sign = if minutes < 0 { '-' } else { '+' };
    let (h, m) = (minutes.abs() / 60, minutes.abs() % 60);
    if letter == 'z' {
        return Some(format!("GMT{sign}{h:02}:{m:02}"));
    }
    Some(match presentation {
        "" | "01:01" => format!("{sign}{h:02}:{m:02}"),
        "Z" if minutes == 0 => "Z".to_owned(),
        "Z" => format!("{sign}{h:02}:{m:02}"),
        "0" | "1" => {
            if m == 0 {
                format!("{sign}{h}")
            } else {
                format!("{sign}{h}:{m:02}")
            }
        }
        "00" | "01" => format!("{sign}{h:02}"),
        "0000" | "0100" => format!("{sign}{h:02}{m:02}"),
        "0:00" | "1:01" => format!("{sign}{h}:{m:02}"),
        _ => return None,
    })
}

fn roman(n: i64) -> Option<String> {
    if !(1..4000).contains(&n) {
        return None;
    }
    let table = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut n = n;
    let mut out = String::new();
    for (v, s) in table {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxsdatatypes::{Double, Integer};
    use std::str::FromStr;

    fn num(picture: &str, n: f64) -> Option<String> {
        number_with_picture(Numeric::Double(Double::from(n)), picture, Dialect::XPath)
    }

    #[test]
    fn numbers_take_their_pictures() {
        // F&O 3.1 §4.7.2 and its examples.
        assert_eq!(num("#,##0.00", 12345.678).as_deref(), Some("12,345.68"));
        assert_eq!(num("#,##0.##", 1234.5).as_deref(), Some("1,234.5"));
        assert_eq!(num("0000.0", 12.34567).as_deref(), Some("0012.3"));
        assert_eq!(num("#%", 0.256).as_deref(), Some("26%"));
        assert_eq!(num("#.##;(#.##)", -2.5).as_deref(), Some("(2.5)"));
        assert_eq!(num("#.00", -0.5).as_deref(), Some("-.50"));
        assert_eq!(num("0.0", 0.25).as_deref(), Some("0.2"), "half to even");
        assert_eq!(
            num("0.0", 0.35).as_deref(),
            Some("0.3"),
            "0.35 is 0.34999… as a double"
        );
        let integer = Numeric::Integer(Integer::from_str("1234567").unwrap());
        assert_eq!(
            number_with_picture(integer, "#,###", Dialect::XPath).as_deref(),
            Some("1,234,567")
        );
        assert_eq!(num("#", f64::INFINITY).as_deref(), Some("Infinity"));
        assert_eq!(num("#", f64::NAN).as_deref(), Some("NaN"));
        assert_eq!(num("##.#.#", 1.0), None, "two decimal separators");
    }

    #[test]
    fn digits_round_half_to_even() {
        let r = |i: &str, f: &str, p: usize| {
            let (mut i, mut f) = (i.to_owned(), f.to_owned());
            round_half_even(&mut i, &mut f, p);
            format!("{i}.{f}")
        };
        assert_eq!(r("2", "5", 0), "2.");
        assert_eq!(r("3", "5", 0), "4.");
        assert_eq!(r("9", "96", 1), "10.0");
        assert_eq!(r("0", "125", 2), "0.12");
        assert_eq!(r("0", "1251", 2), "0.13");
    }

    #[test]
    fn dates_take_their_pictures() {
        let dt = Calendar::DateTime(
            oxsdatatypes::DateTime::from_str("2026-08-25T13:05:07.5+01:00").unwrap(),
        );
        let f = |p: &str| date_time_with_picture(dt, p);
        assert_eq!(f("[Y0001]-[M01]-[D01]").as_deref(), Some("2026-08-25"));
        assert_eq!(f("[D1o] [MNn] [Y]").as_deref(), Some("25th August 2026"));
        assert_eq!(
            f("[FNn], [D] [MNn,3-3]").as_deref(),
            Some("Tuesday, 25 Aug")
        );
        assert_eq!(f("[h]:[m01] [PN]").as_deref(), Some("1:05 PM"));
        assert_eq!(
            f("[H01]:[m01]:[s01].[f001] [Z]").as_deref(),
            Some("13:05:07.500 +01:00")
        );
        assert_eq!(f("[Y01]").as_deref(), Some("26"));
        assert_eq!(f("[[[Y]]]").as_deref(), Some("[2026]"));
        assert_eq!(f("[d]").as_deref(), Some("237"));
        assert_eq!(f("[W]").as_deref(), Some("35"));
        assert_eq!(f("[q]"), None, "not a component");
    }

    #[test]
    fn weekdays_are_iso() {
        assert_eq!(weekday(1970, 1, 1), 4);
        assert_eq!(weekday(2026, 11, 21), 6);
        assert_eq!(weekday(2000, 2, 29), 2);
    }
}
