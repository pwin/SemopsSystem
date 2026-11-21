//! Property functions in queries against the store: `?tag apf:strSplit (?tags ";")`,
//! `?list list:member ?item`, `?container rdfs:member ?member` and the rest
//! (`semops_functions::pfunc` has the list and what each one does).
//!
//! `spareval` has no notion of a property function: to it `apf:strSplit` is a predicate like
//! any other, and the pattern quietly matches nothing. So before evaluation they are taken
//! out of the query, in one of two ways.
//!
//! - **Readers of the data** become ordinary patterns. `list:member` is the property path
//!   `rdf:rest*/rdf:first`; `list:index` and `list:length` are counts over that path;
//!   `rdfs:member` and `apf:bag`/`seq`/`alt` match the container membership properties
//!   `rdf:_1`, `rdf:_2` …, and `rdfs:member` matches statements that use it as well.
//! - **Functions of their arguments** (`apf:strSplit`, `apf:concat`, `apf:splitIRI`,
//!   `apf:str`, `apf:assign`, `apf:blankNode`, `spif:split`) are evaluated where they
//!   stand, as Jena evaluates them: the patterns of the group before one are evaluated, the
//!   function is applied to each solution, and the group continues from what it gives, as a
//!   `VALUES` table. That is the staged evaluation tarka does for a row, done here over
//!   the store, at any depth: inside `OPTIONAL`, `UNION`, `GRAPH` and sub-queries too.
//!
//! A `VALUES` table cannot hold a blank node, and the solutions often do. Such a value goes
//! into the table as a placeholder literal and is turned back into the store's blank node by
//! an internal function ([`BLANK_NODE_FUNCTION`]) on the way out, so a later pattern joins
//! on it as it would have on the original.
//!
//! What is refused, with an error naming it: a property function inside `FILTER EXISTS`, or
//! inside a sub-query inside `GRAPH`, where staging would change what the pattern means.

use crate::EngineError;
use oxrdf::vocab::rdf;
use oxrdf::{BlankNode, Literal, NamedNode, Term};
use semops_functions::pfunc::{self as shared, Arg, Bindings, Kind, PropertyFunction, Regex};
use spargebra::algebra::{
    AggregateExpression, AggregateFunction, Expression, Function, GraphPattern,
    PropertyPathExpression,
};
use spargebra::term::{GroundTerm, NamedNodePattern, TermPattern, TriplePattern, Variable};
use spargebra::Query;
use std::collections::{BTreeSet, HashMap};

/// The internal function that gives back a store blank node from its label. Registered by
/// [`crate::Engine::evaluator`]; nothing outside this module writes calls to it.
pub const BLANK_NODE_FUNCTION: &str = "urn:semops:engine#blank-node";
const BLANK_NODE_LABEL: &str = "urn:semops:engine#blank-node-label";
const RDF_MEMBERSHIP: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#_";

/// [`BLANK_NODE_FUNCTION`]: the blank node with this label.
pub(crate) fn blank_node(args: &[Term]) -> Option<Term> {
    match args {
        [Term::Literal(l)] => BlankNode::new(l.value()).ok().map(Into::into),
        _ => None,
    }
}

/// What staging asks for: the solutions of a pattern with no property functions left in it,
/// evaluated against the same dataset as the query.
pub(crate) type Evaluate<'e> = dyn FnMut(&GraphPattern) -> Result<Vec<Bindings>, EngineError> + 'e;

/// Whether the query has a property function anywhere outside a `SERVICE`.
pub(crate) fn in_query(query: &Query) -> bool {
    match query {
        Query::Select { pattern, .. }
        | Query::Construct { pattern, .. }
        | Query::Describe { pattern, .. }
        | Query::Ask { pattern, .. } => mentions(pattern),
    }
}

