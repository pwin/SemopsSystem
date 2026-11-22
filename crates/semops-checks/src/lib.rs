//! The ontology quality check registry, compiled in, and a native runner for it.
//!
//! The registry is the Python ontology quality suite's: 61 checks, each with a stable id
//! (`STR-001`), a category, a severity, and two formulations, a SHACL shape and a portable
//! SPARQL `CONSTRUCT` query that produces `sh:ValidationResult`s. This crate embeds those
//! files at build time ([`files`]), parses the registry ([`Registry`]), lists the
//! formulations ([`shapes`], [`queries`]) and resolves a validation result back to the check
//! that produced it ([`Registry::resolve_check_id`]).
//!
//! With the default `run` feature, [`run`] also runs the portable formulation on HOLOS's
//! engine, checks literals against their datatypes, and merges the findings as the Python
//! suite does, so `semops check` and `ontology-suite --engine sparql` report the same rows.
//! `PROVENANCE.md` records which suite commit the files come from.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

#[cfg(feature = "run")]
pub mod run;

/// The release and commit of the Python suite the embedded files come from.
pub const SUITE: &str = "ontology-quality-suite 0.28.0 (d9300e6)";

/// The namespace check ids live in: `oq:STR-001` is `{NAMESPACE}STR-001`.
pub const NAMESPACE: &str = "https://semantechs.co.uk/ontology-quality/";

/// An embedded resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct File {
    /// Relative to `resources/`, with `/` separators: `sparql/structural/STR-001.rq`.
    pub path: &'static str,
    pub text: &'static str,
}

static FILES: &[File] = include!(concat!(env!("OUT_DIR"), "/files.rs"));

/// Every embedded file, sorted by path.
#[must_use]
pub fn files() -> &'static [File] {
    FILES
}

/// One embedded file by its path under `resources/`.
#[must_use]
pub fn file(path: &str) -> Option<&'static str> {
    FILES
        .binary_search_by(|f| f.path.cmp(path))
        .ok()
        .map(|i| FILES[i].text)
}

/// The SHACL shapes files, `shapes/*.ttl`. Not recursive, as the suite's loader is not: a
/// shapes graph is one directory.
pub fn shapes() -> impl Iterator<Item = &'static File> {
    FILES.iter().filter(|f| {
        f.path
            .strip_prefix("shapes/")
            .is_some_and(|name| !name.contains('/') && name.ends_with(".ttl"))
    })
}

/// Directories of queries that read a graph only one stage builds, and so are not run
/// against an ontology or its data: `sparql/tarql/` reads the TARQL BIND facts graph.
pub const SUBJECT_SPECIFIC_DIRS: &[&str] = &["tarql"];

/// One portable check query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Query {
    /// The file's stem, which is the check id it reports: `STR-001`.
    pub id: &'static str,
    pub path: &'static str,
    pub text: &'static str,
    /// Under a [`SUBJECT_SPECIFIC_DIRS`] directory.
    pub subject_specific: bool,
}

/// Every `sparql/**/*.rq`, sorted by path.
#[must_use]
pub fn queries() -> Vec<Query> {
    FILES
        .iter()
        .filter(|f| f.path.starts_with("sparql/") && f.path.ends_with(".rq"))
        .map(|f| {
            let name = f.path.rsplit('/').next().unwrap_or(f.path);
            let dirs: Vec<&str> = f.path.split('/').collect();
            Query {
                id: name.strip_suffix(".rq").unwrap_or(name),
                path: f.path,
                text: f.text,
                subject_specific: dirs[..dirs.len() - 1]
                    .iter()
                    .any(|d| SUBJECT_SPECIFIC_DIRS.contains(d)),
            }
        })
        .collect()
}

/// One check in the registry.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Check {
    pub id: String,
    pub category: String,
    pub metric: String,
    pub default_severity: String,
    pub title: String,
    pub description: String,
    pub remediation: String,
    pub cucumber_feature: String,
    pub cucumber_scenario: String,
}

/// The check registry: every check, in the order `registry.json` lists them.
#[derive(Clone, Debug)]
pub struct Registry {
    namespace: String,
    checks: Vec<Check>,
    index: HashMap<String, usize>,
}

#[derive(Deserialize)]
struct RegistryFile {
    namespace: String,
    checks: Vec<Check>,
}

