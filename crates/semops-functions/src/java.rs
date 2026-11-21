//! The Java formats that Jena's and TopBraid's functions are defined by: `SimpleDateFormat`
//! patterns (`spif:parseDate`, `spif:dateFormat`) and `String.format` (`afn:sprintf`).
//!
//! Both are implemented for what their documentation defines without a locale: English
//! month and day names, the Gregorian calendar, ASCII digits. A pattern or format asking for
//! anything else (a named time zone such as `PST`, `%tY` date conversions) is an error, as an
//! exception thrown in Java is an error in a query.
//!
//! Two choices differ from a JVM's, because a JVM's depend on its configuration:
//!
//! - A value with no timezone is formatted as it is written, not first moved into the
//!   machine's default zone, and `z`, `Z` and `X` read its timezone as UTC.
//! - A two-digit year (`yy`) is read as Java reads it, into the century that puts it within
//!   80 years before and 20 years after the current year.

use crate::format::{day_of_year, weekday};
use crate::value::{Calendar, Numeric};
use oxrdf::{Literal, Term};
use oxsdatatypes::{Date, DateTime, Time};
use std::str::FromStr;

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

#[derive(Debug, PartialEq, Eq)]
enum Token {
    Field(char, usize),
    Literal(String),
}

fn tokens(pattern: &str) -> Option<Vec<Token>> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut literal = String::new();
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            if chars.get(i + 1) == Some(&'\'') {
                literal.push('\'');
                i += 2;
                continue;
            }
            // quoted text, with '' inside it a quote
            i += 1;
            loop {
                match chars.get(i) {
                    None => return None,
                    Some('\'') if chars.get(i + 1) == Some(&'\'') => {
                        literal.push('\'');
                        i += 2;
                    }
                    Some('\'') => {
                        i += 1;
                        break;
                    }
                    Some(c) => {
                        literal.push(*c);
                        i += 1;
                    }
                }
            }
        } else if c.is_ascii_alphabetic() {
            if !literal.is_empty() {
                out.push(Token::Literal(std::mem::take(&mut literal)));
            }
            let mut n = 1;
            while chars.get(i + n) == Some(&c) {
                n += 1;
            }
            out.push(Token::Field(c, n));
            i += n;
        } else {
            literal.push(c);
            i += 1;
        }
    }
    if !literal.is_empty() {
        out.push(Token::Literal(literal));
    }
    Some(out)
}

fn pad(n: i64, width: usize) -> String {
    let raw = n.unsigned_abs().to_string();
    let padded = if raw.len() < width {
        format!("{}{raw}", "0".repeat(width - raw.len()))
    } else {
        raw
    };
    if n < 0 { format!("-{padded}") } else { padded }
}

/// `SimpleDateFormat.format`.
pub(crate) fn format_date(value: Calendar, pattern: &str) -> Option<String> {
    let year = value.year().unwrap_or(1970);
    let month = value.month().unwrap_or(1);
    let day = value.day().unwrap_or(1);
    let hour = i64::from(value.hour().unwrap_or(0));
    let minute = i64::from(value.minute().unwrap_or(0));
    let second = value.second().unwrap_or_default();
    let whole_second: i64 = second.checked_floor()?.to_string().parse().ok()?;
    let millis: i64 = {
        let frac = second
            .to_string()
            .split_once('.')
            .map_or(String::new(), |(_, f)| f.to_owned());
        let mut f: String = frac.chars().take(3).collect();
        while f.len() < 3 {
            f.push('0');
        }
        f.parse().ok()?
    };
    let offset_minutes = value
        .timezone()
        .map_or(0, |tz| tz.hours() * 60 + tz.minutes());
    let mut out = String::new();
    for token in tokens(pattern)? {
        match token {
            Token::Literal(s) => out.push_str(&s),
            Token::Field(c, n) => out.push_str(&match c {
                'G' => if year > 0 { "AD" } else { "BC" }.to_owned(),
                'y' => {
                    if n == 2 {
                        pad(year.rem_euclid(100), 2)
                    } else {
                        pad(year, n)
                    }
                }
                'M' | 'L' => match n {
                    1 | 2 => pad(month.into(), n),
                    3 => MONTHS[usize::from(month) - 1][..3].to_owned(),
                    _ => MONTHS[usize::from(month) - 1].to_owned(),
                },
                'd' => pad(day.into(), n),
                'D' => pad(day_of_year(year, month, day), n),
                'E' => {
                    let name = DAYS[usize::from(weekday(year, month, day)) - 1];
                    if n >= 4 {
                        name.to_owned()
                    } else {
                        name[..3].to_owned()
                    }
                }
                'u' => pad(weekday(year, month, day).into(), n),
                'a' => if hour >= 12 { "PM" } else { "AM" }.to_owned(),
                'H' => pad(hour, n),
                'k' => pad(if hour == 0 { 24 } else { hour }, n),
                'K' => pad(hour % 12, n),
                'h' => pad(if hour % 12 == 0 { 12 } else { hour % 12 }, n),
                'm' => pad(minute, n),
                's' => pad(whole_second, n),
                'S' => pad(millis, n),
                'z' => {
                    if offset_minutes == 0 {
                        "GMT".to_owned()
                    } else {
                        format!("GMT{}", offset_text(offset_minutes, true))
                    }
                }
                'Z' => offset_text(offset_minutes, false),
                'X' => {
                    if offset_minutes == 0 {
                        "Z".to_owned()
                    } else {
                        match n {
                            1 => {
                                let s = offset_text(offset_minutes, false);
                                s[..3].to_owned()
                            }
                            2 => offset_text(offset_minutes, false),
                            3 => offset_text(offset_minutes, true),
                            _ => return None,
                        }
                    }
                }
                _ => return None,
            }),
        }
    }
    Some(out)
}

