# 0003. The capability floor, and one function library

Status: accepted for the requirements, 21 November 2026. The function library, property
functions in store queries, and SHACL-SPARQL in the native validator are implemented on
the same day (`crates/semops-functions`, `holos-engine/src/pfunc.rs`, `holos-shacl`). They
reach Python users with the first wheel built from this workspace, not with the
`semops-system` 0.29.0.dev1 bridge, which pins the engine releases made before them.

## Requirement

Every SemOps System surface, meaning the `semops` command and server, the `semops-system`
Python wheel, the WebAssembly package and the container image, does everything HOLOS
(`holosdb`) does. Nothing is held back as an optional extra:

| Capability | Where |
| --- | --- |
| RDF 1.2: triple terms, annotations, base direction, in every syntax | Oxigraph crates with `rdf-12`, on in the workspace dependencies |
| SPARQL 1.2 query and update, with SEP-0002 date, time and duration operators | `spargebra`/`spareval`/`sparopt` with `sparql-12` and `sep-0002` |
| SHACL 1.0 and 1.2 Core, SHACL-SPARQL, node expressions and SHACL-AF rules | `shacl`. `holos-shacl`, the native and incremental validator, checks Core itself and hands SHACL-SPARQL constraints and custom constraint components to `shacl` through its bridge, merging what it reports (below). Until [0011](0011-one-shacl-engine.md) HOLOS used its own copy of the engine, `holos-shacl-engine` |
| GeoSPARQL: the 43 `spargeo` functions, plus `geof:buffer` and `geof:boundary` | `holos-engine`, where `spargeo` and `geo` are unconditional dependencies |
| Persistent store (RocksDB) | `holos-store/rocksdb`, a default feature of the command, server and wheel. Not in WebAssembly, which has no file system |
| Extension functions: XPath 3.1 `fn:` and `math:`, Jena ARQ `afn:`, Jena property functions `apf:` and `list:`, SPIN/TopBraid `spif:` | `semops-functions`, registered by every evaluator |

Checked on 21 November 2026: no cargo feature in the merged workspace turns RDF 1.2,
SPARQL 1.2 or GeoSPARQL off for any HOLOS surface. The proposal's risk table suggested
gating GeoSPARQL behind a feature to make installs smaller. That is withdrawn, and GeoSPARQL
stays unconditional.

**tarka stays RDF 1.1 and SPARQL 1.1.** Its own scope checks (`tarka-tarql/src/sparql11.rs`)
refuse triple terms and SPARQL 1.2 patterns, and that stays as it is. RDF 1.2 output from a
mapping is built in the step after it: tarka writes RDF 1.1, and a SPARQL 1.2 `CONSTRUCT` or
`INSERT` run by the engine lifts it. For example, statements the mapping wrote as
`rdf:Statement` resources become triple terms with annotations. Mapping stays simple, RDF
1.2 lives in one engine, and the lift is ordinary SPARQL that can be tested and reviewed.
`python/semops-system/tests/test_capabilities.py` has a worked example; one belongs in the
manual and in `tarka/examples` too.

## Before: four evaluators, three libraries

Until this decision each engine had its own functions, so a function that worked in one
place failed in another:

| Evaluator | `fn:` | `math:` | `afn:` | `apf:` | `spif:` |
| --- | --- | --- | --- | --- | --- |
| HOLOS queries (`holos-engine/src/functions.rs`) | 26 | none | 11 | none | 12 |
| SHACL-SPARQL in `shacl` | none | none | none | none | none |
| HOLOS's native validator | refused `sh:sparql` | | | | |
| tarka mappings (`tarka-tarql/src/afn.rs`, `pfunc.rs`) | none | none | 13, its own copy, Jena's rules | `strSplit`, `concat`, `splitIRI`, `str`, `assign` | none |

## Decision

**One library, `crates/semops-functions`, registered by every evaluator**: HOLOS queries and
updates, both SHACL validators' SPARQL constraints, targets and rules, and tarka's TARQL
evaluation. `semops_functions::register(QueryEvaluator::new())` is the whole integration.

