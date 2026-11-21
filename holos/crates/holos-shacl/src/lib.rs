//! HOLOS L4 — SHACL validation over the store's own indexes.
//!
//! `DESIGN.md` §8: take the SHACL_Engine design — compile-once flat IR, integer-interned
//! terms, sorted index access, deterministic reports — and change one thing. **The
//! validator reads the store's dictionary and indexes instead of loading a private copy.**
//!
//! That single change is what the layer exists to test. SHACL_Engine's own benchmarks
//! have loading exceeding validation by roughly threefold at 100k instances; a validator
//! that shares the store's ids does not load at all.
//!
//! Two things follow that a library cannot do:
//!
//! - **A data graph is selected, not flattened.** [`Options::data_graph`] takes a
//!   [`GraphFilter`], so a holon's Boundary can validate its own scene (§9).
//! - **Revalidation can be incremental.** [`incremental`] derives the focus nodes a delta
//!   could have affected, which is what makes validation affordable on the write path.
//!
//! # Scope
//!
//! SHACL Core, evaluated here. SHACL-SPARQL constraints and custom constraint components
//! are evaluated by the adapted engine and merged into the same report
//! ([`CompiledShapes::has_sparql`]). Not evaluated, and refused rather than dropped: SPARQL
//! and node-expression targets, and SHACL 1.2 node expressions. SHACL-AF rules are not
//! constraints and are `holos_holon::Rules`'s to fire. Exact conformance numbers come from the
//! W3C suite rather than from this paragraph — see `crates/holos-conformance`, whose
//! `shacl-sparql` ratchets run SHACL-SPARQL through this validator.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::missing_errors_doc)]

pub mod access;
pub mod bridge;
pub mod engine;
pub mod incremental;
pub mod ir;
pub mod report;
pub mod validate;
pub mod vocab;

pub use access::GraphView;
pub use ir::{Compiler, Constraint, Shape, ShapeIdx, Shapes};
pub use validate::Validator;
pub use vocab::Sh;

use holos_core::TermId;
use holos_store::{GraphFilter, Store};

const SH: &str = "http://www.w3.org/ns/shacl#";