fn offset_text(minutes: i64, colon: bool) -> String {
    let sign = if minutes < 0 { '-' } else { '+' };
    let (h, m) = (minutes.abs() / 60, minutes.abs() % 60);
    if colon {
        format!("{sign}{h:02}:{m:02}")
    } else {
        format!("{sign}{h:02}{m:02}")
    }
}

fn is_numeric_field(t: &Token) -> bool {
    matches!(t, Token::Field(c, n) if matches!(c, 'y' | 'd' | 'D' | 'H' | 'k' | 'K' | 'h' | 'm' | 's' | 'S' | 'u')
        || (matches!(c, 'M' | 'L') && *n <= 2))
}

/// `SimpleDateFormat.parse`, as `spif:parseDate` uses it: the literal it gives is an
/// `xsd:date`, `xsd:time` or `xsd:dateTime` by which fields the pattern set, with a timezone
/// only if the pattern read one. Text after what the pattern reads is ignored, as Java's
/// `parse(String)` ignores it.
pub(crate) fn parse_date(input: &str, pattern: &str, current_year: i64) -> Option<Term> {
    let tokens = tokens(pattern)?;
    let chars: Vec<char> = input.chars().collect();
    let mut at = 0;
    let (mut year, mut month, mut day, mut day_of_year_value) =
        (None::<i64>, None::<i64>, None::<i64>, None::<i64>);
    let (mut hour24, mut hour12, mut pm, mut minute, mut second, mut millis) = (
        None::<i64>,
        None::<i64>,
        None::<bool>,
        None::<i64>,
        None::<i64>,
        None::<(i64, usize)>,
    );
    let mut offset = None::<i64>;
    let mut bc = false;

    let read_number = |at: &mut usize, width: Option<usize>| -> Option<(i64, usize)> {
        let start = *at;
        let negative = chars.get(*at) == Some(&'-') && width.is_none();
        if negative {
            *at += 1;
        }
        let digits_start = *at;
        while *at < chars.len()
            && chars[*at].is_ascii_digit()
            && width.is_none_or(|w| *at - digits_start < w)
        {
            *at += 1;
        }
        if *at == digits_start {
            *at = start;
            return None;
        }
        let text: String = chars[digits_start..*at].iter().collect();
        let n: i64 = text.parse().ok()?;
        Some((if negative { -n } else { n }, *at - digits_start))
    };
    let read_name = |at: &mut usize, names: &[&str]| -> Option<usize> {
        let rest: String = chars[*at..].iter().collect::<String>().to_lowercase();
        // full names first, so "May" does not stop at its three letters needlessly
        for (i, name) in names.iter().enumerate() {
            if rest.starts_with(&name.to_lowercase()) {
                *at += name.chars().count();
                return Some(i);
            }
        }
        for (i, name) in names.iter().enumerate() {
            if rest.starts_with(&name[..3].to_lowercase()) {
                *at += 3;
                return Some(i);
            }
        }
        None
    };

    for (i, token) in tokens.iter().enumerate() {
        let abutting = tokens.get(i + 1).is_some_and(is_numeric_field) && is_numeric_field(token);
        match token {
            Token::Literal(s) => {
                for expected in s.chars() {
                    if chars.get(at) != Some(&expected) {
                        return None;
                    }
                    at += 1;
                }
            }
            Token::Field(c, n) => {
                let width = abutting.then_some(*n);
                match c {
                    'y' => {
                        let (v, digits) = read_number(&mut at, width)?;
                        year = Some(if *n <= 2 && digits == 2 && v >= 0 {
                            // the century that puts it within 80 years before, 20 after
                            let pivot = current_year - 80;
                            let century = pivot.div_euclid(100) * 100;
                            let candidate = century + v;
                            if candidate < pivot {
                                candidate + 100
                            } else {
                                candidate
                            }
                        } else {
                            v
                        });
                    }
                    'M' | 'L' if *n >= 3 => month = Some(read_name(&mut at, &MONTHS)? as i64 + 1),
                    'M' | 'L' => month = Some(read_number(&mut at, width)?.0),
                    'd' => day = Some(read_number(&mut at, width)?.0),
                    'D' => day_of_year_value = Some(read_number(&mut at, width)?.0),
                    'E' => {
                        read_name(&mut at, &DAYS)?;
                    }
                    'u' => {
                        read_number(&mut at, width)?;
                    }
                    'G' => {
                        let rest: String = chars[at..].iter().take(2).collect();
                        match rest.to_ascii_uppercase().as_str() {
                            "AD" => {}
                            "BC" => bc = true,
                            _ => return None,
                        }
                        at += 2;
                    }
                    'a' => {
                        let rest: String = chars[at..].iter().take(2).collect();
                        pm = Some(match rest.to_ascii_uppercase().as_str() {
                            "AM" => false,
                            "PM" => true,
                            _ => return None,
                        });
                        at += 2;
                    }
                    'H' => hour24 = Some(read_number(&mut at, width)?.0),
                    'k' => hour24 = Some(read_number(&mut at, width)?.0 % 24),
                    'K' => hour12 = Some(read_number(&mut at, width)?.0),
                    'h' => hour12 = Some(read_number(&mut at, width)?.0 % 12),
                    'm' => minute = Some(read_number(&mut at, width)?.0),
                    's' => second = Some(read_number(&mut at, width)?.0),
                    'S' => millis = Some(read_number(&mut at, width)?),
                    'z' | 'Z' | 'X' => offset = Some(read_offset(&chars, &mut at)?),
                    _ => return None,
                }
            }
        }
    }

    let date_set =
        year.is_some() || month.is_some() || day.is_some() || day_of_year_value.is_some();
    let time_set = hour24.is_some()
        || hour12.is_some()
        || minute.is_some()
        || second.is_some()
        || millis.is_some()
        || pm.is_some();
    let mut y = year.unwrap_or(1970);
    if bc {
        y = 1 - y;
    }
    let (m, d) = match (day_of_year_value, month, day) {
        (Some(doy), None, None) => month_day_of(y, doy)?,
        _ => (month.unwrap_or(1), day.unwrap_or(1)),
    };
    let h = match (hour24, hour12, pm) {
        (Some(h), _, _) => h,
        (None, Some(h), Some(true)) => h + 12,
        (None, Some(h), _) => h,
        (None, None, Some(true)) => 12,
        (None, None, _) => 0,
    };
    let seconds = match millis {
        None => pad(second.unwrap_or(0), 2),
        Some((ms, _)) => format!("{}.{}", pad(second.unwrap_or(0), 2), pad(ms, 3)),
    };
    let tz = match offset {
        None => String::new(),
        Some(0) => "Z".to_owned(),
        Some(o) => offset_text(o, true),
    };
    let date = format!("{}-{}-{}", pad(y, 4), pad(m, 2), pad(d, 2));
    let time = format!("{}:{}:{seconds}", pad(h, 2), pad(minute.unwrap_or(0), 2));
    Some(match (date_set, time_set) {
        (true, false) => Literal::from(Date::from_str(&format!("{date}{tz}")).ok()?).into(),
        (false, true) => Literal::from(Time::from_str(&format!("{time}{tz}")).ok()?).into(),
        _ => Literal::from(DateTime::from_str(&format!("{date}T{time}{tz}")).ok()?).into(),
    })
}

