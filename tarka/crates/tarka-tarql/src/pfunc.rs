//! Jena ARQ's property functions, as TARQL queries written for Jena use them:
//! `?tag apf:strSplit (?tags ";")` binds `?tag` to each part of `?tags`.
//!
//! A property function is a triple pattern with a reserved predicate, and Jena evaluates
//! it where it stands in the WHERE group, with what the patterns before it have bound.
//! tarka evaluates rows against no data, where such a pattern would match nothing; so a
//! group with property functions in it is taken as steps ([`Staged`]): spareval evaluates
//! the patterns up to a property function, tarka applies the function to each solution,
//! and the rest of the group is evaluated on what it gives.
//!
//! The functions themselves, how their arguments are spelled and what each makes of a
//! solution are `semops_functions::pfunc`, which HOLOS evaluates the same functions with
//! over a store. What is tarka's is the staging, and what it refuses: the property
//! functions that read RDF lists or containers in the data (`list:member`, `rdfs:member`,
//! `apf:bag` …) have nothing to work on in a row, `apf:blankNode` would give a label tarka
//! makes afresh on every run, and a property function anywhere but the top level of the
//! WHERE group is refused, since a query that relied on one would otherwise make nothing,
//! and say nothing.

use holos_engine::spargebra;

use spargebra::algebra::{Expression, GraphPattern};
use spargebra::term::{TriplePattern, Variable};

use crate::TarqlError;
use crate::inject;

pub use semops_functions::pfunc::{APF_NAMESPACES, Arg, Bindings, Kind, Output, PropertyFunction, java_split, java_trim};
pub use semops_functions::split_xml;

const IN_THE_DATA: &str = "it reads the data, and a TARQL row has none";

/// The property functions of a basic graph pattern, in order, with their argument lists
/// read from the `rdf:first` and `rdf:rest` patterns that spell them; and the patterns that
/// are neither. Fails on one tarka cannot evaluate, naming it and saying why.
pub fn split_bgp(patterns: &[TriplePattern]) -> Result<(Vec<PropertyFunction>, Vec<TriplePattern>), TarqlError> {
    semops_functions::pfunc::split_bgp(patterns, |kind| match kind {
        k if k.reads_data() => Some(IN_THE_DATA),
        Kind::BlankNode => Some("it gives a blank node's label, which tarka makes afresh on every run"),
        _ => None,
    })
    .map_err(TarqlError::PropertyFunction)
}

/// A WHERE group with property functions in it, as steps to take in order.
#[derive(Clone, Debug)]
pub struct Staged {
    /// From the group's left-most pattern on.
    pub steps: Vec<Step>,
    /// The group's FILTER, which holds for all of it.
    pub filter: Option<Expression>,
}

#[derive(Clone, Debug)]
pub enum Step {
    Extend(Variable, Expression),
    Join(GraphPattern),
    LeftJoin(GraphPattern, Option<Expression>),
    Minus(GraphPattern),
    Function(PropertyFunction),
}

impl Step {
    /// `inner` with this step applied (a property function is not a SPARQL step).
    pub fn wrap(&self, inner: GraphPattern) -> GraphPattern {
        let inner = Box::new(inner);
        match self {
            Self::Extend(variable, expression) => {
                GraphPattern::Extend { inner, variable: variable.clone(), expression: expression.clone() }
            }
            Self::Join(right) => GraphPattern::Join { left: inner, right: Box::new(right.clone()) },
            Self::LeftJoin(right, expression) => {
                GraphPattern::LeftJoin { left: inner, right: Box::new(right.clone()), expression: expression.clone() }
            }
            Self::Minus(right) => GraphPattern::Minus { left: inner, right: Box::new(right.clone()) },
            Self::Function(_) => *inner,
        }
    }
}

/// The steps of a WHERE pattern's group, if it has property functions; checks each of
/// them, wherever it is, and that none is below the top level of the group.
pub fn staged(pattern: &GraphPattern) -> Result<Option<Staged>, TarqlError> {
    let all = count(pattern)?;
    if all == 0 {
        return Ok(None);
    }
    let (filter, chain) = match inject::group(pattern) {
        GraphPattern::Filter { expr, inner } => (Some(expr.clone()), inner.as_ref()),
        group => (None, group),
    };
    let mut steps = Vec::new();
    linear(chain, &mut steps)?;
    if steps.iter().filter(|s| matches!(s, Step::Function(_))).count() < all {
        return Err(TarqlError::PropertyFunction(
            "a property function (apf:…) is evaluated at the top level of the WHERE clause, \
             not inside OPTIONAL, UNION, MINUS, GRAPH, a sub-query or a nested group"
                .into(),
        ));
    }
    Ok(Some(Staged { steps, filter }))
}