/// Whether the shapes graph has what only the adapted engine evaluates: a SHACL-SPARQL
/// constraint, or a custom constraint component (declared as one, or given a SPARQL
/// validator). None of these IRIs is in the well-known table, so each is looked up.
fn needs_adapted_engine(graph: GraphView<'_>, sh: &Sh) -> Result<bool, ShaclError> {
    let id = |local: &str| graph.id(oxrdf::NamedNodeRef::new_unchecked(&format!("{SH}{local}")));
    for predicate in ["sparql", "validator", "nodeValidator", "propertyValidator"] {
        if let Some(p) = id(predicate)? {
            if !graph.subjects_of(p)?.is_empty() {
                return Ok(true);
            }
        }
    }
    if let Some(class) = id("ConstraintComponent")? {
        if !graph.subjects(sh.rdf_type, class)?.is_empty() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Refuses a `sh:targetNode` that is a SHACL 1.2 node expression (`[ sh:select … ]`,
/// `[ sh:path … ]` …) rather than a node: this evaluator would take the expression's blank
/// node for the target, select nothing, and report conforms for data it never looked at.
fn refuse_node_expression_targets(graph: GraphView<'_>, sh: &Sh) -> Result<(), ShaclError> {
    for target in graph.objects_of(sh.target_node)? {
        if access::node_kind(target) != access::NodeKind::BlankNode {
            continue;
        }
        for predicate in graph.predicates_of(target)? {
            if let Some(oxrdf::Term::NamedNode(p)) = graph.term(predicate)? {
                if let Some(local) = p.as_str().strip_prefix(SH) {
                    return Err(ShaclError::Unsupported(format!(
                        "a sh:targetNode written as a node expression (sh:{local}) is not \
                         implemented by the native evaluator; validate with the adapted engine \
                         instead"
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Anything that can go wrong compiling or evaluating shapes.
#[derive(Debug, thiserror::Error)]
pub enum ShaclError {
    /// The storage layer could not answer.
    #[error(transparent)]
    Storage(#[from] holos_store::StorageError),
    /// The shapes graph does not describe valid shapes.
    #[error("ill-formed shapes graph: {0}")]
    IllFormedShape(String),
    /// A construct this validator does not implement yet.
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// Shape references nested past the limit.
    #[error("shape references nest more than {0} deep")]
    TooDeep(u32),
}

/// One violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationResult {
    /// The node the shape was checked at.
    pub focus_node: TermId,
    /// `sh:resultPath` — the `sh:path` of the reporting property shape, if any.
    pub path: Option<TermId>,
    /// `sh:value` — the value that failed, where the constraint names one.
    pub value: Option<TermId>,
    /// `sh:sourceShape`.
    pub source_shape: TermId,
    /// `sh:sourceConstraintComponent`.
    pub component: TermId,
    /// `sh:sourceConstraint` — the node that *stated* the constraint, where that is a thing
    /// distinct from the shape carrying it.
    ///
    /// Most constraints are written inline and have no such node. A node expression does:
    /// the expression is a term in the shapes graph, and a reader needs it to know which
    /// expression failed.
    pub source_constraint: Option<TermId>,
    /// `sh:resultSeverity`.
    pub severity: TermId,
    /// `sh:resultMessage`, in shapes-graph order.
    pub messages: Vec<TermId>,
    /// `sh:detail` — results that explain this one.
    ///
    /// A constraint whose violation is about a *structure* rather than a value reports the
    /// structure and then, underneath, what was wrong inside it: `sh:uniqueMembers` names
    /// the list and details the members that repeated. Nesting is what keeps the outer
    /// result about the thing the shape was checking.
    pub details: Vec<ValidationResult>,
}

/// The outcome of a validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Whether the data conforms.
    pub conforms: bool,
    /// Every violation found.
    pub results: Vec<ValidationResult>,
    /// `sh:conformanceDisallows` — the severities that make the data non-conforming.
    ///
    /// `None` is the default rule: everything from `sh:Info` upwards disqualifies, and
    /// `sh:Debug` and `sh:Trace` do not. A caller with a policy of its own says so here, and
    /// the report then states which rule it used rather than leaving a reader to assume.
    pub conformance_disallows: Option<Vec<TermId>>,
    /// The terms that [`holos_core::Tag::Ephemeral`] ids in the results stand for.
    ///
    /// A SHACL-SPARQL constraint can report a message or value that no graph in the store
    /// contains: a message with `{?value}` filled in, a count. The store's ids cannot name
    /// those, so the result names them by position in this list, and rendering the report
    /// resolves them here. Empty unless the shapes had SPARQL constraints.
    pub ephemeral: Vec<oxrdf::Term>,
}

impl Report {
    /// Builds a report from results.
    ///
    /// `sh:conforms` is **not** "no results". SHACL 1.2 adds `sh:Debug` and `sh:Trace`
    /// below `sh:Info`, and they are diagnostic rather than judgemental: a report may carry
    /// them and still say the data conforms. Everything from `sh:Info` upwards is a finding
    /// about the data and makes it non-conforming.
    ///
    /// The suite pins both halves of that. `severity-004` and `severity-005` carry a
    /// `sh:Debug` and a `sh:Trace` result respectively and expect `sh:conforms true`;
    /// `severity-003` carries a `sh:Warning` and expects `false`. Treating every result as
    /// disqualifying, or none below `sh:Violation` as disqualifying, gets one of those
    /// wrong.
    #[must_use]
    pub fn new(results: Vec<ValidationResult>) -> Self {
        Self {
            conforms: !results.iter().any(is_judgement),
            results,
            conformance_disallows: None,
            ephemeral: Vec::new(),
        }
    }

    /// Recomputes conformance against an explicit set of disqualifying severities.
    ///
    /// The default rule is a rule, not a law: a caller may decide that only `sh:Violation`
    /// disqualifies, and then a report full of warnings still conforms. Saying so here makes
    /// the report carry the rule it was judged by, which is the difference between "this
    /// data is fine" and "this data is fine *by these lights*".
    #[must_use]
    pub fn with_conformance_disallows(mut self, disallowed: Vec<TermId>) -> Self {
        self.conforms = !self
            .results
            .iter()
            .any(|r| disallowed.contains(&r.severity));
        self.conformance_disallows = Some(disallowed);
        self
    }

    /// A conforming report.
    #[must_use]
    pub fn conforming() -> Self {
        Self::new(Vec::new())
    }
}

/// Whether a result is a finding about the data rather than a diagnostic note.
///
/// Compared against the well-known ids directly rather than through a `Sh` vocabulary
/// struct, because [`Report::new`] is a plain constructor with no graph to resolve against —
/// and these two IRIs are compile-time constants precisely so that is possible.
fn is_judgement(result: &ValidationResult) -> bool {
    let diagnostic = [
        "http://www.w3.org/ns/shacl#Debug",
        "http://www.w3.org/ns/shacl#Trace",
    ]
    .into_iter()
    .filter_map(holos_core::vocab::encode_iri)
    .any(|id| id == result.severity);
    !diagnostic
}

/// Which graphs to read.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// The data graph to validate.
    ///
    /// The limitation §8 names in SHACL_Engine is that named graphs are flattened into
    /// one. Selecting a graph is what lets a holon validate its own scene.
    pub data_graph: GraphFilter,
    /// The graph the shapes are read from. Often the same as the data graph.
    pub shapes_graph: GraphFilter,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            data_graph: GraphFilter::Any,
            shapes_graph: GraphFilter::Any,
        }
    }
}

/// Something that can validate a store against shapes.
///
/// The seam exists so an external engine can be substituted for this one without the
/// holon Boundary (§9) knowing which it has.
pub trait Validate {
    /// Validates everything the shapes target.
    fn validate(&self, store: &Store) -> Result<Report, ShaclError>;
}

/// Compiled shapes, ready to validate a store repeatedly.
///
/// Holding this rather than a shapes graph *is* the compile-once property: constructing it
/// reads the shapes graph, and validating never does.
#[derive(Debug, Clone)]
pub struct CompiledShapes {
    shapes: Shapes,
    options: Options,
    sh: Sh,
    /// Whether any shape has a SHACL-SPARQL constraint, which the adapted engine evaluates.
    sparql: bool,
}

impl CompiledShapes {
    /// Compiles the shapes a store holds.
    pub fn compile(store: &Store, options: Options) -> Result<Self, ShaclError> {
        let sh = Sh::new();
        let graph = GraphView::new(store, options.shapes_graph);
        let shapes = Compiler::new(graph, &sh).compile()?;
        refuse_node_expression_targets(graph, &sh)?;
        let sparql = needs_adapted_engine(graph, &sh)?;
        Ok(Self {
            shapes,
            options,
            sh,
            sparql,
        })
    }

    /// The compiled shapes.
    #[must_use]
    pub fn shapes(&self) -> &Shapes {
        &self.shapes
    }

    /// The vocabulary these shapes were compiled against.
    #[must_use]
    pub fn vocabulary(&self) -> &Sh {
        &self.sh
    }

    /// The graph selection.
    #[must_use]
    pub fn options(&self) -> Options {
        self.options
    }

    /// Validates a store.
    ///
    /// SHACL Core here, over the store's own indexes. SHACL-SPARQL constraints, if the shapes
    /// have any, by the adapted engine over a bridged snapshot, with its results merged in:
    /// see [`CompiledShapes::has_sparql`].
    pub fn validate(&self, store: &Store) -> Result<Report, ShaclError> {
        let report = self.validator(store).validate_all()?;
        self.with_sparql(store, report)
    }

    /// Whether the shapes have SHACL-SPARQL constraints or custom constraint components.
    ///
    /// This evaluator implements neither. It used to refuse `sh:sparql`, since dropping a
    /// constraint silently would open the gate this evaluator keeps on the write path, and
    /// custom components went unnoticed, their parameters being in the author's namespace,
    /// so a shapes graph declaring one was validated without it. Now both go to the adapted
    /// engine, which implements them (SPARQL-based validators, pre-binding, the shared
    /// function library), and what it reports for them is merged with what this evaluator
    /// reports for SHACL Core. So every validation and revalidation with such shapes bridges
    /// the graph once and evaluates those constraints in full, whatever the delta: correct,
    /// and visibly more expensive than SHACL Core alone.
    #[must_use]
    pub fn has_sparql(&self) -> bool {
        self.sparql
    }

    /// `report` with the SHACL-SPARQL constraints' results added, if there are any.
    fn with_sparql(&self, store: &Store, report: Report) -> Result<Report, ShaclError> {
        if !self.sparql {
            return Ok(report);
        }
        let mut run = engine::EngineRun::prepare(store, self.options)?;
        let mut ephemeral: Vec<oxrdf::Term> = Vec::new();
        let mut results = report.results;
        {
            let mut id = |(known, term): &engine::Reported| -> Result<TermId, ShaclError> {
                if let Some(id) = known {
                    return Ok(*id);
                }
                if let Some(id) = store.lookup_term(term.as_ref())? {
                    return Ok(id);
                }
                let i = if let Some(i) = ephemeral.iter().position(|t| t == term) {
                    i
                } else {
                    ephemeral.push(term.clone());
                    ephemeral.len() - 1
                };
                Ok(TermId::new(holos_core::Tag::Ephemeral, i as u64))
            };
            for r in run.sparql_results()? {
                let focus_node = id(&r.focus)?;
                let path = match &r.path {
                    Some(p) => Some(id(p)?),
                    None => None,
                };
                let value = match &r.value {
                    Some(v) => Some(id(v)?),
                    None => None,
                };
                let source_shape = match &r.shape {
                    Some(shape) => id(shape)?,
                    None => focus_node,
                };
                let source_constraint = match &r.constraint {
                    Some(c) => Some(id(c)?),
                    None => None,
                };
                let mut messages = Vec::with_capacity(r.messages.len());
                for m in &r.messages {
                    messages.push(id(m)?);
                }
                results.push(ValidationResult {
                    focus_node,
                    path,
                    value,
                    source_shape,
                    component: id(&r.component)?,
                    source_constraint,
                    severity: id(&r.severity)?,
                    messages,
                    details: Vec::new(),
                });
            }
        }
        let mut merged = Report::new(results);
        if let Some(disallowed) = report.conformance_disallows {
            merged = merged.with_conformance_disallows(disallowed);
        }
        merged.ephemeral = ephemeral;
        Ok(merged)
    }

    /// Revalidates only what a delta could have affected.
    ///
    /// The cost tracks the size of the change rather than the size of the graph, which is
    /// what makes SHACL affordable on the write path (`DESIGN.md` §8).
    pub fn revalidate(
        &self,
        store: &Store,
        changes: &[incremental::Change],
    ) -> Result<Report, ShaclError> {
        let validator = self.validator(store);
        let data = GraphView::new(store, self.options.data_graph);
        let rdf_type = self.sh.rdf_type;

        let plan = incremental::plan(&self.shapes, &self.sh, changes, |node| {
            data.objects(node, rdf_type).unwrap_or_default()
        });

        // A change almost always implicates an *anonymous* property shape, which has no
        // targets and is only ever evaluated through its parent. Attributing the work
        // upwards to a shape that does have targets is what makes the change visible at
        // all; without it, revalidation silently finds nothing.
        let mut attributed: rustc_hash::FxHashSet<(ShapeIdx, TermId)> =
            rustc_hash::FxHashSet::default();
        let mut implicated: rustc_hash::FxHashSet<ShapeIdx> = rustc_hash::FxHashSet::default();
        // Shapes whose violation can sit at a node the delta never mentions, so the
        // endpoint attribution below cannot reach it.
        let mut widen: rustc_hash::FxHashSet<ShapeIdx> = rustc_hash::FxHashSet::default();
        for (idx, node) in plan.work() {
            let upstream = self.shapes.focus_may_be_upstream(idx);
            for ancestor in self.shapes.targeted_ancestors(idx) {
                implicated.insert(ancestor);
                attributed.insert((ancestor, node));
                if upstream {
                    widen.insert(ancestor);
                }
            }
        }

        // The candidate focus nodes are the endpoints of changed quads, which is an
        // over-approximation in one direction and an under-approximation in the other:
        // a shape reached down a property path has its focus node further upstream. So
        // trim what the targets do not select, then widen any shape left with nothing to
        // do back to all of its focus nodes. Over-reporting costs time; under-reporting
        // would let an invalid graph through.
        let mut focus_cache: rustc_hash::FxHashMap<ShapeIdx, Vec<TermId>> =
            rustc_hash::FxHashMap::default();
        for idx in &implicated {
            focus_cache.insert(*idx, validator.focus_nodes(*idx).unwrap_or_default());
        }
        attributed.retain(|(idx, focus)| {
            focus_cache
                .get(idx)
                .is_some_and(|nodes| nodes.binary_search(focus).is_ok())
        });
        // Widen a shape that got nothing — and one whose focus node can be upstream of the
        // change, which the endpoint attribution reaches only by accident. Without the
        // second case a compound path finds the violation at the node that changed and
        // misses the one at the node whose path runs through it: a full validation rejects
        // the graph and the write path admits it.
        for idx in &implicated {
            if !widen.contains(idx) && attributed.iter().any(|(i, _)| i == idx) {
                continue;
            }
            for focus in focus_cache.get(idx).into_iter().flatten() {
                attributed.insert((*idx, *focus));
            }
        }

        let mut work: Vec<(ShapeIdx, TermId)> = attributed.into_iter().collect();
        work.sort_unstable();
        let report = validator.validate_selected(&work)?;
        // SPARQL constraints are not attributed to a delta: any change can alter what a
        // query finds, so they are evaluated in full.
        self.with_sparql(store, report)
    }

    /// Renders a report as RDF, deterministically.
    pub fn report_to_quads(
        &self,
        store: &Store,
        report: &Report,
    ) -> Result<Vec<oxrdf::Quad>, ShaclError> {
        report::to_quads(
            report,
            GraphView::new(store, self.options.shapes_graph),
            &self.shapes,
            &self.sh,
        )
    }

    fn validator<'a>(&'a self, store: &'a Store) -> Validator<'a> {
        Validator::new(
            &self.shapes,
            GraphView::new(store, self.options.data_graph),
            GraphView::new(store, self.options.shapes_graph),
            &self.sh,
        )
    }
}

impl Validate for CompiledShapes {
    fn validate(&self, store: &Store) -> Result<Report, ShaclError> {
        Self::validate(self, store)
    }
}
