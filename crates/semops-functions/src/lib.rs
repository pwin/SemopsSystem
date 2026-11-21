//! The SPARQL extension functions of SemOps System: one library, registered by every
//! evaluator, so a function that works in a query also works in a SHACL shape and in a tarka
//! mapping (docs/decisions/0003-capability-floor-and-functions.md).
//!
//! | Namespace | Origin | Module |
//! |---|---|---|
//! | `fn:` | XPath and XQuery Functions and Operators 3.1 | [`xpath`](crate::FN) |
//! | `math:` | the XPath 3.1 math functions | [`xpath`](crate::MATH) |
//! | `afn:` | Apache Jena ARQ, under all three of its namespaces | [`arq`](crate::AFN_NAMESPACES) |
//! | `spif:` | SPIN / TopBraid | [`spif`](crate::SPIF) |
//! | `apf:`, `list:`, `rdfs:member`, `spif:split` | property functions | [`pfunc`] |
//!
//! SPARQL's own built-ins (`STRLEN`, `REGEX`, `NOW` …) are the evaluator's and are not
//! repeated here.
//!
//! [`register`] gives an evaluator every function. [`library`] lists them, for an evaluator
//! that registers them itself, and [`unsupported`] says which functions of these libraries are
//! deliberately missing and what to use instead. The test table every evaluator runs is
//! [`cases`].

#![forbid(unsafe_code)]

mod arq;
pub mod cases;
mod format;
mod java;
pub mod pfunc;
mod spif;
mod value;
mod xpath;

pub use arq::AFN_NAMESPACES;
pub use pfunc::APF_NAMESPACES;
pub use spif::SPIF;
pub use value::split_xml;
pub use xpath::{FN, MATH};

use oxrdf::{NamedNode, Term};
use oxsdatatypes::DateTime;
use spareval::QueryEvaluator;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// An extension function: the evaluated arguments in, a term or an error (`None`) out.
pub type Function = Arc<dyn Fn(&[Term]) -> Option<Term> + Send + Sync>;

/// What a library is made for: the query's start, which `fn:current-dateTime`, `afn:now`
/// and `spif:currentTimeMillis` all give, unchanged for the whole query, as `NOW()` is.
pub(crate) struct Now {
    pub(crate) date_time: DateTime,
}

/// A function with no state: most of the library.
pub(crate) type Plain = fn(&[Term]) -> Option<Term>;

pub(crate) fn plain(f: Plain) -> Function {
    Arc::new(f)
}

/// Every function, with the query start taken as now.
#[must_use]
pub fn library() -> Vec<(NamedNode, Function)> {
    library_at(DateTime::now())
}

/// Every function, for a query that started at `now`.
#[must_use]
pub fn library_at(now: DateTime) -> Vec<(NamedNode, Function)> {
    let now = Now { date_time: now };
    let mut out = Vec::new();
    xpath::add(&mut out, &now);
    arq::add(&mut out, &now);
    spif::add(&mut out, &now);
    out.into_iter()
        .map(|(iri, f)| (NamedNode::new_unchecked(iri), f))
        .collect()
}

/// `evaluator` with every function registered. Build one per query: the time the functions
/// give as now is the time this was called.
#[must_use]
pub fn register(evaluator: QueryEvaluator) -> QueryEvaluator {
    register_at(evaluator, DateTime::now())
}

/// `evaluator` with every function registered, for a query that started at `now`.
#[must_use]
pub fn register_at(mut evaluator: QueryEvaluator, now: DateTime) -> QueryEvaluator {
    for (name, f) in library_at(now) {
        evaluator = evaluator.with_custom_function(name, move |args| f(args));
    }
    evaluator
}

/// The IRIs of every function, sorted: what the manual's function reference is generated
/// from, so it cannot drift from the code.
#[must_use]
pub fn names() -> Vec<String> {
    let mut names: Vec<String> = library()
        .into_iter()
        .map(|(n, _)| n.into_string())
        .collect();
    names.sort();
    names
}