fn linear(p: &GraphPattern, steps: &mut Vec<Step>) -> Result<(), TarqlError> {
    match p {
        GraphPattern::Extend { inner, variable, expression } => {
            linear(inner, steps)?;
            steps.push(Step::Extend(variable.clone(), expression.clone()));
        }
        GraphPattern::Join { left, right } => {
            linear(left, steps)?;
            part(right, steps)?;
        }
        GraphPattern::LeftJoin { left, right, expression } => {
            linear(left, steps)?;
            steps.push(Step::LeftJoin((**right).clone(), expression.clone()));
        }
        GraphPattern::Minus { left, right } => {
            linear(left, steps)?;
            steps.push(Step::Minus((**right).clone()));
        }
        other => part(other, steps)?,
    }
    Ok(())
}

/// One pattern of a group: a basic graph pattern's property functions become steps of
/// their own.
fn part(p: &GraphPattern, steps: &mut Vec<Step>) -> Result<(), TarqlError> {
    match p {
        GraphPattern::Bgp { patterns } => {
            let (functions, rest) = split_bgp(patterns)?;
            if !rest.is_empty() {
                steps.push(Step::Join(GraphPattern::Bgp { patterns: rest }));
            }
            steps.extend(functions.into_iter().map(Step::Function));
        }
        other => steps.push(Step::Join(other.clone())),
    }
    Ok(())
}