fn month_day_of(year: i64, doy: i64) -> Option<(i64, i64)> {
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let lengths = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut left = doy;
    for (i, len) in lengths.iter().enumerate() {
        if left <= *len {
            return (left >= 1).then_some((i as i64 + 1, left));
        }
        left -= len;
    }
    None
}

/// `Z`, `GMT`, `UTC`, `GMT+01:00`, `+0100`, `+01:00` or `+01`, as minutes east of UTC.
fn read_offset(chars: &[char], at: &mut usize) -> Option<i64> {
    let rest: String = chars[*at..].iter().collect();
    if let Some(after) = rest
        .strip_prefix("GMT")
        .or_else(|| rest.strip_prefix("UTC"))
    {
        *at += 3;
        if !after.starts_with(['+', '-']) {
            return Some(0);
        }
    } else if rest.starts_with('Z') {
        *at += 1;
        return Some(0);
    }
    let sign = match chars.get(*at)? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    *at += 1;
    let digits = |at: &mut usize, n: usize| -> Option<i64> {
        let s: String = chars.get(*at..*at + n)?.iter().collect();
        if !s.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        *at += n;
        s.parse().ok()
    };
    let h = digits(at, 2)?;
    if chars.get(*at) == Some(&':') {
        *at += 1;
    }
    let m = digits(at, 2).unwrap_or(0);
    Some(sign * (h * 60 + m))
}

