//! Version comparison: what changed between two versions of an ontology, and the
//! semantic-versioning bump the change calls for.
//!
//! A port of the Ontology Quality Suite's `versioning/diff.py` (0.28.0, d9300e6), with its
//! rules unchanged:
//!
//! - **Major**: a class or property removed, a subclass edge removed, a disjointness axiom
//!   added, a domain or range narrowed or changed incomparably, an equivalence axiom
//!   removed, or a restricting characteristic (functional, inverse functional, asymmetric,
//!   irreflexive) added.
//! - **Minor**: a class, property, subclass edge or equivalence axiom added, a domain or
//!   range widened, a disjointness axiom removed, or any other change of characteristic.
//! - **Patch**: nothing structural, but an `rdfs:label` or `rdfs:comment` differs.
//! - **None**: no difference in what is compared.
//!
//! Only named terms are compared, as in the suite: blank-node class expressions have no
//! identity across two parses. A domain or range is compared by the classes that satisfy
//! it, each class with its subclasses, so moving a domain from a class to its superclass is
//! a widening.

use oxrdf::{NamedOrBlankNode, Term, Triple};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_PROPERTY: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const OWL: &str = "http://www.w3.org/2002/07/owl#";

const PROPERTY_TYPES: [&str; 3] = ["ObjectProperty", "DatatypeProperty", "AnnotationProperty"];
const CHARACTERISTICS: [&str; 7] = [
    "FunctionalProperty",
    "InverseFunctionalProperty",
    "TransitiveProperty",
    "SymmetricProperty",
    "AsymmetricProperty",
    "ReflexiveProperty",
    "IrreflexiveProperty",
];
const RESTRICTING: [&str; 4] = [
    "FunctionalProperty",
    "InverseFunctionalProperty",
    "AsymmetricProperty",
    "IrreflexiveProperty",
];

type Iri = String;
type Set = BTreeSet<Iri>;
/// Two terms related symmetrically (disjoint, equivalent), stored in sorted order.
type Pair = (Iri, Iri);

/// The bump a change calls for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bump {
    None,
    Patch,
    Minor,
    Major,
}

impl Bump {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Patch => "patch",
            Self::Minor => "minor",
            Self::Major => "major",
        }
    }
}

/// The facts compared, from one version.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub classes: Set,
    pub properties: Set,
    pub subclass_edges: BTreeSet<(Iri, Iri)>,
    pub disjoint_pairs: BTreeSet<Pair>,
    pub equivalent_classes: BTreeSet<Pair>,
    pub equivalent_properties: BTreeSet<Pair>,
    pub domain: BTreeMap<Iri, Set>,
    pub range: BTreeMap<Iri, Set>,
    pub characteristics: BTreeMap<Iri, Set>,
    /// Each term's `rdfs:label` and `rdfs:comment` values, as (predicate, text).
    pub annotations: BTreeMap<Iri, BTreeSet<(Iri, String)>>,
}

fn named(subject: &NamedOrBlankNode) -> Option<&str> {
    match subject {
        NamedOrBlankNode::NamedNode(n) => Some(n.as_str()),
        NamedOrBlankNode::BlankNode(_) => None,
    }
}

fn named_object(object: &Term) -> Option<&str> {
    match object {
        Term::NamedNode(n) => Some(n.as_str()),
        _ => None,
    }
}

fn pair(a: &str, b: &str) -> Pair {
    if a <= b {
        (a.to_owned(), b.to_owned())
    } else {
        (b.to_owned(), a.to_owned())
    }
}

/// The text of an annotation value: a literal's lexical form, an IRI, or a blank node's
/// label, as the suite's `str(value)` gives them.
fn text(object: &Term) -> String {
    match object {
        Term::Literal(l) => l.value().to_owned(),
        Term::NamedNode(n) => n.as_str().to_owned(),
        Term::BlankNode(b) => b.as_str().to_owned(),
        #[allow(unreachable_patterns)]
        other => other.to_string(),
    }
}

