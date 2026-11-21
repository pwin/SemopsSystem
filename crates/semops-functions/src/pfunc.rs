//! Property functions: Jena ARQ's (`apf:`, `list:`, `rdfs:member`) and TopBraid's
//! `spif:split`. `?tag apf:strSplit (?tags ";")` binds `?tag` to each part of `?tags`.
//!
//! A property function is a triple pattern with a reserved predicate, and Jena evaluates it
//! where it stands in its group, with what the patterns before it have bound. This module is
//! the part every evaluator shares: which predicates are property functions, how their
//! argument lists are spelled in a basic graph pattern, and what each one makes of a solution.
//! Where the evaluation happens is each evaluator's own business. tarka applies them to
//! the solutions of one row; HOLOS evaluates the patterns before one, applies it, and
//! continues from the result.
//!
//! Two kinds:
//!
//! - **Functions of their arguments** (`apf:strSplit`, `apf:concat`, `apf:splitIRI`,
//!   `apf:str`, `apf:assign`, `apf:blankNode`, `spif:split`), which [`PropertyFunction::apply`]
//!   evaluates against a solution.
//! - **Readers of the data** (`list:member`, `list:index`, `list:length`, `rdfs:member`,
//!   `apf:bag`, `apf:seq`, `apf:alt`), which walk RDF lists and containers. Only an evaluator
//!   with a store can answer them; [`Kind::reads_data`] says which these are.
//!
//! Each does what Jena 4.8's does (`org.apache.jena.sparql.pfunction.library`), as tarka
//! checked against TARQL 1.3, and `spif:split` what RDF4J's `Split` does.

use oxrdf::vocab::{rdf, xsd};
use oxrdf::{Literal, NamedNode, NamedNodeRef, Term};
use spargebra::term::{NamedNodePattern, TermPattern, TriplePattern, Variable};
use std::collections::HashMap;

use crate::value::{Numeric, split_xml};

/// The compiled regular expressions [`PropertyFunction::apply`] keeps between calls.
pub use regex::Regex;

/// Jena's namespaces for its property functions: the current one, the one it had before it
/// moved to Apache, and the Java package that loads them directly.
pub const APF_NAMESPACES: [&str; 3] = [
    "http://jena.apache.org/ARQ/property#",
    "http://jena.hpl.hp.com/ARQ/property#",
    "java:org.apache.jena.sparql.pfunction.library.",
];
/// Jena's list property functions.
pub const LIST_NAMESPACE: &str = "http://jena.apache.org/ARQ/list#";
/// `rdfs:member`, which Jena evaluates as container membership.
pub const RDFS_MEMBER: &str = "http://www.w3.org/2000/01/rdf-schema#member";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    StrSplit,
    SpifSplit,
    Concat,
    SplitIri,
    Str,
    Assign,
    BlankNode,
    ListMember,
    ListIndex,
    ListLength,
    Member,
    Bag,
    Seq,
    Alt,
}

impl Kind {
    /// Whether it reads RDF lists or containers in the data rather than computing from its
    /// arguments.
    #[must_use]
    pub fn reads_data(self) -> bool {
        matches!(
            self,
            Self::ListMember
                | Self::ListIndex
                | Self::ListLength
                | Self::Member
                | Self::Bag
                | Self::Seq
                | Self::Alt
        )
    }
}

/// A side of a property function: one term, or a list of them.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    One(TermPattern),
    List(Vec<TermPattern>),
}

/// What a property function binds a variable to (tarka's SHACL shapes read this).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    String,
    Iri,
    /// Whatever the other side is, or a data term.
    Any,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyFunction {
    pub kind: Kind,
    /// As a message names it: `apf:strSplit`.
    pub name: String,
    pub subject: Arg,
    pub object: Arg,
}

/// A solution, variable by variable.
pub type Bindings = HashMap<Variable, Term>;

