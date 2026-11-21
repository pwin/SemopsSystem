# semops-system

SemOps System as one Python install: the HOLOS RDF 1.2 store and SPARQL 1.2 engine (with
GeoSPARQL, SHACL and the `fn:`, `afn:` and `spif:` function libraries), the SHACL engine, and
tarka for mapping tables to RDF.

```sh
pip install semops-system
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
```

`so.holos`, `so.shacl` and `so.tarka` are the `holosdb`, `shacl` and `tarka_rdf` modules, so
their documentation applies as it stands:
[HOLOS](https://github.com/pwin/SemopsSystem/tree/main/holos/crates/holos-python),
[SHACL engine](https://github.com/pwin/SemopsSystem/tree/main/shacl/crates/shacl-python),
[tarka](https://github.com/pwin/SemopsSystem/tree/main/tarka/crates/tarka-python).

## This release

0.29.0.dev1 is a bridge. It pins the engine releases that agree with each other
(holosdb 0.27.0, shacl 0.3.9, tarka-rdf 0.3.1; `semops_system.ENGINES` says which are
installed), so a project needs one pin instead of three. A later release replaces it with
one native wheel built from the SemOps System workspace, under the same names.

tarka produces RDF 1.1. To get RDF 1.2 from its output, load it into a store and run SPARQL
1.2. For example, this annotates every mapped triple with its source:

```sparql
INSERT { ?s ?p ?o {| ex:source ex:spreadsheet |} } WHERE { ?s ?p ?o }
```

Licence: MIT OR Apache-2.0.
