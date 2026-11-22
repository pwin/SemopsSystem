# 0005. The check registry as a crate, and `semops check`

Status: accepted and implemented, 22 November 2026, in `crates/semops-checks` and
`crates/semops`.

## Context

The ontology quality suite's registry (61 checks, each with a stable id, a SHACL shape and
a portable SPARQL `CONSTRUCT` query) lived in the Python suite's `resources/` and in a copy
inside the VS Code extension. A parity test guarded the copy, but only on a machine with
both repositories checked out. The proposal makes the registry a data crate compiled into
the binary, so the command, the wheel and the WASM package cannot drift, and adds
`semops check` to run it natively ("a port of the orchestration, not of the rules").

## Decision

**`semops-checks` embeds the files** (`registry.json`, `shapes/*.ttl`, `sparql/**/*.rq`,
`repairs/`) at build time, through a build script that `include_str!`s everything under
`resources/`. It parses the registry, lists the shapes and the queries (marking the
TARQL-only ones, which the suite does not run against an ontology), and resolves a
validation result to its check: by the component's local name, then the shape's, then an
`oq:checkId` annotation, then through the node shape that holds a nested property shape.
`PROVENANCE.md` records the suite commit; a test compares the copy with a suite checkout,
character for character apart from line endings, when one is present, and skips in CI as
the suite's own parity test does. The data alone builds with `--no-default-features`, for
a WASM or Python build that brings its own engine.

**The runner is the suite's `--engine sparql`**, which the suite's own walkthrough
recommends for CI and documents as finding exactly what `--engine both` finds, because every
shape has a portable twin. Every query runs on HOLOS's engine over the graph under test; the
literal check (`DAT-001` over every typed literal) is reimplemented against XML Schema's
lexical spaces through `oxsdatatypes`, where the suite uses rdflib's conversions; and
`checks/merge.py` is ported rule for rule: the property-path rendering of blank-node paths,
sorted multi-values, a blank focus node's label kept out of messages, `sh:value` defaulting
to the focus node, the dedup key, the per-arm ordinal that merges anonymous findings without
comparing labels, and the sort.

**`semops check FILE...`** reads the files into one graph, runs the checks and prints the
findings as text, JSON, or the suite's `full_results.csv` columns. It exits 0, 1 when a
finding at or above `--fail-on` (default `Violation`) was found, or 2 when it could not run
or **a check failed to execute**. The suite records a failed query and carries on; here it
fails the run, because a query that does not execute finds nothing and so looks exactly like
a clean graph.

## How it was held to the suite

The suite, at the commit `PROVENANCE.md` names, was installed and run with
`run_registry_suite_on_graph(engine="sparql")` over the four fixture sets its
`test_check_firing_coverage.py` uses. Its rows are committed as
`tests/fixtures/expected/*.json`, and `tests/parity.rs` compares them with the runner's
findings as multisets: every field (check id, severity, focus node, path, value, message,
sources), with blank node labels masked. 447 findings, 60 of them about blank nodes, match.
Run outside the fixtures, on the suite's `riverside_library`, `acme_robotics` and gist
14.1.0 examples, every finding key matched as well (56, 36 and 44 findings).

Three differences were found, and none is a different finding:

- **A literal rdflib rewrites.** The suite parses with rdflib, which turns
  `"yesplz"^^xsd:boolean` into `"false"` as it reads it, so its finding quotes `false` and
  only its literal check sees it. Read as written, both arms see `yesplz`, and the finding
  quotes it. The suite's own `holos_sparql.py` describes this as the improvement it could not
  make without changing rdflib's settings for every stage.
- **`LOG-004`** writes its message from whichever of two symmetric solutions the engine
  returns first ("two distinct declared inverses (A and B)"), and **`STY-005`** joins
  separators with an unordered `GROUP_CONCAT` (`_|-` against `-|_`). Neither order is
  defined; the parity test compares those two checks' lists in sorted order and every other
  check's text exactly. Both queries are worth fixing in the suite (an `ORDER BY` inside the
  aggregate, or a `FILTER(STR(?a) < STR(?b))`), and the fix belongs there, since this crate
  copies the queries unchanged.

## Not done

- The SHACL arm (`--engine shacl`/`both`): compiling the shapes with the SHACL engine and
  resolving its results through the shapes graph. `Registry::resolve_in_shapes` is the part
  of it already here. It adds the cross-check between formulations, not findings.
- `--inference rdfs`, the suite's own-namespace filter, severity policies, repairs and the
  HTML and Markdown reports.
- Pointing the Python suite and the VS Code extension at this crate's copy, which is what
  retires the other two copies. Until then all three must be kept equal.
- The fixtures make 35 of the 42 portable checks fire. The other seven (`QUA-005`,
  `QUA-006`, `QUA-008` and `REA-001` to `REA-004`) are held only by their queries executing,
  and 19 registry checks are not run by `semops check` at all: 17 have no portable query
  (`CNF-001` to `CNF-005`, the reasoner-backed `REA-005` to `REA-022`, `TQL-001` to
  `TQL-003` and `VOC-001`, which the suite runs from Python stages), and `TQL-004` and
  `TQL-005` read the TARQL BIND facts graph, not an ontology.