/// The facts a version comparison looks at, from one version's triples.
#[must_use]
pub fn snapshot(triples: &[Triple]) -> Snapshot {
    let mut s = Snapshot::default();
    let owl = |local: &str| format!("{OWL}{local}");
    let rdfs = |local: &str| format!("{RDFS}{local}");
    let class_types = [owl("Class"), rdfs("Class")];
    let property_types: Vec<String> = PROPERTY_TYPES
        .iter()
        .map(|t| owl(t))
        .chain([RDF_PROPERTY.to_owned()])
        .collect();
    let characteristic_types: Vec<String> = CHARACTERISTICS.iter().map(|c| owl(c)).collect();
    let (sub_class_of, domain, range) = (rdfs("subClassOf"), rdfs("domain"), rdfs("range"));
    let (disjoint, eq_class, eq_property) = (
        owl("disjointWith"),
        owl("equivalentClass"),
        owl("equivalentProperty"),
    );

    for t in triples {
        let p = t.predicate.as_str();
        let (Some(subject), object) = (named(&t.subject), named_object(&t.object)) else {
            continue;
        };
        if p == RDF_TYPE {
            let Some(kind) = object else { continue };
            if class_types.iter().any(|c| c == kind) {
                s.classes.insert(subject.to_owned());
            } else if property_types.iter().any(|c| c == kind) {
                s.properties.insert(subject.to_owned());
            } else if characteristic_types.iter().any(|c| c == kind) {
                s.characteristics
                    .entry(subject.to_owned())
                    .or_default()
                    .insert(kind.to_owned());
                s.properties.insert(subject.to_owned());
            }
            continue;
        }
        let Some(object) = object else { continue };
        if p == sub_class_of {
            s.subclass_edges
                .insert((subject.to_owned(), object.to_owned()));
        } else if p == disjoint && subject != object {
            s.disjoint_pairs.insert(pair(subject, object));
        } else if p == eq_class && subject != object {
            s.equivalent_classes.insert(pair(subject, object));
        } else if p == eq_property && subject != object {
            s.equivalent_properties.insert(pair(subject, object));
        } else if p == domain {
            s.domain
                .entry(subject.to_owned())
                .or_default()
                .insert(object.to_owned());
            s.properties.insert(subject.to_owned());
        } else if p == range {
            s.range
                .entry(subject.to_owned())
                .or_default()
                .insert(object.to_owned());
            s.properties.insert(subject.to_owned());
        }
    }
    for (child, parent) in &s.subclass_edges {
        s.classes.insert(child.clone());
        s.classes.insert(parent.clone());
    }

    let annotation_predicates = [rdfs("label"), rdfs("comment")];
    let terms: Set = s.classes.union(&s.properties).cloned().collect();
    for term in &terms {
        s.annotations.insert(term.clone(), BTreeSet::new());
    }
    for t in triples {
        let p = t.predicate.as_str();
        if !annotation_predicates.iter().any(|a| a == p) {
            continue;
        }
        if let Some(subject) = named(&t.subject)
            && let Some(entries) = s.annotations.get_mut(subject)
        {
            entries.insert((p.to_owned(), text(&t.object)));
        }
    }
    s
}

/// What changed between two versions.
#[derive(Clone, Debug, Default)]
pub struct Diff {
    pub added_classes: Set,
    pub removed_classes: Set,
    pub added_properties: Set,
    pub removed_properties: Set,
    pub added_subclass_edges: BTreeSet<(Iri, Iri)>,
    pub removed_subclass_edges: BTreeSet<(Iri, Iri)>,
    pub added_disjoint_pairs: BTreeSet<Pair>,
    pub removed_disjoint_pairs: BTreeSet<Pair>,
    pub added_equivalent_classes: BTreeSet<Pair>,
    pub removed_equivalent_classes: BTreeSet<Pair>,
    pub added_equivalent_properties: BTreeSet<Pair>,
    pub removed_equivalent_properties: BTreeSet<Pair>,
    /// Narrowed or changed incomparably, both breaking: (old, new).
    pub narrowed_domain: BTreeMap<Iri, (Set, Set)>,
    pub widened_domain: BTreeMap<Iri, (Set, Set)>,
    pub narrowed_range: BTreeMap<Iri, (Set, Set)>,
    pub widened_range: BTreeMap<Iri, (Set, Set)>,
    pub added_characteristics: BTreeMap<Iri, Set>,
    pub removed_characteristics: BTreeMap<Iri, Set>,
    pub changed_annotation_terms: Set,
}

fn minus<T: Ord + Clone>(a: &BTreeSet<T>, b: &BTreeSet<T>) -> BTreeSet<T> {
    a.difference(b).cloned().collect()
}

/// Each class with every class below it (transitively) under `rdfs:subClassOf`: the types
/// that satisfy a domain or range of those classes.
fn with_subclasses(classes: &Set, children: &BTreeMap<&str, Vec<&str>>) -> Set {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut queue: VecDeque<&str> = classes.iter().map(String::as_str).collect();
    while let Some(class) = queue.pop_front() {
        if seen.insert(class) {
            queue.extend(children.get(class).into_iter().flatten().copied());
        }
    }
    seen.into_iter().map(str::to_owned).collect()
}