// ---------------------------------------------------------------------------------------
// String.format
// ---------------------------------------------------------------------------------------

/// `String.format(format, args…)` for the conversions that do not need a locale or a Java
/// object of a particular class: `%s %S %d %o %x %X %f %e %E %g %G %b %B %c %C %% %n`, with
/// argument indexes, the flags `-#+ 0,(`, widths and precisions. An integer formats with
/// `%d`, `%o` and `%x` (signed, as Java's `BigInteger` does) and a decimal or double with
/// `%f`, `%e` and `%g`; the wrong one is an error, as Java's
/// `IllegalFormatConversionException` is.
pub(crate) fn sprintf(format: &str, args: &[Term]) -> Option<String> {
    let chars: Vec<char> = format.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut next_arg = 0;
    let mut last_arg = None::<usize>;
    while i < chars.len() {
        if chars[i] != '%' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        i += 1;
        // argument index: digits followed by '$', or '<' for the previous one
        let mut index = None;
        let save = i;
        let mut digits = String::new();
        while i < chars.len() && chars[i].is_ascii_digit() {
            digits.push(chars[i]);
            i += 1;
        }
        if chars.get(i) == Some(&'$') && !digits.is_empty() {
            index = Some(digits.parse::<usize>().ok()?.checked_sub(1)?);
            i += 1;
        } else {
            i = save;
        }
        if chars.get(i) == Some(&'<') {
            index = Some(last_arg?);
            i += 1;
        }
        let mut flags = String::new();
        while i < chars.len() && "-#+ 0,(".contains(chars[i]) {
            flags.push(chars[i]);
            i += 1;
        }
        let mut width = String::new();
        while i < chars.len() && chars[i].is_ascii_digit() {
            width.push(chars[i]);
            i += 1;
        }
        let width: Option<usize> = if width.is_empty() {
            None
        } else {
            Some(width.parse().ok()?)
        };
        let precision: Option<usize> = if chars.get(i) == Some(&'.') {
            i += 1;
            let mut p = String::new();
            while i < chars.len() && chars[i].is_ascii_digit() {
                p.push(chars[i]);
                i += 1;
            }
            Some(p.parse().ok()?)
        } else {
            None
        };
        let conversion = *chars.get(i)?;
        i += 1;
        match conversion {
            '%' => {
                out.push_str(&justify("%".into(), width, &flags));
                continue;
            }
            'n' => {
                out.push('\n');
                continue;
            }
            _ => {}
        }
        let at = match index {
            Some(n) => n,
            None => {
                let n = next_arg;
                next_arg += 1;
                n
            }
        };
        last_arg = Some(at);
        let arg = args.get(at)?;
        let body = match conversion {
            's' | 'S' => {
                let mut s = match arg {
                    Term::Literal(l) => l.value().to_owned(),
                    Term::NamedNode(n) => n.as_str().to_owned(),
                    Term::BlankNode(b) => b.as_str().to_owned(),
                    #[allow(unreachable_patterns)]
                    _ => return None,
                };
                if let Some(p) = precision {
                    s = s.chars().take(p).collect();
                }
                if conversion == 'S' {
                    s.to_uppercase()
                } else {
                    s
                }
            }
            'b' | 'B' => {
                let b = match arg {
                    Term::Literal(l) if l.datatype() == oxrdf::vocab::xsd::BOOLEAN => {
                        crate::value::boolean(arg)?
                    }
                    _ => true,
                };
                let s = b.to_string();
                if conversion == 'B' {
                    s.to_uppercase()
                } else {
                    s
                }
            }
            'c' | 'C' => {
                let c = match Numeric::of(arg) {
                    Some(Numeric::Integer(i)) => {
                        char::from_u32(u32::try_from(i64::from_str(&i.to_string()).ok()?).ok()?)?
                    }
                    _ => {
                        let Term::Literal(l) = arg else { return None };
                        let mut cs = l.value().chars();
                        let c = cs.next()?;
                        if cs.next().is_some() {
                            return None;
                        }
                        c
                    }
                };
                let s = c.to_string();
                if conversion == 'C' {
                    s.to_uppercase()
                } else {
                    s
                }
            }
            'd' | 'o' | 'x' | 'X' => {
                let Some(Numeric::Integer(n)) = Numeric::of(arg).filter(|_| is_typed_number(arg))
                else {
                    return None;
                };
                let n: i128 = n.to_string().parse().ok()?;
                let digits = match conversion {
                    'd' => n.unsigned_abs().to_string(),
                    'o' => format!("{:o}", n.unsigned_abs()),
                    'x' => format!("{:x}", n.unsigned_abs()),
                    _ => format!("{:X}", n.unsigned_abs()),
                };
                let digits = if conversion == 'd' && flags.contains(',') {
                    group_thousands(&digits)
                } else {
                    digits
                };
                let prefix = match (flags.contains('#'), conversion) {
                    (true, 'o') => "0",
                    (true, 'x') => "0x",
                    (true, 'X') => "0X",
                    _ => "",
                };
                signed(n < 0, format!("{prefix}{digits}"), width, &flags)?
            }
            'f' | 'e' | 'E' | 'g' | 'G' => {
                let n = Numeric::of(arg).filter(|_| is_typed_number(arg))?;
                if matches!(n, Numeric::Integer(_)) {
                    return None;
                }
                let x = n.to_f64();
                if x.is_nan() {
                    justify("NaN".into(), width, &flags)
                } else if x.is_infinite() {
                    let s = if x > 0.0 {
                        if flags.contains('+') {
                            "+Infinity"
                        } else {
                            "Infinity"
                        }
                    } else if flags.contains('(') {
                        "(Infinity)"
                    } else {
                        "-Infinity"
                    };
                    justify(s.into(), width, &flags)
                } else {
                    let p = precision.unwrap_or(6);
                    let magnitude = x.abs();
                    let body = match conversion {
                        'f' => fixed(n, p, flags.contains(','))?,
                        'e' | 'E' => scientific(magnitude, p, conversion == 'E'),
                        _ => {
                            let p = if p == 0 { 1 } else { p };
                            let rounded: f64 = format!("{:.*e}", p - 1, magnitude).parse().ok()?;
                            if rounded != 0.0
                                && (rounded < 1e-4 || rounded >= 10_f64.powi(p as i32))
                            {
                                scientific(magnitude, p - 1, conversion == 'G')
                            } else {
                                let exponent = if rounded == 0.0 {
                                    0
                                } else {
                                    rounded.log10().floor() as i32
                                };
                                let decimals = (p as i32 - 1 - exponent).max(0) as usize;
                                format!("{magnitude:.decimals$}")
                            }
                        }
                    };
                    signed(x < 0.0, body, width, &flags)?
                }
            }
            _ => return None,
        };
        out.push_str(&body);
    }
    Some(out)
}

