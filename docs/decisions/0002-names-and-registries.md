# 0002. Public names and package registries

Status: proposed, 21 November 2026. The PyPI and TestPyPI projects already exist as pending
trusted publishers.

## Context

The proposal used `semops` as a working name for the product, its command, its crates, its
wheel and its npm package. Checking each registry on 21 November 2026 found:

| Registry | `semops` | What we would use instead |
| --- | --- | --- |
| PyPI | **Taken.** An unrelated embeddings library ("Semantic operations for Python", 0.3.1, May 2026). It installs a top-level `semops` import package and no command. | `semops-system`: free, and already set up as a pending publisher |
| TestPyPI | (not checked) | `semops-system`: set up as a pending publisher |
| crates.io | Free, as are `semops-system`, `semops-core` and `semops-cli` | — |
| npm | Free, as are `semops-wasm` and `semops-system` | — |
| GitHub | No user or organisation called `semops` | Repository `pwin/SemopsSystem` |

"SemOps" is also used for a practice, not a product: Semantic Arts writes about it as DevOps
for semantic technology, which is the tradition the SemOps Manual belongs to. No one appears
to hold it as a product mark. The only registry collision is the PyPI library above.

## Decision

The product is **SemOps System**. Each surface takes the shortest name that collides with
nothing:

| Surface | Name | Notes |
| --- | --- | --- |
| Product | SemOps System | Matches the repository and the PyPI project |
| Repository | `github.com/pwin/SemopsSystem` | Exists |
| Command | `semops` | No package anywhere installs a `semops` command. The old `holos`, `shacl` and `tarka` commands stay as aliases (proposal, phase 3) |
| Python distribution | **`semops-system`** on PyPI and TestPyPI | `pip install semops-system` |
| Python import | **`semops_system`** | Not `semops`. The PyPI `semops` library installs that import package, and it is an embeddings library whose users (notebooks, Fabric, Databricks) are the same people who would install ours. Two distributions writing the same `site-packages/semops/` silently overwrite each other, and the second install wins. `semops_system` follows the usual rule that the import name is the distribution name with `_` for `-` |
| Rust crates | `semops` (the command), `semops-core`, `semops-engine`, `semops-shacl`, `semops-map`, `semops-checks`, … | All free on crates.io. Nothing needs publishing there yet: HOLOS, SHACL_Engine and tarka are not on crates.io either |
| npm (WebAssembly) | `semops-wasm` (browser and bundler) and `semops-wasm-node` (Node.js) | The same split as `holos-wasm` and `holos-wasm-node` today |
| Container image | `ghcr.io/pwin/semops` | Mirrored into Azure Container Registry by the deployment pipeline (proposal, Azure section) |
| Old names | `holosdb`, `shacl`, `tarka-rdf` on PyPI; `holos-wasm`, `shacl-wasm` and their `-node` builds on npm | Kept as shims that depend on and re-export `semops-system` / `semops-wasm` at the same version, for at least two minor releases |

### Publishing to PyPI

- The publish job runs from `.github/workflows/workflow.yml`, the workflow filename the
  pending publishers were registered with. A different filename fails the OIDC exchange,
  so the file must not be renamed without updating both publishers.
- It uses trusted publishing (`id-token: write`, `pypa/gh-action-pypi-publish`), the same
  way holosdb, shacl and tarka-rdf publish today, with GitHub environments `testpypi` and
  `pypi`. **To confirm:** if the pending publishers name an environment, it must match these
  names exactly; if they name none, any environment works.
- Versions: the wheel carries the workspace version (0.29.0, see Cargo.toml). Pre-releases
  for checking the pipeline use PEP 440 forms such as `0.29.0.dev1`, published to TestPyPI.

### The first release: a bridge

`python/semops-system` is ready to publish as **0.29.0.dev1**, before the native wheel
exists. It is a pure-Python package that depends on exactly the engine releases in
`versions.toml` (holosdb 0.27.0, shacl 0.3.9, tarka-rdf 0.3.1) and presents them as
`semops_system.holos`, `semops_system.shacl` and `semops_system.tarka`. Those are the modules
themselves, not copies. The native wheel will replace it under the same names, so code
written now keeps working. Its tests (`tests/test_capabilities.py`) are the capability floor
in [0003](0003-capability-floor-and-functions.md) as checks: RDF 1.2 annotations, SPARQL 1.2
triple functions, SEP-0002 dates, GeoSPARQL, `fn:`/`afn:`/`spif:` functions, SHACL Core and
SHACL-SPARQL, tarka with `apf:strSplit`, and the SPARQL 1.2 lift of tarka's output. All 21
passed on Windows with CPython 3.13 on 21 November 2026.

Publishing tags are `py-v*`, kept apart from the workspace's `v*` tags while the two are
versioned separately.

### Claiming the name

A pending publisher does not reserve a name on PyPI. The project only exists once the first
file is uploaded, and until then anyone else can register `semops-system`. Publishing an
early build (even a `0.29.0.dev` wheel or sdist) to PyPI closes that gap. The maintainer
decides whether and when to do it.

## Consequences

- Python users write `pip install semops-system` and `import semops_system`. The wheel is
  abi3, so one wheel per platform serves every CPython from 3.11 (holosdb's floor, the
  highest of the three).
- `semops` stays free as a command name on every platform, and as a crate name on crates.io
  for whenever the crates are published.
- The `shacl` name problem in the proposal's open questions goes away: SHACL_Engine's Rust
  crate becomes `semops-shacl`, and the crates.io `shacl` belongs to someone else, which no
  longer matters.