| Namespace | Functions | Specification followed |
| --- | --- | --- |
| `fn:` | 61. Strings (`upper-case` … `translate`, `compare`, `matches`, `replace`, `encode-for-uri`, `iri-to-uri`, `escape-html-uri`), numbers (`abs`, `ceiling`, `floor`, `round` and `round-half-to-even` with precision, `number`, `format-number`), booleans, every `*-from-dateTime`, `*-from-date`, `*-from-time` and `*-from-duration` accessor, the three `adjust-*-to-timezone`, `implicit-timezone`, `current-dateTime`/`-date`/`-time`, and `format-dateTime`/`-date`/`-time` | XPath and XQuery Functions and Operators 3.1, including its typing: `fn:abs(-3)` is the integer 3, `fn:round(-2.5)` is -2, a double is rounded from its exact binary value |
| `math:` | 14: `pi`, `exp`, `exp10`, `log`, `log10`, `pow`, `sqrt`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2` | XPath 3.1 |
| `afn:` | 14, under each of Jena's three namespaces: `localname`, `namespace`, `substr`, `substring`, `strjoin`, `sprintf`, `sha1sum`, `min`, `max`, `pi`, `e`, `sqrt`, `bnode`, `now` | Jena 4.8, as tarka checked against TARQL 1.3. Where HOLOS's copies differed, Jena's rules won: `afn:localname` splits before an XML name; `afn:sprintf` formats as Java's `String.format` does, so tarka no longer refuses it |
| `spif:` | 23: HOLOS's twelve, plus `buildURI`, `replaceAll`, `parseDate`, `dateFormat`, `decimalFormat`, `isValidURI`, `timeMillis`, `currentTimeMillis`, `mod`, `cast`, `random` | TopBraid publishes its function reference only inside its product, so the new ones follow RDF4J's implementation of the library (`org.eclipse.rdf4j.spin.function.spif`): Java `SimpleDateFormat` patterns, Java `replaceAll`, `DecimalFormat` |
| Property functions | `apf:strSplit`, `apf:concat`, `apf:splitIRI`/`splitURI`, `apf:str`, `apf:assign`, `apf:blankNode`/`bnode`, `spif:split`; and, reading the data, `list:member`, `list:index`, `list:length`, `rdfs:member`, `apf:bag`, `apf:seq`, `apf:alt` | Jena 4.8; `spif:split` as RDF4J's `Split` |

The time-dependent functions give the query's start, fixed for the query, as `NOW()` does:
an evaluator is built per query, and the library takes its time then.

**Property functions in store queries** (`holos-engine/src/pfunc.rs`). `spareval` has no
notion of them, so they are taken out of the query before it is evaluated:

- Those that read lists and containers become ordinary SPARQL: `list:member` is the path
  `rdf:rest*/rdf:first`, `list:index` and `list:length` are counts over it, `rdfs:member` and
  `apf:bag`/`seq`/`alt` match `rdf:_1`, `rdf:_2` …, and `rdfs:member` also matches statements
  that use it.
- The rest are evaluated where they stand, as Jena does: the patterns before one are
  evaluated, the function is applied to each solution, and the group continues from the
  result as a `VALUES` table. This is tarka's staged evaluation, generalised from one row to
  the store and to any depth: `OPTIONAL`, `UNION`, `GRAPH`, sub-queries and `INSERT … WHERE`.
  A blank node in a staged solution is carried through the table and comes back as the
  store's own node, so later patterns still join on it.
- Refused, with an error naming the function: `apf:textMatch` and `apf:versionARQ`, a
  property function inside `FILTER EXISTS`, and one in a sub-query inside `GRAPH`.

tarka keeps its own staging, for rows, and refuses what reads the data or invents blank
node labels, as before; the functions, their argument rules and what each makes of a
solution are the shared module.

**SHACL-SPARQL in the native validator.** `holos-shacl` gates commits on the write path, so
it must never answer *conforms* for a constraint it did not check. It refused `sh:sparql`
for that reason. It now accepts it: Core is checked over the store's indexes as before, and
the adapted engine, which implements SHACL-SPARQL (pre-binding, `$PATH`, message templates,
the shared functions), evaluates the SPARQL constraints and custom constraint components;
its results for those are merged into one report. A message or value no graph holds (a
filled-in template, a count) is named by an ephemeral term id that the report carries.
Shapes with either construct bridge the graph once per validation, and revalidation
evaluates them in full whatever the delta: correct, and more expensive than Core alone.

Measuring this found a hole that predates it: custom constraint components were never
refused, because their parameters are in the author's namespace, so a shapes graph that
declared one was validated without it. They are now delegated too. A `sh:targetNode`
written as a SHACL 1.2 node expression is refused rather than taken for a node. The new
`shacl-sparql` and `shacl12-sparql` ratchets run the W3C SHACL-SPARQL suites through the
native validator: one failure (`unsupported-sparql-004`, which the adapted engine fails the
same way), against none for the adapted ratchets' three user-defined `sh:SPARQLFunction`
tests, which the native path skips.

**One table of cases, run through every evaluator** (`semops_functions::cases`): an
expression and the value it must give, checked by this crate against `spareval`, by
`holos-engine` through store queries, by `shacl` and `holos-shacl` as
one `sh:sparql` constraint per case (each with a control case that must fail), and by tarka
as a TARQL mapping. A function that works in a query and not in a shape is a test failure.

## Not supported, and why

`semops_functions::unsupported()` is the list, kept with the code:

- XPath functions on sequences (`fn:tokenize`, `fn:string-join`, `fn:distinct-values` …):
  SPARQL has no sequence type. `apf:strSplit`, `spif:split`, `afn:strjoin` and the aggregates
  do these jobs.
- `fn:doc`, `fn:collection`, `fn:unparsed-text`: requests from the server, the surface
  refused for `LOAD` and `SERVICE`.
- `spif:hasAllObjects` reads the data and `spif:invoke` runs a SPIN function, neither a
  function of its arguments. `spif:regex` has no specification to check against.
- User-defined SHACL-AF functions (`sh:SPARQLFunction` declared in a shapes graph): the
  adapted engine does not implement them yet (three W3C tests).

## Consequences

- The manual's function reference can be generated from `semops_functions::names()` and
  `unsupported()`, so it cannot drift from the code.
- The next `holosdb` (or the workspace's own wheel) changes one answer the bridge's tests
  pin: `Store.validate(..., engine="native")` checks `sh:sparql` instead of raising.
- A function added to the library is added everywhere at once, with a case in the table.