/// The property functions anywhere in a pattern, each checked.
fn count(p: &GraphPattern) -> Result<usize, TarqlError> {
    Ok(match p {
        GraphPattern::Bgp { patterns } => split_bgp(patterns)?.0.len(),
        GraphPattern::Join { left, right }
        | GraphPattern::LeftJoin { left, right, .. }
        | GraphPattern::Union { left, right }
        | GraphPattern::Minus { left, right } => count(left)? + count(right)?,
        GraphPattern::Filter { inner, .. }
        | GraphPattern::Extend { inner, .. }
        | GraphPattern::Graph { inner, .. }
        | GraphPattern::Service { inner, .. }
        | GraphPattern::Project { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::OrderBy { inner, .. }
        | GraphPattern::Group { inner, .. } => count(inner)?,
        _ => 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use holos_engine::oxrdf::{Literal, Term};
    use regex::Regex;
    use std::collections::HashMap;

    fn split(text: &str, re: &str) -> Vec<String> {
        java_split(text, &Regex::new(re).unwrap()).into_iter().map(|p| java_trim(p).to_owned()).collect()
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

    /// As Jena 4.8 splits them (TARQL 1.3, checked).
    #[test]
    fn iris_split_before_an_xml_name() {
        let parts = |iri: &str| {
            let at = split_xml(iri);
            (iri[..at].to_owned(), iri[at..].to_owned())
        };
        assert_eq!(parts("http://example.com/ns#thing"), ("http://example.com/ns#".into(), "thing".into()));
        assert_eq!(parts("http://example.com/a/b"), ("http://example.com/a/".into(), "b".into()));
        assert_eq!(parts("http://example.com/item/123"), ("http://example.com/item/123".into(), "".into()));
        assert_eq!(parts("urn:isbn:0451450523"), ("urn:isbn:0451450523".into(), "".into()));
        assert_eq!(parts("mailto:me@example.com"), ("mailto:me@".into(), "example.com".into()));
        assert_eq!(parts("http://ex.org/a-1.b"), ("http://ex.org/".into(), "a-1.b".into()));
        assert_eq!(parts("http://example.com/"), ("http://example.com/".into(), "".into()));
        assert_eq!(parts("http://ex.org/p%20q"), ("http://ex.org/p%20".into(), "q".into()));
    }

    fn function(query: &str) -> PropertyFunction {
        let text = format!("PREFIX apf: <http://jena.apache.org/ARQ/property#> SELECT * WHERE {{ {query} }}");
        let spargebra::Query::Select { pattern, .. } = spargebra::SparqlParser::new().parse_query(&text).unwrap() else { unreachable!() };
        let Some(Staged { steps, .. }) = staged(&pattern).unwrap() else { panic!("no property function") };
        steps.into_iter().find_map(|s| if let Step::Function(f) = s { Some(f) } else { None }).unwrap()
    }

    fn apply(query: &str, given: &[(&str, Term)]) -> Result<Vec<Vec<(String, String)>>, String> {
        let mu: Bindings = given.iter().map(|(v, t)| (Variable::new_unchecked(*v), t.clone())).collect();
        let out = function(query).apply(&mu, &mut HashMap::new())?;
        Ok(out
            .into_iter()
            .map(|m| {
                let mut pairs: Vec<(String, String)> = m.into_iter().map(|(v, t)| (v.as_str().to_owned(), t.to_string())).collect();
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
        let pairs = |v: &[(&str, &str)]| v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
        // strSplit: a solution per part; bound, kept if one of them; arguments must be literals
        assert_eq!(apply("?p apf:strSplit (?s \";\")", &[("s", lit("a;b"))]).unwrap().len(), 2);
        assert_eq!(
            apply("?p apf:strSplit (?s \";\")", &[("s", lit("a;b")), ("p", lit("b"))]).unwrap(),
            [pairs(&[("p", "\"b\""), ("s", "\"a;b\"")])]
        );
        assert_eq!(apply("?p apf:strSplit (?s \";\")", &[("s", lit("a;b")), ("p", lit("c"))]).unwrap().len(), 0);
        assert_eq!(apply("?p apf:strSplit (?s \";\")", &[]).unwrap().len(), 0);
        // concat: lexical forms and IRIs; nothing if an argument is unbound
        assert_eq!(apply("?c apf:concat (\"x\" 1 <http://ex/y> \"z\"@en)", &[]).unwrap(), [pairs(&[("c", "\"x1http://ex/yz\"")])]);
        assert_eq!(apply("?c apf:concat (\"x\" ?missing)", &[]).unwrap().len(), 0);
        // splitIRI: the namespace an IRI, the local name a string; nothing for an unbound subject
        assert_eq!(
            apply("<http://example.com/ns#thing> apf:splitIRI (?ns ?ln)", &[]).unwrap(),
            [pairs(&[("ln", "\"thing\""), ("ns", "<http://example.com/ns#>")])]
        );
        assert_eq!(apply("?i apf:splitURI (?ns ?ln)", &[]).unwrap().len(), 0);
        assert_eq!(apply("<http://ex/a> apf:splitIRI (?ns \"a\")", &[]).unwrap(), [pairs(&[("ns", "<http://ex/>")])]);
        assert_eq!(apply("<http://ex/a> apf:splitIRI (?ns \"b\")", &[]).unwrap().len(), 0);
        // str and assign
        assert_eq!(apply("?s apf:str <http://ex/w>", &[]).unwrap(), [pairs(&[("s", "\"http://ex/w\"")])]);
        assert!(apply("?s apf:str ?o", &[]).is_err());
        assert_eq!(apply("?a apf:assign 42", &[]).unwrap(), [pairs(&[("a", "\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>")])]);
        assert_eq!(apply("\"back\" apf:assign ?b", &[]).unwrap(), [pairs(&[("b", "\"back\"")])]);
        assert_eq!(apply("1 apf:assign 1.0", &[]).unwrap().len(), 1, "the same value");
        assert!(apply("?a apf:assign ?b", &[]).is_err());
    }

    #[test]
    fn what_cannot_be_evaluated_is_refused() {
        let error = |where_clause: &str| {
            let text = format!(
                "PREFIX apf: <http://jena.apache.org/ARQ/property#> PREFIX list: <http://jena.apache.org/ARQ/list#> SELECT * WHERE {{ {where_clause} }}"
            );
            let spargebra::Query::Select { pattern, .. } = spargebra::SparqlParser::new().parse_query(&text).unwrap() else {
                unreachable!()
            };
            staged(&pattern).unwrap_err().to_string()
        };
        assert!(error("?x list:member ?y").contains("list:member is not supported: it reads the data"));
        assert!(error("?x apf:textMatch \"x\"").contains("text index"));
        assert!(error("?x apf:nope ?y").contains("not one of Jena's property functions"));
        assert!(error("?p apf:strSplit (?s)").contains("apf:strSplit is written ?part apf:strSplit"));
        assert!(error("?p apf:strSplit (?s \"[\")").contains("apf:strSplit:"));
        assert!(error("OPTIONAL { ?p apf:strSplit (?s \";\") }").contains("top level of the WHERE clause"));
        // the old namespace is Jena's too
        let old = "PREFIX old: <http://jena.hpl.hp.com/ARQ/property#> SELECT * WHERE { ?p old:strSplit (?s \";\") }";
        let spargebra::Query::Select { pattern, .. } = spargebra::SparqlParser::new().parse_query(old).unwrap() else { unreachable!() };
        assert!(staged(&pattern).unwrap().is_some());
    }
}