/// Java formats only numbers with the numeric conversions: a plain string that spells one
/// is still a `String`.
fn is_typed_number(t: &Term) -> bool {
    matches!(t, Term::Literal(l) if l.datatype() != oxrdf::vocab::xsd::STRING)
}

fn fixed(n: Numeric, precision: usize, grouping: bool) -> Option<String> {
    // Exactly, half up, as Java's Formatter rounds (RoundingMode.HALF_UP on the decimal
    // value): through the decimal digits rather than through f64 formatting, which rounds
    // the binary value.
    let text = match n {
        Numeric::Decimal(d) => d.to_string(),
        _ => format!("{}", n.to_f64()),
    };
    let text = text.trim_start_matches('-');
    let (int, frac) = text.split_once('.').unwrap_or((text, ""));
    let (mut int, mut frac) = (int.to_owned(), frac.to_owned());
    if frac.len() > precision {
        let up = frac.as_bytes()[precision] >= b'5';
        frac.truncate(precision);
        if up {
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
            int = String::from_utf8_lossy(&all[..split]).into_owned();
            frac = String::from_utf8_lossy(&all[split..]).into_owned();
        }
    }
    while frac.len() < precision {
        frac.push('0');
    }
    let int = if grouping { group_thousands(&int) } else { int };
    Some(if precision == 0 {
        int
    } else {
        format!("{int}.{frac}")
    })
}