/// What a predicate names: a property function, one that is refused (with why), or neither.
#[derive(Debug, PartialEq, Eq)]
pub enum Named {
    Function(Kind, String),
    Refused(String, &'static str),
    Not,
}

/// Whether `iri` is a property function's.
#[must_use]
pub fn named(iri: &str) -> Named {
    if iri == RDFS_MEMBER {
        return Named::Function(Kind::Member, "rdfs:member".into());
    }
    if iri == "http://spinrdf.org/spif#split" {
        return Named::Function(Kind::SpifSplit, "spif:split".into());
    }
    if let Some(local) = iri.strip_prefix(LIST_NAMESPACE) {
        let name = format!("list:{local}");
        return match local {
            "member" => Named::Function(Kind::ListMember, name),
            "index" => Named::Function(Kind::ListIndex, name),
            "length" => Named::Function(Kind::ListLength, name),
            _ => Named::Refused(name, "it is not one of Jena's list property functions"),
        };
    }
    let Some(local) = APF_NAMESPACES.iter().find_map(|ns| iri.strip_prefix(ns)) else {
        return Named::Not;
    };
    let name = format!("apf:{local}");
    let kind = match local {
        "strSplit" => Kind::StrSplit,
        "concat" => Kind::Concat,
        "splitIRI" | "splitURI" => Kind::SplitIri,
        "str" => Kind::Str,
        "assign" => Kind::Assign,
        "blankNode" | "bnode" => Kind::BlankNode,
        "bag" => Kind::Bag,
        "seq" => Kind::Seq,
        "alt" => Kind::Alt,
        "textMatch" => return Named::Refused(name, "it searches a text index"),
        "versionARQ" => return Named::Refused(name, "it gives the version of Jena's ARQ"),
        _ => return Named::Refused(name, "it is not one of Jena's property functions"),
    };
    Named::Function(kind, name)
}

/// The property functions of a basic graph pattern, in order, with their argument lists read
/// from the `rdf:first` and `rdf:rest` patterns that spell them; and the patterns that are
/// neither. `refuse` names the kinds the caller cannot evaluate, with why.
///
/// # Errors
///
/// A refused property function, one the caller refuses, or one written in a way Jena would
/// refuse, as a message naming it.
pub fn split_bgp(
    patterns: &[TriplePattern],
    refuse: impl Fn(Kind) -> Option<&'static str>,
) -> Result<(Vec<PropertyFunction>, Vec<TriplePattern>), String> {
    let mut used = vec![false; patterns.len()];
    let mut functions = Vec::new();
    for (i, t) in patterns.iter().enumerate() {
        let NamedNodePattern::NamedNode(p) = &t.predicate else {
            continue;
        };
        let (kind, name) = match named(p.as_str()) {
            Named::Not => continue,
            Named::Refused(name, why) => return Err(format!("{name} is not supported: {why}")),
            Named::Function(kind, name) => (kind, name),
        };
        if let Some(why) = refuse(kind) {
            return Err(format!("{name} is not supported: {why}"));
        }
        used[i] = true;
        // `rdf:nil` reads as the empty list; where a side is a term, it is the term rdf:nil.
        let nil_as_term = |a: Arg| match a {
            Arg::List(items) if items.is_empty() => {
                Arg::One(TermPattern::NamedNode(rdf::NIL.into_owned()))
            }
            other => other,
        };
        let subject = nil_as_term(argument(&t.subject, patterns, &mut used));
        let object = argument(&t.object, patterns, &mut used);
        let takes_list = matches!(
            kind,
            Kind::StrSplit | Kind::SpifSplit | Kind::Concat | Kind::SplitIri | Kind::ListIndex
        );
        let object = if takes_list {
            object
        } else {
            nil_as_term(object)
        };
        let f = PropertyFunction {
            kind,
            name,
            subject,
            object,
        };
        f.check()?;
        functions.push(f);
    }
    let rest = patterns
        .iter()
        .zip(&used)
        .filter(|(_, u)| !**u)
        .map(|(t, _)| t.clone())
        .collect();
    Ok((functions, rest))
}

/// Whether any pattern of a basic graph pattern has a property function's predicate,
/// including a refused one.
#[must_use]
pub fn mentions_any(patterns: &[TriplePattern]) -> bool {
    patterns.iter().any(|t| matches!(&t.predicate, NamedNodePattern::NamedNode(p) if named(p.as_str()) != Named::Not))
}

/// A side of a property function: a list, if the term heads one spelled out in the
/// patterns, or else the term.
fn argument(t: &TermPattern, patterns: &[TriplePattern], used: &mut [bool]) -> Arg {
    let nil = |t: &TermPattern| matches!(t, TermPattern::NamedNode(n) if n.as_ref() == rdf::NIL);
    if nil(t) {
        return Arg::List(Vec::new());
    }
    let find = |node: &TermPattern, predicate: NamedNodeRef<'_>| {
        patterns.iter().position(|p| {
            &p.subject == node
                && matches!(&p.predicate, NamedNodePattern::NamedNode(n) if n.as_ref() == predicate)
        })
    };
    let (mut items, mut spelling, mut node) = (Vec::new(), Vec::new(), t.clone());
    // a list is no longer than the patterns that spell it
    for _ in 0..patterns.len() {
        if !matches!(node, TermPattern::BlankNode(_)) {
            break;
        }
        let (Some(first), Some(rest)) = (find(&node, rdf::FIRST), find(&node, rdf::REST)) else {
            break;
        };
        items.push(patterns[first].object.clone());
        spelling.extend([first, rest]);
        node = patterns[rest].object.clone();
        if nil(&node) {
            spelling.into_iter().for_each(|i| used[i] = true);
            return Arg::List(items);
        }
    }
    Arg::One(t.clone())
}

