# 0003. The capability floor, and one function library

Status: accepted for the requirements, 21 November 2026; the function library is planned
work.

## Requirement

Every SemOps System surface, meaning the `semops` command and server, the `semops-system`
Python wheel, the WebAssembly package and the container image, does everything HOLOS
(`holosdb`) does today. Nothing is held back as an optional extra:

| Capability | Today | Where |
| --- | --- | --- |
| RDF 1.2: triple terms, annotations, base direction, in every syntax | HOLOS, SHACL_Engine | Oxigraph crates with `rdf-12`, on in the workspace dependencies |
| SPARQL 1.2 query and update, with SEP-0002 date, time and duration operators | HOLOS | `spargebra`/`spareval`/`sparopt` with `sparql-12` and `sep-0002` |
| SHACL 1.0 and 1.2 Core, SHACL-SPARQL, node expressions and SHACL-AF rules | SHACL_Engine, and HOLOS's adapted validator. HOLOS's native, incremental validator does Core only and **refuses** `sh:sparql` with an error rather than passing it (checked 21 November 2026) | `shacl`, `holos-shacl-engine`; `holos-shacl` (native). One validator with both is phase 4 |
| GeoSPARQL: the 43 `spargeo` functions, plus `geof:buffer` and `geof:boundary` | HOLOS | `holos-engine`, where `spargeo` and `geo` are unconditional dependencies |
| Persistent store (RocksDB) | HOLOS command, server and wheel | `holos-store/rocksdb`, a default feature of each. Not in WebAssembly, which has no file system |
| Extension functions: XPath 3.1 `fn:` and `math:`, ARQ `afn:`, ARQ property functions `apf:`, SPIN/TopBraid `spif:` | Split across engines, see below | To be one library |

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
A worked example of the lift belongs in the manual and in `tarka/examples`.

## The function libraries today

The engines implement extension functions separately, so a function that works in one
place fails in another:

| Evaluator | `fn:` | `math:` | `afn:` | `apf:` (property functions) | `spif:` |
| --- | --- | --- | --- | --- | --- |
| HOLOS queries (`holos-engine/src/functions.rs`) | 28 | none | 11 | none | 13 |
| SHACL-SPARQL in `holos-shacl-engine` and `shacl` | none | none | none | none | none |
| tarka mappings (`tarka-tarql/src/afn.rs`, `pfunc.rs`) | none | none | 12, its own copy | `strSplit`, `concat`, `splitIRI`/`splitURI`, `assign`, `bnode`/`blankNode`, `str` | none |

The SPARQL 1.2 built-ins (`STRLEN`, `REGEX`, `REPLACE`, `YEAR`, `SHA256`, `NOW`, `UUID`,
`TRIPLE`, `LANGDIR` and the rest) come from `spareval` and work in all three. The gaps are in
the extension namespaces:

- A SHACL shape whose `sh:sparql` constraint calls `spif:buildString` or `afn:localname`
  fails in both validators, though SPIN/TopBraid functions are most common in exactly
  that place.
- A TARQL mapping can split a cell with `apf:strSplit`, but the same pattern in a query
  against the store fails.
- HOLOS has no `math:` functions (`math:pow`, `math:exp`, `math:log`, the trigonometric
  functions), which XPath 3.1 defines and Jena supports.
- HOLOS leaves out functions it cannot give a single-valued answer through a custom
  function: `fn:tokenize`, `fn:string-join`, `fn:current-dateTime`, `afn:now`, `afn:bnode`,
  `afn:sha1sum`, `spif:cast` and `spif:random` (`functions::unsupported`). Some have a
  defined answer with more context. `fn:current-dateTime` and `afn:now` are SPARQL's
  `NOW()`, fixed for the query, and `fn:tokenize` is what `apf:strSplit` does as a property
  function.

## Decision

One function library, in the engine, registered by every evaluator: HOLOS queries and
updates, both SHACL validators' SPARQL constraints, targets, rules and node expressions, and
tarka's TARQL and OTTR evaluation. It starts from HOLOS's `functions.rs` (52 functions, each
written to its own library's specification) and adds:

1. `apf:` property functions in store queries, with Jena's semantics (`strSplit`, `concat`,
   `splitIRI`, `assign`, `bnode`, `str`, and `list:member`/`rdfs:member`/`apf:bag`/`seq`/`alt`,
   which read lists and containers and so have something to work on in a store, unlike in a
   tarka row). tarka's staged evaluation (`pfunc.rs`) is the model for evaluating them where
   they stand in the group.
2. `math:` from XPath 3.1: `pi`, `exp`, `exp10`, `log`, `log10`, `pow`, `sqrt`, `sin`, `cos`,
   `tan`, `asin`, `acos`, `atan`, `atan2`.
3. The XPath 3.1 `fn:` functions that SPARQL users reach for and that have one value: `fn:matches`,
   `fn:replace`, `fn:encode-for-uri`, `fn:iri-to-uri`, `fn:escape-html-uri`,
   `fn:timezone-from-dateTime`, the `*-from-date`, `*-from-time` and `*-from-duration`
   families, `fn:adjust-dateTime-to-timezone`, `fn:format-number`, `fn:format-dateTime`,
   `fn:round-half-to-even` and `fn:number`. `fn:tokenize` and `fn:string-join` follow the
   property-function form.
4. The `spif:` functions common in SHACL and SPIN rule sets that HOLOS lacks: `spif:split`
   (a property function), `spif:parseDate`, `spif:dateFormat`, `spif:replaceAll`,
   `spif:regex`, `spif:isValidURI`, `spif:currentTimeMillis`, `spif:hasAllObjects` and
   `spif:invoke`. Each one is checked against TopBraid's documentation before it goes in,
   as HOLOS did for the first 13.
5. tarka's own `afn.rs` is deleted and tarka registers the shared library. tarka keeps
   refusing anything RDF 1.2, as above.

Each function gets a test in the library and one test per evaluator that calls it, so a
function that works in a query and not in a shape is a test failure, not a support ticket.
The list of what is supported is generated from the library, so the manual's function
reference can't drift from the code.

## Consequences

- A conformance gate for functions sits beside the W3C suites: a table of function, inputs
  and expected result, run through each evaluator.
- The library is a crate of its own (working name `semops-functions`). The engine, both
  validators and tarka depend on it, and none of them depend on one another just to get
  functions.
- Until it lands, the inventory above is the honest answer to "does SemOps support X". The
  manual should say so.