/// Java's `%e`: one digit, the fraction, and an exponent of at least two digits.
fn scientific(x: f64, precision: usize, upper: bool) -> String {
    let s = format!("{x:.precision$e}");
    let (mantissa, exponent) = s.split_once('e').unwrap_or((&s, "0"));
    let e: i32 = exponent.parse().unwrap_or(0);
    let out = format!("{mantissa}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs());
    if upper { out.to_uppercase() } else { out }
}

fn group_thousands(digits: &str) -> String {
    let chars: Vec<char> = digits.chars().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*c);
    }
    out
}

fn signed(negative: bool, body: String, width: Option<usize>, flags: &str) -> Option<String> {
    let (lead, trail) = if negative {
        if flags.contains('(') {
            ("(", ")")
        } else {
            ("-", "")
        }
    } else if flags.contains('+') {
        ("+", "")
    } else if flags.contains(' ') {
        (" ", "")
    } else {
        ("", "")
    };
    if flags.contains('0') {
        let w = width?;
        let used = lead.chars().count() + body.chars().count() + trail.chars().count();
        let zeros = "0".repeat(w.saturating_sub(used));
        return Some(format!("{lead}{zeros}{body}{trail}"));
    }
    Some(justify(format!("{lead}{body}{trail}"), width, flags))
}