impl PropertyFunction {
    /// Fails on a property function written in a way Jena would refuse.
    ///
    /// # Errors
    ///
    /// How it should have been written.
    pub fn check(&self) -> Result<(), String> {
        let usage = |usage: &str| Err(format!("{} is written {usage}", self.name));
        let one = |a: &Arg| matches!(a, Arg::One(_));
        match (self.kind, &self.subject, &self.object) {
            (Kind::StrSplit | Kind::SpifSplit, Arg::One(_), Arg::List(args)) if args.len() == 2 => {
                // a regular expression written in the query is checked now
                if let TermPattern::Literal(re) = &args[1] {
                    Regex::new(re.value()).map_err(|e| format!("{}: {e}", self.name))?;
                }
                Ok(())
            }
            (Kind::StrSplit, ..) => usage("?part apf:strSplit (?string \"regular expression\")"),
            (Kind::SpifSplit, ..) => usage("?part spif:split (?string \"regular expression\")"),
            (Kind::Concat, Arg::One(TermPattern::Variable(_)), _) => Ok(()),
            (Kind::Concat, ..) => {
                usage("?string apf:concat (?a ?b …), with a variable for its subject")
            }
            (Kind::SplitIri, Arg::One(_), Arg::List(args)) if args.len() == 2 => Ok(()),
            (Kind::SplitIri, ..) => usage(&format!("?iri {} (?namespace ?localname)", self.name)),
            (Kind::Str | Kind::Assign | Kind::BlankNode, s, o) if one(s) && one(o) => Ok(()),
            (Kind::Str, ..) => usage("?string apf:str ?term"),
            (Kind::Assign, ..) => usage("?a apf:assign ?b"),
            (Kind::BlankNode, ..) => usage(&format!("?node {} ?label", self.name)),
            (
                Kind::ListMember
                | Kind::ListLength
                | Kind::Member
                | Kind::Bag
                | Kind::Seq
                | Kind::Alt,
                s,
                o,
            ) if one(s) && one(o) => Ok(()),
            (Kind::ListIndex, Arg::One(_), Arg::List(args)) if args.len() == 2 => Ok(()),
            (Kind::ListIndex, ..) => usage("?list list:index (?index ?member)"),
            (Kind::ListLength, ..) => usage("?list list:length ?length"),
            _ => usage(&format!("?container {} ?member", self.name)),
        }
    }