fn children(edges: &BTreeSet<(Iri, Iri)>) -> BTreeMap<&str, Vec<&str>> {
    let mut map: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (child, parent) in edges {
        map.entry(parent.as_str()).or_default().push(child.as_str());
    }
    map
}

#[derive(PartialEq, Eq)]
enum Change {
    Narrowed,
    Widened,
}

/// How a domain or range changed. No domain is no constraint, so declaring one where there
/// was none narrows it, and dropping it widens it. An incomparable change counts as
/// narrowing: something that satisfied the old one may not satisfy the new.
fn classify_constraint(old: &Set, new: &Set, old_all: &Set, new_all: &Set) -> Option<Change> {
    if old == new {
        None
    } else if old.is_empty() {
        Some(Change::Narrowed)
    } else if new.is_empty() || new_all.is_superset(old_all) {
        Some(Change::Widened)
    } else {
        Some(Change::Narrowed)
    }
}

/// The difference between two snapshots.
#[must_use]
pub fn compute(old: &Snapshot, new: &Snapshot) -> Diff {
    let mut d = Diff {
        added_classes: minus(&new.classes, &old.classes),
        removed_classes: minus(&old.classes, &new.classes),
        added_properties: minus(&new.properties, &old.properties),
        removed_properties: minus(&old.properties, &new.properties),
        added_subclass_edges: minus(&new.subclass_edges, &old.subclass_edges),
        removed_subclass_edges: minus(&old.subclass_edges, &new.subclass_edges),
        added_disjoint_pairs: minus(&new.disjoint_pairs, &old.disjoint_pairs),
        removed_disjoint_pairs: minus(&old.disjoint_pairs, &new.disjoint_pairs),
        added_equivalent_classes: minus(&new.equivalent_classes, &old.equivalent_classes),
        removed_equivalent_classes: minus(&old.equivalent_classes, &new.equivalent_classes),
        added_equivalent_properties: minus(&new.equivalent_properties, &old.equivalent_properties),
        removed_equivalent_properties: minus(
            &old.equivalent_properties,
            &new.equivalent_properties,
        ),
        ..Diff::default()
    };
    let (old_children, new_children) =
        (children(&old.subclass_edges), children(&new.subclass_edges));
    let empty = Set::new();
    // Domain, range and characteristics are compared only for properties in both versions:
    // a new or removed property is already counted as one.
    for p in old.properties.intersection(&new.properties) {
        for (olds, news, narrowed, widened) in [
            (
                &old.domain,
                &new.domain,
                &mut d.narrowed_domain,
                &mut d.widened_domain,
            ),
            (
                &old.range,
                &new.range,
                &mut d.narrowed_range,
                &mut d.widened_range,
            ),
        ] {
            let (o, n) = (olds.get(p).unwrap_or(&empty), news.get(p).unwrap_or(&empty));
            let change = classify_constraint(
                o,
                n,
                &with_subclasses(o, &old_children),
                &with_subclasses(n, &new_children),
            );
            match change {
                Some(Change::Narrowed) => {
                    narrowed.insert(p.clone(), (o.clone(), n.clone()));
                }
                Some(Change::Widened) => {
                    widened.insert(p.clone(), (o.clone(), n.clone()));
                }
                None => {}
            }
        }
        let o = old.characteristics.get(p).unwrap_or(&empty);
        let n = new.characteristics.get(p).unwrap_or(&empty);
        let added = minus(n, o);
        let removed = minus(o, n);
        if !added.is_empty() {
            d.added_characteristics.insert(p.clone(), added);
        }
        if !removed.is_empty() {
            d.removed_characteristics.insert(p.clone(), removed);
        }
    }
    let no_annotations = BTreeSet::new();
    let old_terms: Set = old.classes.union(&old.properties).cloned().collect();
    let new_terms: Set = new.classes.union(&new.properties).cloned().collect();
    d.changed_annotation_terms = old_terms
        .intersection(&new_terms)
        .filter(|t| {
            old.annotations.get(*t).unwrap_or(&no_annotations)
                != new.annotations.get(*t).unwrap_or(&no_annotations)
        })
        .cloned()
        .collect();
    d
}

