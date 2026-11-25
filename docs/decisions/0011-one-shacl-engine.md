# 0011. One SHACL engine: HOLOS validates with `shacl`

Status: accepted and implemented, 25 November 2026, in `shacl/crates/shacl` and
`holos/crates/holos-shacl`. `holos/crates/holos-shacl-engine` is removed.

## Context

Phase 4 of the proposal asks for one SHACL validator: SHACL_Engine's compile-once IR for
full passes, HOLOS's native evaluator for incremental revalidation, behind the one trait
HOLOS already defines. Its risk table asks for both validators to be run against the W3C
suites until they agree, and for one to be retired then.

The workspace held two copies of the engine. `shacl/crates/shacl` is SHACL_Engine itself,
the crate published as `shacl`. `holos/crates/holos-shacl-engine` was the copy HOLOS took
on 19 August 2026 and adapted. Its PROVENANCE.md (corrected for #25) measured eleven of its
files as differing from upstream, `sparql.rs` by 2,357 lines. Two costs followed:

- **Every engine fix landed twice.** #24 was one bug in identical code in both copies, so it
  needed the same change twice.
- **Upstream fixes had to be ported by hand.** Five were, each against HOLOS's ratchets
  (#25).

#25 also found that the reason recorded for carrying the copy, its wider W3C coverage, no
longer held: both HOLOS validators pass every Core test.

## What the copy had that `shacl` did not

Compiling `holos-shacl` against `shacl` in place of the copy answered this directly: ten
errors, all in HOLOS's incremental path. The bridge from the store compiled unchanged.

| Missing from `shacl` | What HOLOS uses it for |
|---|---|
| `Graph::apply(added, removed)` | Keeping the bridged graph current from a store delta, instead of re-bridging per commit |
| `Shapes::shapes_touching`, `shapes_targeting_class`, `unconditional`, `targeted_ancestors`, `focus_may_be_upstream`, and the index behind them | Planning a revalidation from the predicates and classes a delta touched |
| `sparql::predicates` | Bounding what a SPARQL constraint reads, so it can take part in that plan |
| `ValidationReport::to_oxrdf_declaring` | Rendering a report with or without `sh:conformanceDisallows` |

## Decision

**The incremental API moves into `shacl`, and the copy is deleted.** All of it is additive:

- The shape index (`by_predicate`, `by_target_class`, `parents`, `unconditional`) is built at
  the end of `Shapes::compile`, with the methods above. The code is the copy's, unchanged.
- `sparql::predicates`, with its walk into `FILTER EXISTS` patterns, is the copy's too.
- `Graph::apply` is new code. `shacl`'s graph has a first-level index the copy's lacked: per
  permutation, an array of where each leading id's run of rows starts. `apply` keeps it in
  step as it goes. A row inserted or removed shifts only the offsets after its leading id,
  and the array grows at either end when a delta brings an id outside the span. So a small
  delta costs what the copy's did, a binary search and a memmove of the tail per index, and
  a large one rebuilds. The copy's differential tests came with it: random deltas on both
  sides of the rebuild threshold, compared per index against a fresh build. There is also a
  new test for growth at both ends.

`to_oxrdf_declaring` did not move. `holos-shacl` now follows `shacl`'s convention: an empty
set means the default severities, left undeclared, and a non-empty set is declared.
`report_to_oxrdf` passes the empty set and `report_to_oxrdf_with` passes the caller's.

**`holos-shacl` depends on `shacl` through the workspace entry** (`default-features = false`,
`rdf-12`). That is the configuration the copy had: no rayon, so `holos-wasm` still builds
for `wasm32-unknown-unknown`. `holos-shacl`'s `parallel` feature now forwards to
`shacl/parallel`.

**Two validators remain, one engine.** This is the proposal's split. The native evaluator in
`holos-shacl` checks Core over the store's own indexes and revalidates incrementally.
`shacl` handles SHACL-SPARQL, custom components, node expressions and rules, through the
bridge. What is gone is the second copy of the engine.

## Evidence

- **HOLOS's nine SHACL ratchets are unchanged.** They fail on a newly passing test as well
  as a regression. Core: 98/98 (1.0) and 138/138 (1.2), native and engine. SHACL-SPARQL:
  the same one known failure in 1.0 and three in 1.2 as under the copy. Node expressions:
  none.
- **Every `holos-shacl` test passes**, among them incremental revalidation
  (`engine_revalidate`, `engine_delta`, `incremental_is_safe`) and SHACL-SPARQL through the
  native evaluator (`sparql_native`, `sparql_computed_value`).
- **The whole workspace's tests pass**: 1,522 tests in 157 binaries, with none failing
  (`cargo test --locked --no-fail-fast`). `shacl` also passes its 233 tests in the RDF 1.1
  build. `holos-wasm`, `semops-wasm` and `shacl-wasm` build for `wasm32-unknown-unknown`
  without `shacl/parallel`.
- **It is faster where validation runs.** These are `holos-bench --bin shaclinc` results, one
  release run each, on the same machine. "Prepare" is bridging the store and compiling the
  shapes. "Reval" is applying a one-triple delta and checking what it made stale.

  | quads | prepare, copy → `shacl` | full validate | engine reval |
  |---:|---:|---:|---:|
  | 5,018 | 1.75 → 1.82 ms | 0.62 → 0.43 ms | 0.0072 → 0.0069 ms |
  | 50,018 | 19.7 → 21.2 ms | 7.23 → 4.98 ms | 0.0244 → 0.0235 ms |
  | 250,018 | 109.4 → 116.1 ms | 42.6 → 32.5 ms | 0.0989 → 0.0911 ms |

  Full validation is 24–31% faster, because `shacl` looks a subject's or object's rows up in
  that first-level index instead of binary-searching for them. Building that index makes
  bridging 4–8% slower. Bridging is paid once per `EngineRun`, and validation on every pass.
  Delta revalidation is unchanged.

## What this does not do

- **No `semops-shacl` rename.** Crate names stay as they are (ADR 0002); `shacl` is the name
  PyPI and npm already carry.
- **No behaviour carried over from the copy beyond the API above.** Anything else the copy
  did differently is gone, unless a test or ratchet would have caught it. The copy is in git
  history: the last commit before this one has `holos/crates/holos-shacl-engine` and its
  PROVENANCE.md.
- **The conformance names stay.** The `*_adapted` suites and their `*-adapted.failures`
  baselines keep their names. "Adapted" now means `shacl` reached through HOLOS's bridge.

## Consequences

- A fix to the engine is made once and reaches the `shacl` package, HOLOS, `holosdb`,
  `semops`, `semops-system` and `semops-wasm` together.
- The upstream-port work in #25 ends: there is no downstream copy left to port to.
- `shacl` gains public API, so its next release is a minor version, which the workspace
  version moves anyway.
