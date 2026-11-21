//! Apache Jena ARQ's function library, `afn:`.
//!
//! Each function does what Jena 4.8's does (`org.apache.jena.sparql.function.library`): tarka
//! checked these against TARQL 1.3, which runs on it. Where HOLOS's own copies had followed
//! other rules, Jena's win, since the namespace is Jena's: `afn:localname` splits an IRI
//! before an XML name, not at the last `#` or `/`; `afn:substr` is Java's `substring` and an
//! index past the end is an error; `afn:min` and `afn:max` give back the argument as it was
//! given rather than a double; `afn:sprintf` formats as Java's `String.format` does.

use crate::value::{Numeric, double_out, lexical, number, split_xml, string};
use crate::{Function, Now, Plain, java, plain};
use oxrdf::{Literal, Term};
use sha1::{Digest, Sha1};
use std::sync::Arc;

/// Jena's namespaces for its functions: the current one, the one it had before it moved to
/// Apache, and the Java package that loads them directly.
pub const AFN_NAMESPACES: [&str; 3] = [
    "http://jena.apache.org/ARQ/function#",
    "http://jena.hpl.hp.com/ARQ/function#",
    "java:org.apache.jena.sparql.function.library.",
];

pub(crate) fn add(out: &mut Vec<(String, Function)>, now: &Now) {
    let afns: &[(&str, Plain)] = &[
        ("localname", |a| iri_part(a, false)),
        ("namespace", |a| iri_part(a, true)),
        ("substr", substr),
        ("substring", substr),
        ("strjoin", strjoin),
        ("sprintf", |a| {
            let (format, args) = a.split_first()?;
            string(java::sprintf(lexical(format)?, args)?)
        }),
        ("sha1sum", sha1sum),
        ("min", |a| pick(a, |x, y| x <= y)),
        ("max", |a| pick(a, |x, y| x >= y)),
        ("pi", |a| {
            if a.is_empty() {
                double_out(std::f64::consts::PI)
            } else {
                None
            }
        }),
        ("e", |a| {
            if a.is_empty() {
                double_out(std::f64::consts::E)
            } else {
                None
            }
        }),
        ("sqrt", |a| match a {
            [x] => double_out(number(x)?.sqrt()),
            _ => None,
        }),
        ("bnode", |a| match a {
            // The label the evaluator gives the blank node: the same within one set of
            // results, and nothing more.
            [Term::BlankNode(b)] => string(b.as_str()),
            _ => None,
        }),
    ];
    let at: Term = Literal::from(now.date_time).into();
    for ns in AFN_NAMESPACES {
        for (name, f) in afns {
            out.push((format!("{ns}{name}"), plain(*f)));
        }
        // the time the query started, as Jena has it
        let at = at.clone();
        out.push((
            format!("{ns}now"),
            Arc::new(move |a: &[Term]| a.is_empty().then(|| at.clone())),
        ));
    }
}

/// `afn:localname` and `afn:namespace`: the parts of an IRI either side of where Jena splits
/// it, as strings. Anything but an IRI is an error.
fn iri_part(args: &[Term], namespace: bool) -> Option<Term> {
    let [Term::NamedNode(iri)] = args else {
        return None;
    };
    let (ns, local) = iri.as_str().split_at(split_xml(iri.as_str()));
    string(if namespace { ns } else { local })
}

/// `afn:strjoin(separator, string …)`: the lexical forms joined.
fn strjoin(args: &[Term]) -> Option<Term> {
    let (separator, strings) = args.split_first()?;
    let parts: Option<Vec<&str>> = strings.iter().map(lexical).collect();
    string(parts?.join(lexical(separator)?))
}

/// `afn:substr(string, start [, end])`: Java's `substring`, counting characters from 0.
fn substr(args: &[Term]) -> Option<Term> {
    let index = |t: &Term| {
        Numeric::of(t)
            .map(Numeric::to_f64)
            .filter(|n| n.fract() == 0.0 && *n >= 0.0)
            .map(|n| n as usize)
    };
    if args.len() > 3 {
        return None;
    }
    let (Term::Literal(text), start, end) = (args.first()?, index(args.get(1)?)?, args.get(2))
    else {
        return None;
    };
    let chars: Vec<char> = text.value().chars().collect();
    let end = match end {
        Some(e) => index(e)?,
        None => chars.len(),
    };
    (start <= end && end <= chars.len())
        .then(|| Literal::new_simple_literal(chars[start..end].iter().collect::<String>()).into())
}

/// `afn:sha1sum`: the SHA-1 of a literal's lexical form or an IRI, in hex.
fn sha1sum(args: &[Term]) -> Option<Term> {
    let text = match args {
        [Term::Literal(l)] => l.value(),
        [Term::NamedNode(n)] => n.as_str(),
        _ => return None,
    };
    let hex: String = Sha1::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    string(hex)
}

/// `afn:min` and `afn:max` of two numbers: the one `keep_first` prefers, as it was given.
fn pick(args: &[Term], keep_first: fn(f64, f64) -> bool) -> Option<Term> {
    let [a @ Term::Literal(_), b @ Term::Literal(_)] = args else {
        return None;
    };
    let (x, y) = (number(a)?, number(b)?);
    Some(if keep_first(x, y) {
        a.clone()
    } else {
        b.clone()
    })
}