impl Registry {
    /// The embedded registry.
    ///
    /// # Panics
    ///
    /// Never for a released build: the embedded `registry.json` is parsed by this crate's
    /// tests.
    pub fn builtin() -> &'static Self {
        static BUILTIN: OnceLock<Registry> = OnceLock::new();
        BUILTIN.get_or_init(|| {
            Self::from_json(file("registry.json").expect("registry.json is embedded"))
                .expect("the embedded registry.json parses")
        })
    }

    /// A registry from `registry.json` text: a project's own, or the suite's merged with one.
    ///
    /// # Errors
    ///
    /// If the text is not a registry, or names one check id twice.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        let file: RegistryFile = serde_json::from_str(text)?;
        let mut index = HashMap::new();
        for (i, check) in file.checks.iter().enumerate() {
            if index.insert(check.id.clone(), i).is_some() {
                return Err(serde::de::Error::custom(format!(
                    "check id {} is listed twice",
                    check.id
                )));
            }
        }
        Ok(Self {
            namespace: file.namespace,
            checks: file.checks,
            index,
        })
    }

    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    #[must_use]
    pub fn checks(&self) -> &[Check] {
        &self.checks
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Check> {
        self.index.get(id).map(|&i| &self.checks[i])
    }

    /// The categories, in the order their first check appears.
    #[must_use]
    pub fn categories(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for check in &self.checks {
            if !out.contains(&check.category.as_str()) {
                out.push(&check.category);
            }
        }
        out
    }

    /// The check a validation result came from, from its `sh:sourceConstraintComponent`
    /// and `sh:sourceShape` IRIs.
    ///
    /// The component's local name first (how the portable queries tag their results: they
    /// write `sh:sourceConstraintComponent oq:STR-001`), then the shape's (how a shape named
    /// after its check is tagged). A shape split into several, or a nested property shape,
    /// needs the shapes graph: see [`Registry::resolve_in_shapes`].
    #[must_use]
    pub fn resolve_check_id(&self, component: Option<&str>, shape: Option<&str>) -> Option<&str> {
        [component, shape]
            .into_iter()
            .flatten()
            .map(local_name)
            .find_map(|local| self.get(local).map(|c| c.id.as_str()))
    }

    /// As [`Registry::resolve_check_id`], following a shape through the shapes graph: its
    /// own local name, then an `oq:checkId` annotation on it, then the same for every node
    /// shape that holds it through `sh:property`, so a finding reported against a nested
    /// property shape resolves to the check its enclosing shape is annotated with.
    #[must_use]
    pub fn resolve_in_shapes(
        &self,
        component: Option<&str>,
        shape: Option<oxrdf::NamedOrBlankNodeRef<'_>>,
        shapes: &oxrdf::Graph,
    ) -> Option<&str> {
        if let Some(id) = component.and_then(|c| self.get(local_name(c))) {
            return Some(&id.id);
        }
        let shape = shape?;
        let mut visited = Vec::new();
        self.resolve_shape(shape, shapes, &mut visited)
    }

    fn resolve_shape<'a>(
        &self,
        shape: oxrdf::NamedOrBlankNodeRef<'a>,
        shapes: &'a oxrdf::Graph,
        visited: &mut Vec<oxrdf::NamedOrBlankNodeRef<'a>>,
    ) -> Option<&str> {
        if visited.contains(&shape) {
            return None;
        }
        visited.push(shape);
        if let oxrdf::NamedOrBlankNodeRef::NamedNode(n) = shape
            && let Some(check) = self.get(local_name(n.as_str()))
        {
            return Some(&check.id);
        }
        let check_id =
            oxrdf::NamedNodeRef::new_unchecked("https://semantechs.co.uk/ontology-quality/checkId");
        for value in shapes.objects_for_subject_predicate(shape, check_id) {
            if let oxrdf::TermRef::Literal(l) = value
                && let Some(check) = self.get(l.value())
            {
                return Some(&check.id);
            }
        }
        let property = oxrdf::NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#property");
        let parents: Vec<_> = shapes
            .subjects_for_predicate_object(property, shape)
            .collect();
        parents
            .into_iter()
            .find_map(|parent| self.resolve_shape(parent, shapes, visited))
    }
}

/// The part of an IRI after its last `#`, or failing that its last `/`.
#[must_use]
pub fn local_name(iri: &str) -> &str {
    ['#', '/']
        .into_iter()
        .find_map(|sep| iri.rsplit_once(sep).map(|(_, local)| local))
        .unwrap_or(iri)
}