/// The query with its property functions taken out (see the module documentation).
pub(crate) fn stage_query(query: Query, evaluate: &mut Evaluate<'_>) -> Result<Query, EngineError> {
    let mut stager = Stager {
        evaluate,
        regexes: HashMap::new(),
        fresh: 0,
    };
    Ok(match query {
        Query::Select {
            dataset,
            pattern,
            base_iri,
        } => Query::Select {
            pattern: stager.pattern(&pattern, None)?,
            dataset,
            base_iri,
        },
        Query::Construct {
            template,
            dataset,
            pattern,
            base_iri,
        } => Query::Construct {
            template,
            pattern: stager.pattern(&pattern, None)?,
            dataset,
            base_iri,
        },
        Query::Describe {
            dataset,
            pattern,
            base_iri,
        } => Query::Describe {
            pattern: stager.pattern(&pattern, None)?,
            dataset,
            base_iri,
        },
        Query::Ask {
            dataset,
            pattern,
            base_iri,
        } => Query::Ask {
            pattern: stager.pattern(&pattern, None)?,
            dataset,
            base_iri,
        },
    })
}

/// A graph pattern with its property functions taken out: an update's `WHERE`.
pub(crate) fn stage_pattern(
    pattern: &GraphPattern,
    evaluate: &mut Evaluate<'_>,
) -> Result<GraphPattern, EngineError> {
    Stager {
        evaluate,
        regexes: HashMap::new(),
        fresh: 0,
    }
    .pattern(pattern, None)
}