fn justify(s: String, width: Option<usize>, flags: &str) -> String {
    let Some(w) = width else { return s };
    let n = s.chars().count();
    if n >= w {
        return s;
    }
    let fill = " ".repeat(w - n);
    if flags.contains('-') {
        format!("{s}{fill}")
    } else {
        format!("{fill}{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxrdf::vocab::xsd;

    fn lit(v: &str) -> Term {
        Literal::new_simple_literal(v).into()
    }
    fn int(v: i64) -> Term {
        Literal::from(v).into()
    }
    fn dbl(v: &str) -> Term {
        Literal::new_typed_literal(v, xsd::DOUBLE).into()
    }
    fn dec(v: &str) -> Term {
        Literal::new_typed_literal(v, xsd::DECIMAL).into()
    }

    #[test]
    fn sprintf_formats_as_java_does() {
        // Checked against java.util.Formatter's documentation and a JDK 21 run.
        assert_eq!(
            sprintf("%s-%d", &[lit("a"), int(1)]).as_deref(),
            Some("a-1")
        );
        assert_eq!(
            sprintf("%05d|%-4d|%+d", &[int(42), int(7), int(3)]).as_deref(),
            Some("00042|7   |+3")
        );
        assert_eq!(
            sprintf("%,d", &[int(1234567)]).as_deref(),
            Some("1,234,567")
        );
        assert_eq!(
            sprintf("%x %X %o %#x", &[int(255), int(255), int(8), int(255)]).as_deref(),
            Some("ff FF 10 0xff")
        );
        assert_eq!(sprintf("%.2f", &[dbl("3.14159")]).as_deref(), Some("3.14"));
        assert_eq!(
            sprintf("%.1f", &[dec("0.25")]).as_deref(),
            Some("0.3"),
            "half up"
        );
        assert_eq!(
            sprintf("%8.3f|", &[dbl("-2.5")]).as_deref(),
            Some("  -2.500|")
        );
        assert_eq!(
            sprintf("%e", &[dbl("12345.678")]).as_deref(),
            Some("1.234568e+04")
        );
        assert_eq!(sprintf("%.3S", &[lit("hello")]).as_deref(), Some("HEL"));
        assert_eq!(
            sprintf("%2$s %1$s %<s", &[lit("a"), lit("b")]).as_deref(),
            Some("b a a")
        );
        assert_eq!(sprintf("100%%", &[]).as_deref(), Some("100%"));
        assert_eq!(sprintf("%(d", &[int(-5)]).as_deref(), Some("(5)"));
        // errors, as Java's exceptions are
        assert_eq!(
            sprintf("%d", &[dbl("1.5")]),
            None,
            "IllegalFormatConversion"
        );
        assert_eq!(sprintf("%f", &[int(1)]), None, "IllegalFormatConversion");
        assert_eq!(sprintf("%s %s", &[lit("a")]), None, "MissingFormatArgument");
        assert_eq!(sprintf("%q", &[lit("a")]), None, "UnknownFormatConversion");
        assert_eq!(
            sprintf("%d", &[lit("12")]),
            None,
            "a string is not a number"
        );
    }

    #[test]
    fn dates_parse_as_simple_date_format_parses_them() {
        let p =
            |input: &str, pattern: &str| parse_date(input, pattern, 2026).map(|t| t.to_string());
        let date = |v: &str| format!("\"{v}\"^^<http://www.w3.org/2001/XMLSchema#date>");
        let date_time = |v: &str| format!("\"{v}\"^^<http://www.w3.org/2001/XMLSchema#dateTime>");
        let time = |v: &str| format!("\"{v}\"^^<http://www.w3.org/2001/XMLSchema#time>");
        assert_eq!(p("25/08/2026", "dd/MM/yyyy"), Some(date("2026-08-25")));
        assert_eq!(p("Aug 5, 2026", "MMM d, yyyy"), Some(date("2026-08-05")));
        assert_eq!(
            p("Tuesday, 25 August 2026", "EEEE, d MMMM yyyy"),
            Some(date("2026-08-25"))
        );
        assert_eq!(
            p("20260825", "yyyyMMdd"),
            Some(date("2026-08-25")),
            "abutting fields take their widths"
        );
        assert_eq!(
            p("2026-08-25 13:05", "yyyy-MM-dd HH:mm"),
            Some(date_time("2026-08-25T13:05:00"))
        );
        assert_eq!(p("1:05 PM", "h:mm a"), Some(time("13:05:00")));
        assert_eq!(
            p("2026-08-25T13:05:07.250+0100", "yyyy-MM-dd'T'HH:mm:ss.SSSZ"),
            Some(date_time("2026-08-25T13:05:07.25+01:00"))
        );
        assert_eq!(
            p("12/31/99", "MM/dd/yy"),
            Some(date("1999-12-31")),
            "within 80 years before"
        );
        assert_eq!(
            p("01/01/30", "MM/dd/yy"),
            Some(date("2030-01-01")),
            "within 20 years after"
        );
        assert_eq!(
            p("2026-08-25 and more", "yyyy-MM-dd"),
            Some(date("2026-08-25")),
            "trailing text is ignored"
        );
        assert_eq!(p("25.08.2026", "dd/MM/yyyy"), None);
        assert_eq!(p("2026-13-01", "yyyy-MM-dd"), None, "no month 13");
    }

    #[test]
    fn dates_format_as_simple_date_format_formats_them() {
        let dt = Calendar::DateTime(DateTime::from_str("2026-08-05T13:05:07.25+01:00").unwrap());
        let f = |p: &str| format_date(dt, p);
        assert_eq!(f("yyyy-MM-dd").as_deref(), Some("2026-08-05"));
        assert_eq!(f("EEE, d MMM yy").as_deref(), Some("Wed, 5 Aug 26"));
        assert_eq!(
            f("EEEE d MMMM yyyy").as_deref(),
            Some("Wednesday 5 August 2026")
        );
        assert_eq!(f("hh:mm a").as_deref(), Some("01:05 PM"));
        assert_eq!(f("HH:mm:ss.SSS Z").as_deref(), Some("13:05:07.250 +0100"));
        assert_eq!(
            f("'Quarter' yyyy, 'o''clock'").as_deref(),
            Some("Quarter 2026, o'clock")
        );
        assert_eq!(f("XXX z").as_deref(), Some("+01:00 GMT+01:00"));
        assert_eq!(f("yyyy-MM-dd Q"), None, "Q is not a pattern letter");
    }
}
