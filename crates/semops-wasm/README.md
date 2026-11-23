# semops-wasm

SemOps System for WebAssembly: an RDF 1.2 store with SPARQL 1.2, a SHACL validator and the
61 ontology quality checks, in one module that runs in a browser or in Node with no server
and no network.

| Package | For |
| --- | --- |
| `semops-wasm` | Bundlers (webpack, Vite, Rollup): ES modules with a separate `.wasm` |
| `semops-wasm-node` | Node and `require()`, including the VS Code extension host |

```js
const semops = require('semops-wasm-node');

// A store, and SPARQL 1.2 over it.
const store = new semops.Store();
store.loadTurtle(turtle);
const rows = store.query('SELECT ?s WHERE { ?s a <https://example.org/Shop> }');

// SHACL validation.
const report = semops.validateTurtle(data, shapes);
report.conforms;      // false
report.results();     // the results, as objects

// The ontology quality checks, as `semops check` runs them.
const run = semops.check(ontology, 'turtle');
run.findings;         // [{ checkId, severity, focusNode, message, ... }]
run.complete;         // false if a check's query did not run
semops.listChecks();  // the 61 checks
```

## What is in it

- **`Store`**: HOLOS's in-memory store and SPARQL 1.2 engine, with RDF 1.2, the SPARQL
  extension functions, `queryFederated` for `SERVICE`, and `update`, `dump` and `explain`.
- **`Validator`, `Report`, `validateTurtle`, `rdfsClosure`**: the SHACL engine, with SHACL
  1.0 and 1.2 Core, SHACL-SPARQL, node expressions and SHACL-AF rules.
- **`check`, `listChecks`**: the quality checks. Each runs as a SHACL shape and as a SPARQL
  query, and the findings are merged as the Ontology Quality Suite merges them.
- **`version`**: the SemOps System release the module was built from.

These classes and functions have the same names and signatures as in `holos-wasm` and
`shacl-wasm`, so code written against either moves here by changing its import. The two
engines share one copy of the RDF model, parsers and SPARQL evaluator.

## Building

```sh
node crates/semops-wasm/build.mjs            # pkg-bundler/ and pkg-node/
node crates/semops-wasm/tests/smoke.cjs      # runs the Node build
```

It needs Rust with the `wasm32-unknown-unknown` target, and wasm-pack.

## Publishing

`.github/workflows/npm.yml` builds both packages at the workspace version, smoke-tests the
Node one, and publishes them with npm provenance. Run it by hand from the Actions tab; it
does a dry run unless that box is unticked. It needs an npm token that can publish both
packages, as the repository secret `NPM_TOKEN`:

```sh
gh secret set NPM_TOKEN --repo pwin/SemopsSystem
```

MIT OR Apache-2.0.