    /// The variables it binds, and to what (for SHACL shapes).
    #[must_use]
    pub fn outputs(&self) -> Vec<(&Variable, Output)> {
        fn var(t: &TermPattern) -> Option<&Variable> {
            match t {
                TermPattern::Variable(v) => Some(v),
                _ => None,
            }
        }
        let mut out = Vec::new();
        match (&self.kind, &self.subject, &self.object) {
            (Kind::StrSplit | Kind::SpifSplit | Kind::Concat | Kind::Str, Arg::One(s), _) => {
                out.extend(var(s).map(|v| (v, Output::String)));
            }
            (Kind::SplitIri, _, Arg::List(args)) => {
                out.extend(var(&args[0]).map(|v| (v, Output::Iri)));
                out.extend(var(&args[1]).map(|v| (v, Output::String)));
            }
            (Kind::Assign, Arg::One(s), Arg::One(o)) => out.extend(
                [var(s), var(o)]
                    .into_iter()
                    .flatten()
                    .map(|v| (v, Output::Any)),
            ),
            (Kind::BlankNode, _, Arg::One(o)) => out.extend(var(o).map(|v| (v, Output::String))),
            _ => {}
        }
        out
    }

    /// The solutions it makes of one: none, one or several. Errors are Jena's: an `apf:str`
    /// or `apf:assign` with nothing to work from, or a regular expression that is not one.
    ///
    /// # Errors
    ///
    /// As above, and a property function that reads the data, which this cannot answer.
    pub fn apply(
        &self,
        mu: &Bindings,
        regexes: &mut HashMap<String, Regex>,
    ) -> Result<Vec<Bindings>, String> {
        let value = |t: &TermPattern| -> Option<Term> {
            match t {
                TermPattern::Variable(v) => mu.get(v).cloned(),
                TermPattern::NamedNode(n) => Some(n.clone().into()),
                TermPattern::Literal(l) => Some(l.clone().into()),
                _ => None,
            }
        };
        let unbound =
            |t: &TermPattern| matches!(t, TermPattern::Variable(v) if !mu.contains_key(v));
        // `target` given `term`: bound if a free variable, else kept only if it is the same
        let give = |target: &TermPattern, term: Term| -> Vec<Bindings> {
            match target {
                TermPattern::Variable(v) if !mu.contains_key(v) => {
                    let mut next = mu.clone();
                    next.insert(v.clone(), term);
                    vec![next]
                }
                t => value(t)
                    .filter(|t| *t == term)
                    .map(|_| mu.clone())
                    .into_iter()
                    .collect(),
            }
        };
        if self.kind.reads_data() {
            return Err(format!(
                "{} reads the data, and is evaluated by the store",
                self.name
            ));
        }
        Ok(match (self.kind, &self.subject, &self.object) {
            (Kind::StrSplit | Kind::SpifSplit, Arg::One(s), Arg::List(args)) => {
                let (Some(Term::Literal(text)), Some(Term::Literal(re))) =
                    (value(&args[0]), value(&args[1]))
                else {
                    return Ok(Vec::new());
                };
                if !regexes.contains_key(re.value()) {
                    let compiled =
                        Regex::new(re.value()).map_err(|e| format!("{}: {e}", self.name))?;
                    regexes.insert(re.value().to_owned(), compiled);
                }
                let split = java_split(text.value(), &regexes[re.value()]);
                // Jena trims each part of apf:strSplit; RDF4J's spif:split keeps them as split.
                let parts: Vec<&str> = if self.kind == Kind::StrSplit {
                    split.into_iter().map(java_trim).collect()
                } else {
                    split
                };
                match (s, value(s)) {
                    (TermPattern::Variable(v), None) => parts
                        .into_iter()
                        .map(|p| {
                            let mut next = mu.clone();
                            next.insert(v.clone(), Literal::new_simple_literal(p).into());
                            next
                        })
                        .collect(),
                    // bound: kept if it is one of the parts
                    (_, Some(Term::Literal(l)))
                        if l.datatype() == xsd::STRING && parts.contains(&l.value()) =>
                    {
                        vec![mu.clone()]
                    }
                    _ => Vec::new(),
                }
            }
            (Kind::Concat, Arg::One(s), object) => {
                let args = match object {
                    Arg::List(args) => args.as_slice(),
                    Arg::One(t) => std::slice::from_ref(t),
                };
                let mut text = String::new();
                for a in args {
                    match value(a) {
                        Some(Term::Literal(l)) => text.push_str(l.value()),
                        Some(Term::NamedNode(n)) => text.push_str(n.as_str()),
                        // unbound, or a blank node
                        _ => return Ok(Vec::new()),
                    }
                }
                give(s, Literal::new_simple_literal(text).into())
            }
            (Kind::SplitIri, Arg::One(s), Arg::List(args)) => {
                let Some(Term::NamedNode(iri)) = value(s) else {
                    return Ok(Vec::new());
                };
                let at = split_xml(iri.as_str());
                let (namespace, localname) = iri.as_str().split_at(at);
                let Ok(namespace) = NamedNode::new(namespace) else {
                    return Ok(Vec::new());
                };
                let mut out = Vec::new();
                // the namespace: an IRI, or a string with its text
                let namespaced: Vec<Bindings> = match (&args[0], value(&args[0])) {
                    (_, None) => give(&args[0], namespace.clone().into()),
                    (_, Some(Term::NamedNode(n))) if n == namespace => vec![mu.clone()],
                    (_, Some(Term::Literal(l)))
                        if l.datatype() == xsd::STRING && l.value() == namespace.as_str() =>
                    {
                        vec![mu.clone()]
                    }
                    _ => Vec::new(),
                };
                for m in namespaced {
                    let local = Literal::new_simple_literal(localname);
                    match &args[1] {
                        TermPattern::Variable(v) if !m.contains_key(v) => {
                            let mut next = m;
                            next.insert(v.clone(), local.into());
                            out.push(next);
                        }
                        TermPattern::Variable(v) => {
                            if m.get(v) == Some(&Term::from(local)) {
                                out.push(m);
                            }
                        }
                        t => {
                            if value(t) == Some(local.into()) {
                                out.push(m);
                            }
                        }
                    }
                }
                out
            }
            (Kind::Str, Arg::One(s), Arg::One(o)) => {
                if matches!(value(s), Some(Term::NamedNode(_) | Term::BlankNode(_)))
                    || matches!(s, TermPattern::BlankNode(_))
                {
                    return Ok(Vec::new());
                }
                let text = match value(o) {
                    _ if unbound(s) && unbound(o) => {
                        return Err(format!(
                            "{}: its subject and object are both unbound",
                            self.name
                        ));
                    }
                    None => return Err(format!("{}: its object is unbound", self.name)),
                    Some(Term::Literal(l)) => l.value().to_owned(),
                    Some(Term::NamedNode(n)) => n.as_str().to_owned(),
                    Some(_) => return Err(format!("{}: its object is a blank node", self.name)),
                };
                give(s, Literal::new_simple_literal(text).into())
            }
            (Kind::Assign, Arg::One(s), Arg::One(o)) => match (value(s), value(o)) {
                (None, None) => {
                    return Err(format!(
                        "{}: its subject and object are both unbound",
                        self.name
                    ));
                }
                (None, Some(term)) => give(s, term),
                (Some(term), None) => give(o, term),
                (Some(a), Some(b)) => {
                    if same_value(&a, &b) {
                        vec![mu.clone()]
                    } else {
                        Vec::new()
                    }
                }
            },
            (Kind::BlankNode, Arg::One(s), Arg::One(o)) => match value(s) {
                // the label the evaluator gives the blank node
                Some(Term::BlankNode(b)) => give(o, Literal::new_simple_literal(b.as_str()).into()),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        })
    }
}

/// Java's `String.split(regex)`, as Jena's `apf:strSplit` and RDF4J's `spif:split` call it:
/// an empty string is one empty part, a match of no width at the start makes no leading part,
/// and the empty parts at the end are dropped.
#[must_use]
pub fn java_split<'a>(text: &'a str, re: &Regex) -> Vec<&'a str> {
    if text.is_empty() {
        return vec![""];
    }
    let (mut parts, mut last) = (Vec::new(), 0);
    for m in re.find_iter(text) {
        if m.end() == 0 {
            continue;
        }
        parts.push(&text[last..m.start()]);
        last = m.end();
    }
    parts.push(&text[last..]);
    while parts.last() == Some(&"") {
        parts.pop();
    }
    parts
}