fn restricting(characteristics: &Set) -> Set {
    characteristics
        .iter()
        .filter(|c| {
            c.strip_prefix(OWL)
                .is_some_and(|local| RESTRICTING.contains(&local))
        })
        .cloned()
        .collect()
}

/// The bump a diff calls for.
#[must_use]
pub fn classify(d: &Diff) -> Bump {
    let major = !d.removed_classes.is_empty()
        || !d.removed_properties.is_empty()
        || !d.removed_subclass_edges.is_empty()
        || !d.added_disjoint_pairs.is_empty()
        || !d.narrowed_domain.is_empty()
        || !d.narrowed_range.is_empty()
        || !d.removed_equivalent_classes.is_empty()
        || !d.removed_equivalent_properties.is_empty()
        || d.added_characteristics
            .values()
            .any(|c| !restricting(c).is_empty());
    if major {
        return Bump::Major;
    }
    let minor = !d.added_classes.is_empty()
        || !d.added_properties.is_empty()
        || !d.added_subclass_edges.is_empty()
        || !d.widened_domain.is_empty()
        || !d.widened_range.is_empty()
        || !d.added_characteristics.is_empty()
        || !d.removed_characteristics.is_empty()
        || !d.added_equivalent_classes.is_empty()
        || !d.added_equivalent_properties.is_empty()
        || !d.removed_disjoint_pairs.is_empty();
    if minor {
        Bump::Minor
    } else if d.changed_annotation_terms.is_empty() {
        Bump::None
    } else {
        Bump::Patch
    }
}

/// Snapshots both versions, compares them and classifies the change.
#[must_use]
pub fn diff(old: &[Triple], new: &[Triple]) -> (Diff, Bump) {
    let d = compute(&snapshot(old), &snapshot(new));
    let bump = classify(&d);
    (d, bump)
}

fn braces(set: &Set) -> String {
    if set.is_empty() {
        "{(none)}".to_owned()
    } else {
        format!("{{{}}}", set.iter().cloned().collect::<Vec<_>>().join(", "))
    }
}

/// The text report the suite's `version-diff` prints, line for line: a section for each
/// kind of change found, in order of severity, then the suggested bump.
#[must_use]
pub fn report(d: &Diff, bump: Bump, old_label: &str, new_label: &str) -> String {
    let mut out = format!("Ontology version diff: {old_label} -> {new_label}\n\n");
    let mut section = |title: &str, lines: Vec<String>| {
        if lines.is_empty() {
            return;
        }
        let _ = writeln!(out, "{title}:");
        for line in lines {
            let _ = writeln!(out, "  - {line}");
        }
        out.push('\n');
    };
    let iris = |set: &Set| set.iter().cloned().collect::<Vec<_>>();
    // The suite sorts edges by the text of the Python tuple, which orders them by child,
    // then parent, except where one IRI is a prefix of another followed by a character
    // below `'`; that text is reproduced as the sort key.
    let edges = |edges: &BTreeSet<(Iri, Iri)>, phrase: &str| {
        let mut sorted: Vec<&(Iri, Iri)> = edges.iter().collect();
        sorted.sort_by_key(|(a, b)| {
            format!("(rdflib.term.URIRef('{a}'), rdflib.term.URIRef('{b}'))")
        });
        sorted
            .into_iter()
            .map(|(a, b)| format!("{a} {phrase} {b}"))
            .collect::<Vec<_>>()
    };
    let pairs = |pairs: &BTreeSet<Pair>, word: &str| {
        pairs
            .iter()
            .map(|(a, b)| format!("{a} {word} {b}"))
            .collect::<Vec<_>>()
    };
    let changes = |map: &BTreeMap<Iri, (Set, Set)>| {
        map.iter()
            .map(|(p, (o, n))| format!("{p}: {} -> {}", braces(o), braces(n)))
            .collect::<Vec<_>>()
    };
    let characteristic_lines = |map: &BTreeMap<Iri, Set>| {
        map.iter()
            .map(|(p, c)| format!("{p}: {}", c.iter().cloned().collect::<Vec<_>>().join(", ")))
            .collect::<Vec<_>>()
    };

    section("Removed classes [MAJOR]", iris(&d.removed_classes));
    section("Removed properties [MAJOR]", iris(&d.removed_properties));
    section(
        "Removed subclass edges [MAJOR]",
        edges(&d.removed_subclass_edges, "no longer rdfs:subClassOf"),
    );
    section(
        "New disjointness axioms [MAJOR]",
        pairs(&d.added_disjoint_pairs, "disjointWith"),
    );
    section(
        "Narrowed/changed property domains [MAJOR]",
        changes(&d.narrowed_domain),
    );
    section(
        "Narrowed/changed property ranges [MAJOR]",
        changes(&d.narrowed_range),
    );
    section(
        "Removed equivalentClass axioms [MAJOR]",
        pairs(&d.removed_equivalent_classes, "equivalentClass"),
    );
    section(
        "Removed equivalentProperty axioms [MAJOR]",
        pairs(&d.removed_equivalent_properties, "equivalentProperty"),
    );
    let new_restrictions: BTreeMap<Iri, Set> = d
        .added_characteristics
        .iter()
        .map(|(p, c)| (p.clone(), restricting(c)))
        .filter(|(_, c)| !c.is_empty())
        .collect();
    section(
        "New restricting property characteristics [MAJOR]",
        characteristic_lines(&new_restrictions),
    );
    section("Added classes [minor]", iris(&d.added_classes));
    section("Added properties [minor]", iris(&d.added_properties));
    section(
        "Added subclass edges [minor]",
        edges(&d.added_subclass_edges, "rdfs:subClassOf"),
    );
    section(
        "Widened property domains [minor]",
        changes(&d.widened_domain),
    );
    section("Widened property ranges [minor]", changes(&d.widened_range));
    section(
        "Removed (relaxed) disjointness axioms [minor]",
        pairs(&d.removed_disjoint_pairs, "disjointWith"),
    );
    section(
        "Added equivalentClass axioms [minor]",
        pairs(&d.added_equivalent_classes, "equivalentClass"),
    );
    section(
        "Added equivalentProperty axioms [minor]",
        pairs(&d.added_equivalent_properties, "equivalentProperty"),
    );
    section(
        "Removed (relaxed) property characteristics [minor]",
        characteristic_lines(&d.removed_characteristics),
    );
    section(
        "Terms with changed rdfs:label/rdfs:comment only [patch]",
        iris(&d.changed_annotation_terms),
    );
    let _ = write!(
        out,
        "Suggested version bump: {}",
        bump.name().to_ascii_uppercase()
    );
    out
}