/// Functions of these libraries that are deliberately not here, and why.
///
/// Each would need something a function of its arguments cannot have: a sequence type, the
/// data, an XPath static context, or a SPIN engine. A plausible answer without them would be
/// a guess wearing a specification's name.
#[must_use]
pub fn unsupported() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "fn:tokenize",
            "returns a sequence; use the property function apf:strSplit or spif:split",
        ),
        (
            "fn:string-join",
            "takes a sequence; use afn:strjoin, CONCAT, or GROUP_CONCAT over solutions",
        ),
        (
            "fn:distinct-values, fn:count, fn:sum, fn:avg, fn:min, fn:max",
            "sequence functions; use SPARQL's aggregates",
        ),
        (
            "fn:doc, fn:collection, fn:unparsed-text",
            "retrieve external documents: the server-side request surface refused for LOAD and SERVICE",
        ),
        (
            "fn:format-integer, fn:analyze-string, fn:parse-json",
            "XPath 3.1 functions with no SPARQL use that has an unambiguous answer here",
        ),
        (
            "spif:hasAllObjects",
            "reads the data, which a function of its arguments cannot",
        ),
        (
            "spif:invoke, spif:eval",
            "evaluate a SPIN function or expression; there is no SPIN engine here",
        ),
        (
            "spif:regex",
            "no implementation to check one against: neither TopBraid's documentation nor RDF4J's library defines it",
        ),
        ("apf:textMatch", "searches a text index"),
        ("apf:versionARQ", "gives the version of Jena's ARQ"),
    ]
}

/// A pseudo-random 64-bit value: the clock, the process id and a counter, mixed, on native
/// targets; the host's entropy on `wasm32`, where the first two are not available.
pub(crate) fn pseudo_u64() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut state = seed()
        ^ COUNTER
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_mul(0xD6E8_FEB8_6659_FD93);
    // xorshift, a few rounds, so that consecutive counters do not give related values
    for _ in 0..4 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
    }
    state
}

/// A pseudo-random double in [0, 1), as Java's `Math.random()` gives.
pub(crate) fn pseudo_random() -> f64 {
    (pseudo_u64() >> 11) as f64 / (1_u64 << 53) as f64
}

#[cfg(not(target_arch = "wasm32"))]
fn seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    (nanos as u64) ^ u64::from(std::process::id()).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

#[cfg(target_arch = "wasm32")]
fn seed() -> u64 {
    let mut bytes = [0_u8; 8];
    if getrandom::fill(&mut bytes).is_ok() {
        return u64::from_le_bytes(bytes);
    }
    let here = 0_u8;
    (&here as *const u8 as usize as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_function_has_its_own_iri() {
        let names = names();
        let mut unique = names.clone();
        unique.dedup();
        assert_eq!(
            names.len(),
            unique.len(),
            "a function IRI is registered twice"
        );
        // 61 fn:, 14 math:, 14 afn: under each of Jena's three namespaces, and 23 spif:. A
        // change here is a function added or lost, and the docs should say which.
        assert_eq!(names.len(), 140, "the library changed size");
    }

    #[test]
    fn a_wrong_call_is_an_error_not_a_panic() {
        // The evaluator calls these with whatever the query supplied.
        let s = |v: &str| Term::from(oxrdf::Literal::new_simple_literal(v));
        let i = |v: i64| Term::from(oxrdf::Literal::from(v));
        let iri = Term::from(NamedNode::new_unchecked("http://example.com/x"));
        for (name, f) in library() {
            for args in [
                vec![],
                vec![s("only-one")],
                vec![i(-1), i(0)],
                vec![iri.clone(), s(""), i(i64::MAX)],
                vec![s("%"), s("["), s("$9")],
            ] {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&args)));
                assert!(result.is_ok(), "{name} panicked on {args:?}");
            }
        }
    }

    #[test]
    fn identifiers_and_random_numbers_vary() {
        let a = pseudo_u64();
        let b = pseudo_u64();
        assert_ne!(a, b);
        let r = pseudo_random();
        assert!((0.0..1.0).contains(&r));
    }
}