/// Java's `String.trim()`: the characters up to U+0020 at either end.
#[must_use]
pub fn java_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c <= ' ')
}

/// Jena's `sameValueAs`, as `apf:assign` tests two bound sides: the same term, or numbers of
/// the same value.
#[must_use]
pub fn same_value(a: &Term, b: &Term) -> bool {
    if a == b {
        return true;
    }
    match (
        Numeric::of(a).filter(|_| typed_number(a)),
        Numeric::of(b).filter(|_| typed_number(b)),
    ) {
        (Some(x), Some(y)) => x.to_f64() == y.to_f64(),
        _ => false,
    }
}

fn typed_number(t: &Term) -> bool {
    matches!(t, Term::Literal(l) if l.datatype() != xsd::STRING)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(text: &str, re: &str) -> Vec<String> {
        java_split(text, &Regex::new(re).unwrap())
            .into_iter()
            .map(|p| java_trim(p).to_owned())
            .collect()
    }

    /// As Jena 4.8 splits them (TARQL 1.3, checked).
    #[test]
    fn strings_split_as_java_splits_them() {
        assert_eq!(split("a;;b", ";"), ["a", "", "b"]);
        assert_eq!(split("a;b;", ";"), ["a", "b"]);
        assert_eq!(split("a;b; ", ";"), ["a", "b", ""]);
        assert_eq!(split(";a", ";"), ["", "a"]);
        assert_eq!(split("  x ; y  ", ";"), ["x", "y"]);
        assert_eq!(split(";;", ";"), Vec::<String>::new());
        assert_eq!(split("a", ";"), ["a"]);
        assert_eq!(split("", ";"), [""]);
        assert_eq!(split("a ; b|c ;d", r"\s*[;|]\s*"), ["a", "b", "c", "d"]);
        // a match of no width splits between characters, with no leading part
        assert_eq!(split("abc", ""), ["a", "b", "c"]);
    }

    fn functions(query: &str) -> Result<Vec<PropertyFunction>, String> {
        let text = format!(
            "PREFIX apf: <http://jena.apache.org/ARQ/property#> PREFIX list: <http://jena.apache.org/ARQ/list#> \
             PREFIX spif: <http://spinrdf.org/spif#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT * WHERE {{ {query} }}"
        );
        let spargebra::Query::Select { pattern, .. } =
            spargebra::SparqlParser::new().parse_query(&text).unwrap()
        else {
            unreachable!()
        };
        let mut found = Vec::new();
        fn walk(p: &spargebra::algebra::GraphPattern, found: &mut Vec<Vec<TriplePattern>>) {
            use spargebra::algebra::GraphPattern as G;
            match p {
                G::Bgp { patterns } => found.push(patterns.clone()),
                G::Project { inner, .. } | G::Filter { inner, .. } | G::Extend { inner, .. } => {
                    walk(inner, found)
                }
                G::Join { left, right } => {
                    walk(left, found);
                    walk(right, found);
                }
                _ => {}
            }
        }
        let mut bgps = Vec::new();
        walk(&pattern, &mut bgps);
        for bgp in bgps {
            found.extend(split_bgp(&bgp, |_| None)?.0);
        }
        Ok(found)
    }

    fn apply(query: &str, given: &[(&str, Term)]) -> Result<Vec<Vec<(String, String)>>, String> {
        let mu: Bindings = given
            .iter()
            .map(|(v, t)| (Variable::new_unchecked(*v), t.clone()))
            .collect();
        let out = functions(query)?
            .remove(0)
            .apply(&mu, &mut HashMap::new())?;
        Ok(out
            .into_iter()
            .map(|m| {
                let mut pairs: Vec<(String, String)> = m
                    .into_iter()
                    .map(|(v, t)| (v.as_str().to_owned(), t.to_string()))
                    .collect();
                pairs.sort();
                pairs
            })
            .collect())
    }

    fn lit(s: &str) -> Term {
        Literal::new_simple_literal(s).into()
    }

    #[test]
    fn each_function_does_what_jena_does() {
        let pairs = |v: &[(&str, &str)]| {
            v.iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect::<Vec<_>>()
        };
        // strSplit: a solution per part, trimmed; bound, kept if one of them
        assert_eq!(
            apply("?p apf:strSplit (?s \";\")", &[("s", lit("a; b"))])
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            apply(
                "?p apf:strSplit (?s \";\")",
                &[("s", lit("a; b")), ("p", lit("b"))]
            )
            .unwrap(),
            [pairs(&[("p", "\"b\""), ("s", "\"a; b\"")])]
        );
        assert_eq!(
            apply(
                "?p apf:strSplit (?s \";\")",
                &[("s", lit("a;b")), ("p", lit("c"))]
            )
            .unwrap()
            .len(),
            0
        );
        assert_eq!(apply("?p apf:strSplit (?s \";\")", &[]).unwrap().len(), 0);
        // spif:split keeps the parts as split
        assert_eq!(
            apply("?p spif:split (\"a; b\" \";\")", &[]).unwrap(),
            [pairs(&[("p", "\"a\"")]), pairs(&[("p", "\" b\"")])]
        );
        // concat: lexical forms and IRIs; nothing if an argument is unbound
        assert_eq!(
            apply("?c apf:concat (\"x\" 1 <http://ex/y> \"z\"@en)", &[]).unwrap(),
            [pairs(&[("c", "\"x1http://ex/yz\"")])]
        );
        assert_eq!(
            apply("?c apf:concat (\"x\" ?missing)", &[]).unwrap().len(),
            0
        );
        // splitIRI: the namespace an IRI, the local name a string
        assert_eq!(
            apply("<http://example.com/ns#thing> apf:splitIRI (?ns ?ln)", &[]).unwrap(),
            [pairs(&[
                ("ln", "\"thing\""),
                ("ns", "<http://example.com/ns#>")
            ])]
        );
        assert_eq!(apply("?i apf:splitURI (?ns ?ln)", &[]).unwrap().len(), 0);
        // str and assign
        assert_eq!(
            apply("?s apf:str <http://ex/w>", &[]).unwrap(),
            [pairs(&[("s", "\"http://ex/w\"")])]
        );
        assert!(apply("?s apf:str ?o", &[]).is_err());
        assert_eq!(
            apply("?a apf:assign 42", &[]).unwrap(),
            [pairs(&[(
                "a",
                "\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>"
            )])]
        );
        assert_eq!(
            apply("1 apf:assign 1.0", &[]).unwrap().len(),
            1,
            "the same value"
        );
        assert!(apply("?a apf:assign ?b", &[]).is_err());
        // blankNode: a blank node's label, and nothing for anything else
        let b = Term::BlankNode(oxrdf::BlankNode::new_unchecked("b0"));
        assert_eq!(
            apply("?n apf:blankNode ?l", &[("n", b)]).unwrap(),
            [pairs(&[("l", "\"b0\""), ("n", "_:b0")])]
        );
        assert_eq!(
            apply("?n apf:bnode ?l", &[("n", lit("x"))]).unwrap().len(),
            0
        );
    }

    #[test]
    fn what_cannot_be_evaluated_is_refused() {
        let error = |q: &str| functions(q).unwrap_err();
        assert!(error("?x apf:textMatch \"x\"").contains("text index"));
        assert!(error("?x apf:nope ?y").contains("not one of Jena's property functions"));
        assert!(error("?x list:nope ?y").contains("not one of Jena's list property functions"));
        assert!(
            error("?p apf:strSplit (?s)").contains("apf:strSplit is written ?part apf:strSplit")
        );
        assert!(error("?p apf:strSplit (?s \"[\")").contains("apf:strSplit:"));
        assert!(error("?l list:index ?i").contains("list:index is written"));
        // readers of the data are recognised, and a caller without data refuses them
        let f = functions("?l list:member ?m").unwrap();
        assert!(f[0].kind.reads_data());
        assert!(f[0].apply(&Bindings::new(), &mut HashMap::new()).is_err());
        let text = "SELECT * WHERE { ?c <http://www.w3.org/2000/01/rdf-schema#member> ?m }";
        let spargebra::Query::Select { pattern, .. } =
            spargebra::SparqlParser::new().parse_query(text).unwrap()
        else {
            unreachable!()
        };
        let spargebra::algebra::GraphPattern::Project { inner, .. } = pattern else {
            unreachable!()
        };
        let spargebra::algebra::GraphPattern::Bgp { patterns } = *inner else {
            unreachable!()
        };
        let refused =
            split_bgp(&patterns, |k| k.reads_data().then_some("it reads the data")).unwrap_err();
        assert_eq!(refused, "rdfs:member is not supported: it reads the data");
    }
}