/// Whether a pattern has a property function anywhere outside a `SERVICE`.
pub(crate) fn mentions(p: &GraphPattern) -> bool {
    match p {
        GraphPattern::Bgp { patterns } => shared::mentions_any(patterns),
        GraphPattern::Join { left, right }
        | GraphPattern::LeftJoin { left, right, .. }
        | GraphPattern::Union { left, right }
        | GraphPattern::Minus { left, right } => mentions(left) || mentions(right),
        GraphPattern::Filter { expr, inner } => mentions(inner) || in_exists(expr),
        GraphPattern::Extend {
            inner, expression, ..
        } => mentions(inner) || in_exists(expression),
        GraphPattern::Graph { inner, .. }
        | GraphPattern::Project { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::OrderBy { inner, .. }
        | GraphPattern::Group { inner, .. } => mentions(inner),
        _ => false,
    }
}

/// Whether an expression has an `EXISTS` with a property function in it.
fn in_exists(e: &Expression) -> bool {
    let mut found = false;
    visit_exists(e, &mut |p| found |= mentions(p));
    found
}

fn has_exists(e: &Expression) -> bool {
    let mut found = false;
    visit_exists(e, &mut |_| found = true);
    found
}

fn visit_exists(e: &Expression, f: &mut dyn FnMut(&GraphPattern)) {
    match e {
        Expression::Exists(p) => f(p),
        Expression::Or(a, b)
        | Expression::And(a, b)
        | Expression::Equal(a, b)
        | Expression::SameTerm(a, b)
        | Expression::Greater(a, b)
        | Expression::GreaterOrEqual(a, b)
        | Expression::Less(a, b)
        | Expression::LessOrEqual(a, b)
        | Expression::Add(a, b)
        | Expression::Subtract(a, b)
        | Expression::Multiply(a, b)
        | Expression::Divide(a, b) => {
            visit_exists(a, f);
            visit_exists(b, f);
        }
        Expression::In(a, list) => {
            visit_exists(a, f);
            for x in list {
                visit_exists(x, f);
            }
        }
        Expression::UnaryPlus(a) | Expression::UnaryMinus(a) | Expression::Not(a) => {
            visit_exists(a, f);
        }
        Expression::If(a, b, c) => {
            visit_exists(a, f);
            visit_exists(b, f);
            visit_exists(c, f);
        }
        Expression::Coalesce(list) | Expression::FunctionCall(_, list) => {
            for x in list {
                visit_exists(x, f);
            }
        }
        _ => {}
    }
}

fn refused(what: &str) -> EngineError {
    EngineError::BadRequest(format!("a property function {what} is not supported"))
}

enum Step {
    Extend(Variable, Expression),
    Join(GraphPattern),
    LeftJoin(GraphPattern, Option<Expression>),
    Minus(GraphPattern),
    Function(PropertyFunction),
}

struct Stager<'s, 'e> {
    evaluate: &'s mut Evaluate<'e>,
    regexes: HashMap<String, Regex>,
    fresh: usize,
}

fn empty() -> GraphPattern {
    GraphPattern::Bgp {
        patterns: Vec::new(),
    }
}

fn in_graph(p: GraphPattern, graph: Option<&NamedNodePattern>) -> GraphPattern {
    match graph {
        None => p,
        Some(name) => GraphPattern::Graph {
            name: name.clone(),
            inner: Box::new(p),
        },
    }
}

impl Stager<'_, '_> {
    fn fresh(&mut self, what: &str) -> Variable {
        self.fresh += 1;
        Variable::new_unchecked(format!("__semops_{what}{}", self.fresh))
    }

    /// `p` with its property functions taken out. Inside `GRAPH`, every piece that reads the
    /// data is wrapped in the graph, since a staged table cannot be.
    fn pattern(
        &mut self,
        p: &GraphPattern,
        graph: Option<&NamedNodePattern>,
    ) -> Result<GraphPattern, EngineError> {
        if !mentions(p) {
            return Ok(in_graph(p.clone(), graph));
        }
        Ok(match p {
            GraphPattern::Bgp { .. }
            | GraphPattern::Join { .. }
            | GraphPattern::LeftJoin { .. }
            | GraphPattern::Minus { .. }
            | GraphPattern::Extend { .. } => self.chain(p, graph)?,
            GraphPattern::Filter { expr, inner } => {
                if in_exists(expr) {
                    return Err(refused("inside FILTER EXISTS"));
                }
                if graph.is_some() && has_exists(expr) {
                    return Err(refused("in a GRAPH group with a FILTER EXISTS"));
                }
                GraphPattern::Filter {
                    expr: expr.clone(),
                    inner: Box::new(self.pattern(inner, graph)?),
                }
            }
            GraphPattern::Union { left, right } => GraphPattern::Union {
                left: Box::new(self.pattern(left, graph)?),
                right: Box::new(self.pattern(right, graph)?),
            },
            // a GRAPH inside a GRAPH names the graph its group reads
            GraphPattern::Graph { name, inner } => self.pattern(inner, Some(name))?,
            _ if graph.is_some() => return Err(refused("in a sub-query inside GRAPH")),
            GraphPattern::Project { inner, variables } => GraphPattern::Project {
                inner: Box::new(self.pattern(inner, None)?),
                variables: variables.clone(),
            },
            GraphPattern::Distinct { inner } => GraphPattern::Distinct {
                inner: Box::new(self.pattern(inner, None)?),
            },
            GraphPattern::Reduced { inner } => GraphPattern::Reduced {
                inner: Box::new(self.pattern(inner, None)?),
            },
            GraphPattern::Slice {
                inner,
                start,
                length,
            } => GraphPattern::Slice {
                inner: Box::new(self.pattern(inner, None)?),
                start: *start,
                length: *length,
            },
            GraphPattern::OrderBy { inner, expression } => GraphPattern::OrderBy {
                inner: Box::new(self.pattern(inner, None)?),
                expression: expression.clone(),
            },
            GraphPattern::Group {
                inner,
                variables,
                aggregates,
            } => GraphPattern::Group {
                inner: Box::new(self.pattern(inner, None)?),
                variables: variables.clone(),
                aggregates: aggregates.clone(),
            },
            _ => return Err(refused("here")),
        })
    }

    /// A group, step by step: each property function sees what the steps before it bound.
    fn chain(
        &mut self,
        p: &GraphPattern,
        graph: Option<&NamedNodePattern>,
    ) -> Result<GraphPattern, EngineError> {
        let mut steps = Vec::new();
        self.linear(p, &mut steps)?;
        let mut acc: Option<GraphPattern> = None;
        for step in steps {
            acc = Some(match step {
                Step::Function(f) => {
                    let solutions = match &acc {
                        None => vec![Bindings::new()],
                        Some(a) => (self.evaluate)(a)?,
                    };
                    let mut out = Vec::new();
                    for mu in &solutions {
                        out.extend(
                            f.apply(mu, &mut self.regexes)
                                .map_err(EngineError::BadRequest)?,
                        );
                    }
                    table(&out)?
                }
                Step::Extend(variable, expression) => {
                    if graph.is_some() && has_exists(&expression) {
                        return Err(refused("in a GRAPH group with a BIND over EXISTS"));
                    }
                    GraphPattern::Extend {
                        inner: Box::new(acc.unwrap_or_else(empty)),
                        variable,
                        expression,
                    }
                }
                Step::Join(sub) => {
                    let sub = self.pattern(&sub, graph)?;
                    match acc {
                        None => sub,
                        Some(a) => GraphPattern::Join {
                            left: Box::new(a),
                            right: Box::new(sub),
                        },
                    }
                }
                Step::LeftJoin(sub, expression) => GraphPattern::LeftJoin {
                    left: Box::new(acc.unwrap_or_else(empty)),
                    right: Box::new(self.pattern(&sub, graph)?),
                    expression,
                },
                Step::Minus(sub) => GraphPattern::Minus {
                    left: Box::new(acc.unwrap_or_else(empty)),
                    right: Box::new(self.pattern(&sub, graph)?),
                },
            });
        }
        Ok(acc.unwrap_or_else(|| in_graph(empty(), graph)))
    }

    fn linear(&mut self, p: &GraphPattern, steps: &mut Vec<Step>) -> Result<(), EngineError> {
        match p {
            GraphPattern::Extend {
                inner,
                variable,
                expression,
            } => {
                if in_exists(expression) {
                    return Err(refused("inside EXISTS"));
                }
                self.linear(inner, steps)?;
                steps.push(Step::Extend(variable.clone(), expression.clone()));
            }
            GraphPattern::Join { left, right } => {
                self.linear(left, steps)?;
                self.part(right, steps)?;
            }
            GraphPattern::LeftJoin {
                left,
                right,
                expression,
            } => {
                self.linear(left, steps)?;
                steps.push(Step::LeftJoin((**right).clone(), expression.clone()));
            }
            GraphPattern::Minus { left, right } => {
                self.linear(left, steps)?;
                steps.push(Step::Minus((**right).clone()));
            }
            other => self.part(other, steps)?,
        }
        Ok(())
    }

    /// One pattern of a group. A basic graph pattern's property functions become steps of
    /// their own, after the rest of its triples; readers of the data become patterns.
    fn part(&mut self, p: &GraphPattern, steps: &mut Vec<Step>) -> Result<(), EngineError> {
        let GraphPattern::Bgp { patterns } = p else {
            steps.push(Step::Join(p.clone()));
            return Ok(());
        };
        let (functions, rest) =
            shared::split_bgp(patterns, |_| None).map_err(EngineError::BadRequest)?;
        // A blank node in a pattern is a variable nobody can name: renamed to one, so that the
        // same one in a property function's arguments still joins.
        let rest: Vec<TriplePattern> = rest
            .into_iter()
            .map(|t| TriplePattern {
                subject: unblank(&t.subject),
                predicate: t.predicate,
                object: unblank(&t.object),
            })
            .collect();
        if !rest.is_empty() {
            steps.push(Step::Join(GraphPattern::Bgp { patterns: rest }));
        }
        for mut f in functions {
            f.subject = unblank_arg(&f.subject);
            f.object = unblank_arg(&f.object);
            if f.kind.reads_data() {
                steps.push(Step::Join(self.reader(&f)?));
            } else {
                steps.push(Step::Function(f));
            }
        }
        Ok(())
    }

    /// A reader of the data as the pattern that reads it. One arm per function, each the
    /// SPARQL that function means, which is why it is long.
    #[allow(clippy::too_many_lines)]
    fn reader(&mut self, f: &PropertyFunction) -> Result<GraphPattern, EngineError> {
        let one = |a: &Arg| match a {
            Arg::One(t) => Ok(t.clone()),
            Arg::List(_) => Err(EngineError::BadRequest(format!(
                "{}: unexpected list argument",
                f.name
            ))),
        };
        let first = PropertyPathExpression::NamedNode(rdf::FIRST.into_owned());
        let rest_star = || {
            PropertyPathExpression::ZeroOrMore(Box::new(PropertyPathExpression::NamedNode(
                rdf::REST.into_owned(),
            )))
        };
        let subject = one(&f.subject)?;
        Ok(match f.kind {
            Kind::ListMember => GraphPattern::Path {
                subject,
                path: PropertyPathExpression::Sequence(Box::new(rest_star()), Box::new(first)),
                object: one(&f.object)?,
            },
            Kind::ListLength => {
                let (cell, any) = (self.fresh("cell"), self.fresh("item"));
                let (length, check) = self.output(&one(&f.object)?, "length");
                let inner = GraphPattern::Join {
                    left: Box::new(GraphPattern::Path {
                        subject: subject.clone(),
                        path: rest_star(),
                        object: cell.clone().into(),
                    }),
                    right: Box::new(bgp(vec![triple(
                        cell.clone().into(),
                        rdf::FIRST.into_owned(),
                        any.into(),
                    )])),
                };
                let group_by: Vec<Variable> = variable_of(&subject).into_iter().collect();
                let counted = count_into(inner, group_by.clone(), length.clone(), cell);
                // a length written as a constant is tested, not bound
                let keep = group_by
                    .into_iter()
                    .chain(check.is_none().then_some(length))
                    .collect();
                finish(counted, keep, check)
            }
            Kind::ListIndex => {
                let Arg::List(args) = &f.object else {
                    unreachable!("checked by split_bgp")
                };
                let (cell, mid) = (self.fresh("cell"), self.fresh("before"));
                let member = args[1].clone();
                let (index, check) = self.output(&args[0], "index");
                let rest_plus = PropertyPathExpression::OneOrMore(Box::new(
                    PropertyPathExpression::NamedNode(rdf::REST.into_owned()),
                ));
                let cells = GraphPattern::Join {
                    left: Box::new(GraphPattern::Path {
                        subject: subject.clone(),
                        path: rest_star(),
                        object: cell.clone().into(),
                    }),
                    right: Box::new(bgp(vec![triple(
                        cell.clone().into(),
                        rdf::FIRST.into_owned(),
                        member.clone(),
                    )])),
                };
                // the cells before this one: its index
                let before = GraphPattern::Join {
                    left: Box::new(GraphPattern::Path {
                        subject: subject.clone(),
                        path: rest_star(),
                        object: mid.clone().into(),
                    }),
                    right: Box::new(GraphPattern::Path {
                        subject: mid.clone().into(),
                        path: rest_plus,
                        object: cell.clone().into(),
                    }),
                };
                let inner = GraphPattern::LeftJoin {
                    left: Box::new(cells),
                    right: Box::new(before),
                    expression: None,
                };
                let mut group_by: Vec<Variable> = variable_of(&subject).into_iter().collect();
                group_by.push(cell);
                group_by.extend(variable_of(&member));
                let counted = count_into(inner, group_by.clone(), index.clone(), mid);
                let keep: Vec<Variable> = variable_of(&subject)
                    .into_iter()
                    .chain(check.is_none().then_some(index))
                    .chain(variable_of(&member))
                    .collect();
                finish(counted, keep, check)
            }
            Kind::Member => {
                let object = one(&f.object)?;
                let both = GraphPattern::Union {
                    // Statements that use rdfs:member, as a one-step path: written as a triple
                    // it would be this property function again.
                    left: Box::new(GraphPattern::Path {
                        subject: subject.clone(),
                        path: PropertyPathExpression::NamedNode(NamedNode::new_unchecked(
                            shared::RDFS_MEMBER,
                        )),
                        object: object.clone(),
                    }),
                    right: Box::new(self.membership(&subject, &object)),
                };
                // Membership is a relation, not a count: under RDFS entailment rdfs12 states the
                // rdfs:member statement for every rdf:_n one, and each pair is still one answer.
                let keep: Vec<Variable> = variable_of(&subject)
                    .into_iter()
                    .chain(variable_of(&object))
                    .collect();
                GraphPattern::Distinct {
                    inner: Box::new(GraphPattern::Project {
                        inner: Box::new(both),
                        variables: keep,
                    }),
                }
            }
            Kind::Bag | Kind::Seq | Kind::Alt => {
                let class = match f.kind {
                    Kind::Bag => rdf::BAG,
                    Kind::Seq => rdf::SEQ,
                    _ => rdf::ALT,
                };
                GraphPattern::Join {
                    left: Box::new(bgp(vec![triple(
                        subject.clone(),
                        rdf::TYPE.into_owned(),
                        class.into_owned().into(),
                    )])),
                    right: Box::new(self.membership(&subject, &one(&f.object)?)),
                }
            }
            _ => unreachable!("only readers of the data come here"),
        })
    }

    /// `?container rdf:_n ?member`, for any n.
    fn membership(&mut self, subject: &TermPattern, object: &TermPattern) -> GraphPattern {
        let p = self.fresh("membership");
        let matched = GraphPattern::Filter {
            expr: Expression::FunctionCall(
                Function::StrStarts,
                vec![
                    Expression::FunctionCall(Function::Str, vec![Expression::Variable(p.clone())]),
                    Expression::Literal(Literal::new_simple_literal(RDF_MEMBERSHIP)),
                ],
            ),
            inner: Box::new(bgp(vec![TriplePattern {
                subject: subject.clone(),
                predicate: p.into(),
                object: object.clone(),
            }])),
        };
        let keep: Vec<Variable> = variable_of(subject)
            .into_iter()
            .chain(variable_of(object))
            .collect();
        GraphPattern::Project {
            inner: Box::new(matched),
            variables: keep,
        }
    }

    /// The variable a reader binds its result to: the one written, or a fresh one with a
    /// test that it equals the constant written.
    fn output(&mut self, written: &TermPattern, what: &str) -> (Variable, Option<Expression>) {
        match written {
            TermPattern::Variable(v) => (v.clone(), None),
            other => {
                let v = self.fresh(what);
                let constant = match other {
                    TermPattern::NamedNode(n) => Expression::NamedNode(n.clone()),
                    TermPattern::Literal(l) => Expression::Literal(l.clone()),
                    _ => Expression::Literal(Literal::new_simple_literal("")),
                };
                (
                    v.clone(),
                    Some(Expression::Equal(
                        Box::new(Expression::Variable(v)),
                        Box::new(constant),
                    )),
                )
            }
        }
    }
}

fn bgp(patterns: Vec<TriplePattern>) -> GraphPattern {
    GraphPattern::Bgp { patterns }
}

fn triple(subject: TermPattern, predicate: NamedNode, object: TermPattern) -> TriplePattern {
    TriplePattern {
        subject,
        predicate: predicate.into(),
        object,
    }
}

fn variable_of(t: &TermPattern) -> Option<Variable> {
    match t {
        TermPattern::Variable(v) => Some(v.clone()),
        _ => None,
    }
}

fn unblank(t: &TermPattern) -> TermPattern {
    match t {
        TermPattern::BlankNode(b) => {
            Variable::new_unchecked(format!("__semops_b_{}", b.as_str())).into()
        }
        other => other.clone(),
    }
}

fn unblank_arg(a: &Arg) -> Arg {
    match a {
        Arg::One(t) => Arg::One(unblank(t)),
        Arg::List(items) => Arg::List(items.iter().map(unblank).collect()),
    }
}

/// `COUNT(DISTINCT ?counted)` of `inner`, grouped, into `into`.
fn count_into(
    inner: GraphPattern,
    group_by: Vec<Variable>,
    into: Variable,
    counted: Variable,
) -> GraphPattern {
    GraphPattern::Group {
        inner: Box::new(inner),
        variables: group_by,
        aggregates: vec![(
            into,
            AggregateExpression::FunctionCall {
                name: AggregateFunction::Count,
                expr: Expression::Variable(counted),
                distinct: true,
            },
        )],
    }
}

/// A reader's result, keeping only the variables written, and checking a constant result.
fn finish(counted: GraphPattern, keep: Vec<Variable>, check: Option<Expression>) -> GraphPattern {
    let checked = match check {
        None => counted,
        Some(expr) => GraphPattern::Filter {
            expr,
            inner: Box::new(counted),
        },
    };
    GraphPattern::Project {
        inner: Box::new(checked),
        variables: keep,
    }
}

/// Solutions as a pattern: a `VALUES` table, with a blank node carried as a placeholder
/// literal in a hidden column and turned back into itself.
fn table(solutions: &[Bindings]) -> Result<GraphPattern, EngineError> {
    // A hidden column from an earlier stage is not carried again: its variable is.
    let variables: BTreeSet<&Variable> = solutions
        .iter()
        .flat_map(HashMap::keys)
        .filter(|v| !v.as_str().starts_with("__semops_blank_"))
        .collect();
    let variables: Vec<&Variable> = variables.into_iter().collect();
    let blank: Vec<bool> = variables
        .iter()
        .map(|v| {
            solutions
                .iter()
                .any(|mu| matches!(mu.get(*v), Some(Term::BlankNode(_))))
        })
        .collect();
    let hidden = |v: &Variable| Variable::new_unchecked(format!("__semops_blank_{}", v.as_str()));
    let columns: Vec<Variable> = variables
        .iter()
        .zip(&blank)
        .map(|(v, b)| if *b { hidden(v) } else { (*v).clone() })
        .collect();
    let label_type = NamedNode::new_unchecked(BLANK_NODE_LABEL);
    let mut rows = Vec::with_capacity(solutions.len());
    for mu in solutions {
        let mut row = Vec::with_capacity(variables.len());
        for v in &variables {
            row.push(match mu.get(*v) {
                None => None,
                Some(Term::BlankNode(b)) => Some(GroundTerm::Literal(Literal::new_typed_literal(b.as_str(), label_type.clone()))),
                Some(term) => Some(GroundTerm::try_from(term.clone()).map_err(|()| {
                    EngineError::BadRequest(format!(
                        "a property function's input binds ?{v} to {term}, which contains a blank node and cannot be carried to the next step"
                    ))
                })?),
            });
        }
        rows.push(row);
    }
    let mut pattern = GraphPattern::Values {
        variables: columns,
        bindings: rows,
    };
    for (v, b) in variables.iter().zip(&blank) {
        if !*b {
            continue;
        }
        let h = Expression::Variable(hidden(v));
        pattern = GraphPattern::Extend {
            inner: Box::new(pattern),
            variable: (*v).clone(),
            // isLiteral first: DATATYPE of an IRI is an error, which would unbind it
            expression: Expression::If(
                Box::new(Expression::And(
                    Box::new(Expression::FunctionCall(
                        Function::IsLiteral,
                        vec![h.clone()],
                    )),
                    Box::new(Expression::Equal(
                        Box::new(Expression::FunctionCall(
                            Function::Datatype,
                            vec![h.clone()],
                        )),
                        Box::new(Expression::NamedNode(label_type.clone())),
                    )),
                )),
                Box::new(Expression::FunctionCall(
                    Function::Custom(NamedNode::new_unchecked(BLANK_NODE_FUNCTION)),
                    vec![Expression::FunctionCall(Function::Str, vec![h.clone()])],
                )),
                Box::new(h),
            ),
        };
    }
    Ok(pattern)
}
