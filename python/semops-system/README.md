# semops-system

SemOps System as one Python package:

- the HOLOS RDF 1.2 store and SPARQL 1.2 engine, with GeoSPARQL, SHACL and the `fn:`,
  `afn:` and `spif:` function libraries;
- the SHACL engine;
- tarka, for mapping tables and data frames to RDF;
- the 61 ontology quality checks.

One native extension holds them all, built from one commit of the
[SemOps System workspace](https://github.com/pwin/SemopsSystem).

```sh
pip install semops-system
pip install "semops-system[polars]"     # to map Polars data frames with tarka as well
```

```python
import semops_system as so

store = so.holos.Store()                      # in memory; Store("path") persists to disk
store.update("""
    PREFIX ex: <http://example.org/>
    INSERT DATA { ex:alice ex:age 42 {| ex:source ex:census |} }
""")
store.query("ASK { ?r <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> ?t }")   # True

report = so.shacl.Shapes.from_file("shapes.ttl").validate_file("data.ttl")
report.conforms

mapping = so.tarka.Mapping.tarql_file("people.tarql")
mapping.triplify(polars_frame)

run = so.check("ontology.ttl")                # the quality checks, as `semops check` runs them
for finding in run["findings"]:
    print(finding["severity"], finding["check_id"], finding["focus_node"])
```

`so.holos`, `so.shacl` and `so.tarka` are the modules the `holosdb`, `shacl` and
`tarka-rdf` wheels carry, with the same classes and functions, so their documentation
applies as it stands:
[HOLOS](https://github.com/pwin/SemopsSystem/tree/main/holos/crates/holos-python),
[SHACL engine](https://github.com/pwin/SemopsSystem/tree/main/shacl/crates/shacl-python),
[tarka](https://github.com/pwin/SemopsSystem/tree/main/tarka/crates/tarka-python).

`so.check(*paths, engine="both")` reads the files into one graph and runs each check as a
SHACL shape and as a SPARQL query, merging the findings. `so.check_text(text, format)` does
the same for text, and `so.list_checks()` lists the registry. A run is a dict: `findings`,
`failed` (the checks whose query did not run) and `complete`, which is false when any did
not.

## The `semops` command

The wheel installs the `semops` command, the same command as the binary, with every
subcommand: `serve`, `query`, `load`, `validate`, `map`, `check`, `diff`, `repair` and the
rest. A build that has Python needs nothing else to gate on the quality checks:

```sh
pip install semops-system
semops check ontology/*.ttl --format sarif --out semops.sarif
```

`python -m semops_system` runs it too. Under `semops serve`, `--max-query-memory` is not
enforced: an extension module cannot install the counting allocator the binary has. For a
server, prefer the binary or the container image.

tarka produces RDF 1.1. To get RDF 1.2 from its output, load it into a store and run SPARQL
1.2. For example, this annotates every mapped triple with its source:

```sparql
INSERT { ?s ?p ?o {| ex:source ex:spreadsheet |} } WHERE { ?s ?p ?o }
```

## Releases

- 0.30.0 installs the `semops` command, which now has `diff` and `repair`.
- 0.29.0 was the first native release.
- 0.29.0.dev1 was a bridge that installed three separately released engines (holosdb
  0.27.0, shacl 0.3.9 and tarka-rdf 0.3.1) under these names. Code written against it keeps
  working.

Wheels are built for Linux (x86_64 and aarch64, glibc 2.28 or later), macOS (Apple Silicon
and Intel) and Windows (x64), each for every CPython from 3.11. On other platforms, pip
builds from the sdist, which needs a Rust toolchain, a C++ compiler and libclang.

Licence: MIT OR Apache-2.0.
