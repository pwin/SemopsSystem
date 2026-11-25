# What SemOps System is {#what-it-is}

SemOps System is the working half of SemOps: the commands and services that load, store,
query, validate, map and check the knowledge graphs a semantic team builds. It came
together from separate projects, and each part still does one job.

## The parts

| Part | Command | What it does |
|---|---|---|
| **HOLOS** | `holos`, `holos-server` | Stores RDF 1.2 in memory or on disk, answers SPARQL 1.2 queries and updates, serves them over HTTP, and applies access policy at the index scan. Also carries GeoSPARQL and two SHACL validators |
| **SHACL engine** | `shacl` | Validates RDF against SHACL 1.0 and 1.2 shapes, including SHACL-SPARQL, node expressions and SHACL-AF rules |
| **tarka** | `tarka` | Maps CSV, Parquet and Polars data frames to RDF 1.1 through TARQL queries or OTTR templates |
| **The check registry** | `semops check` | Runs the 61 ontology quality checks of the Ontology Quality Suite natively, and gates a build on what they find |
| **The `semops` command** | `semops` | All of the above in one binary: `semops serve`, `query`, `validate`, `map`, `check` and the rest. Copied or linked under an engine command's name, it is that command |
| **The function library** | (inside every engine) | 140 SPARQL extension functions: XPath `fn:` and `math:`, Jena `afn:` and property functions, SPIN `spif:` |
| **Placetime** | (inside `holos-server`) | URIs for calendar intervals, instants and points, each described in RDF from the URI alone |

## How they fit

```{.diagram}
  CSV, Parquet, data frames
            |
            v
         tarka  ------------->  RDF 1.1
                                  |
  Turtle, TriG, RDF/XML ...       |
            |                     |
            v                     v
        +------------------------------+        SPARQL 1.2 over HTTP
        |  HOLOS store                 | <----> holos-server
        |  RDF 1.2, policy, GeoSPARQL  |        Graph Store Protocol
        +------------------------------+
            |                 |
            v                 v
     shacl / holos      semops check
     SHACL shapes       61 quality checks
            |                 |
            +-------+---------+
                    v
         a report, and an exit code a build can gate on
```

tarka writes RDF 1.1, because the mapping languages it runs are defined over RDF 1.1. When
you want RDF 1.2 from mapped data, load tarka's output into the store and build the RDF
1.2 with a SPARQL update. That keeps mapping simple and puts RDF 1.2 in one place.

## One version

Every part comes from one Cargo workspace and carries one version number, 0.31.0 for this
manual. A store written by one version's `holos` is read by the same version's
`holos-server`, and the functions a query can call are the functions a SHACL constraint can
call, because both register the same library.

The parts still have their old names, and those names keep working: `holos`, `shacl` and
`tarka` are commands today, and the Python packages `holosdb`, `shacl` and `tarka-rdf` are
on PyPI. The product name is SemOps System. Its command is `semops`, its Python package is
`semops-system`, imported as `semops_system`.

## What it runs on

The store keeps its data in RocksDB on disk, or in memory. SPARQL parsing and evaluation
come from the Oxigraph crates, patched where SemOps System needs behaviour they lack. The
core needs no Java, no Node and no Python at run time. Everything is written in Rust. It
builds for Linux and macOS on x86-64 and Arm and for Windows on x86-64, and the store and
the validator also build for WebAssembly, which is how they run in a browser.