/// The JSON the suite's `version-diff --json` writes: the same keys and shapes, with every
/// list sorted (the suite leaves edges and pairs in set order).
#[must_use]
pub fn to_json(d: &Diff, bump: Bump) -> Value {
    let strs = |s: &Set| json!(s.iter().collect::<Vec<_>>());
    let edges = |e: &BTreeSet<(Iri, Iri)>| json!(e.iter().map(|(a, b)| [a, b]).collect::<Vec<_>>());
    let pairs = |p: &BTreeSet<Pair>| json!(p.iter().map(|(a, b)| [a, b]).collect::<Vec<_>>());
    let changes = |m: &BTreeMap<Iri, (Set, Set)>| {
        let mut object = serde_json::Map::new();
        for (p, (o, n)) in m {
            object.insert(p.clone(), json!({ "old": strs(o), "new": strs(n) }));
        }
        Value::Object(object)
    };
    let characteristics = |m: &BTreeMap<Iri, Set>| {
        let mut object = serde_json::Map::new();
        for (p, c) in m {
            object.insert(p.clone(), strs(c));
        }
        Value::Object(object)
    };
    json!({
        "bump": bump.name(),
        "removed_classes": strs(&d.removed_classes),
        "added_classes": strs(&d.added_classes),
        "removed_properties": strs(&d.removed_properties),
        "added_properties": strs(&d.added_properties),
        "removed_subclass_edges": edges(&d.removed_subclass_edges),
        "added_subclass_edges": edges(&d.added_subclass_edges),
        "added_disjoint_pairs": pairs(&d.added_disjoint_pairs),
        "removed_disjoint_pairs": pairs(&d.removed_disjoint_pairs),
        "added_equivalent_classes": pairs(&d.added_equivalent_classes),
        "removed_equivalent_classes": pairs(&d.removed_equivalent_classes),
        "added_equivalent_properties": pairs(&d.added_equivalent_properties),
        "removed_equivalent_properties": pairs(&d.removed_equivalent_properties),
        "narrowed_domain": changes(&d.narrowed_domain),
        "widened_domain": changes(&d.widened_domain),
        "narrowed_range": changes(&d.narrowed_range),
        "widened_range": changes(&d.widened_range),
        "added_characteristics": characteristics(&d.added_characteristics),
        "removed_characteristics": characteristics(&d.removed_characteristics),
        "changed_annotation_terms": strs(&d.changed_annotation_terms),
    })
}
