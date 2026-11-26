# 2. Installing

tarka is developed and released in [SemOps System](https://github.com/pwin/SemopsSystem), with
the HOLOS store and SPARQL engine, the SHACL engine and the ontology quality checks. It comes two
ways, and most people want both: a **command**, for the shell and for pipelines, and a **Python
package**, for notebooks and data-frame work. They share one engine and give the same RDF.

## The command

Each [SemOps System release](https://github.com/pwin/SemopsSystem/releases) carries tarka's
command, built and tested on its platform, in an archive with the licences and the README, beside
the `semops` command that holds tarka and everything else:

| Platform | tarka on its own | All of SemOps System |
|---|---|---|
| Linux, x86_64 | `tarka-0.31.0-x86_64-unknown-linux-gnu.tar.gz` | `semops-0.31.0-x86_64-unknown-linux-gnu.tar.gz` |
| Linux, ARM64 | `tarka-0.31.0-aarch64-unknown-linux-gnu.tar.gz` | `semops-0.31.0-aarch64-unknown-linux-gnu.tar.gz` |
| macOS, Apple Silicon | `tarka-0.31.0-aarch64-apple-darwin.tar.gz` | `semops-0.31.0-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `tarka-0.31.0-x86_64-apple-darwin.tar.gz` | `semops-0.31.0-x86_64-apple-darwin.tar.gz` |
| Windows, x86_64 | `tarka-0.31.0-x86_64-pc-windows-msvc.zip` | `semops-0.31.0-x86_64-pc-windows-msvc.zip` |

Unpack one and put `tarka` (or `semops`) on your `PATH`. `SHA256SUMS`, beside the archives, lets
you check what you downloaded:

```sh
sha256sum -c SHA256SUMS --ignore-missing
tarka --version
```

```text
tarka 0.31.0
```

tarka carries SemOps System's version: 0.31.0 is the tarka that was 0.3.1 in its own repository,
in the release that holds it. The command needs nothing else installed: no Java, no Python.

### tarka as `semops map`

`semops map` is tarka, argument for argument, so every command in this manual can be written
either way:

```sh
tarka run -q tarql/bookshops.rq -i data/bookshops.csv
semops map run -q tarql/bookshops.rq -i data/bookshops.csv      # the same
```

The manual writes `tarka`. With only `semops` installed, a copy or link of it named `tarka` is
tarka too, chosen by the name it is run under. `semops` also validates (`semops validate`),
queries (`semops query`), serves a store (`semops serve`) and runs the quality checks
(`semops check`), which Chapters 10 to 13 use.

### From source

With Rust 1.95 or later and a clone of SemOps System:

```sh
cargo install --path tarka/crates/tarka-cli     # installs `tarka` into ~/.cargo/bin
cargo build --release -p tarka-cli              # or build it as target/release/tarka
cargo build --release -p semops                 # and the semops command
```

A plain `cargo build --release` in the repository builds the commands too, and leaves the Python
modules out on purpose: built in the same run, their dependencies would link Python into the
commands, which then would not start without it.

## The Python package

```sh
pip install "semops-system[polars]"
```

`semops-system` is SemOps System as one Python package, imported as `semops_system`. tarka is its
`tarka` module, beside `holos` (the store), `shacl` (the SHACL engine) and `check` (the quality
checks). The `[polars]` extra installs [Polars](https://pola.rs), for mapping data frames. One
wheel per platform serves every CPython from 3.11 on, and it installs the `semops` command as
well.

```python
import semops_system as so
mapping = so.tarka.Mapping.tarql_file("tarql/bookshops.rq")
```

The manual writes `so.` for it throughout. `so.tarka` is the module the `tarka-rdf` package
carries, with the same classes and functions, so code written for one runs on the other: for
tarka alone, or on Python 3.10, `pip install tarka-rdf` and `import tarka_rdf` instead.

## The companions

tarka makes RDF. The manual also checks it, and `semops-system` has what that needs: the HOLOS
store, for loading output and running SPARQL checks over it, and the SHACL engine, which tarka
embeds for `--validate`. One more package does what SemOps System does not do yet:

| Package | What it is | Used in |
|---|---|---|
| `ontology-quality-suite` | the Ontology Quality Suite, the Python home of the quality checks, whose `sketch` reads TARQL mappings | Chapter 12 |

```sh
pip install ontology-quality-suite
```

This edition was checked with SemOps System 0.31.0 (the `tarka` and `semops` commands and
`semops-system`), Polars 2.2.0 and ontology-quality-suite 0.28.0.

The store's server is `semops serve`, needed only for [Chapter 13](13-into-a-store.md), to load
tarka's output over HTTP.

## The examples

Everything the manual runs is in the SemOps System repository, under
`tarka/manual/examples/bookshop-trail/`:

```text
bookshop-trail/
  reference/    the courses' ontology, reference data, the SHACL course's s64 shapes, q01
  data/         the committee's spreadsheets (CSV), a faulty copy, and the prepared tables
  tarql/        TARQL queries
  ottr/         the OTTR template library (three modules)
  ottr-mistakes/  a first draft of some templates, mistakes and all, for tarka lint
  instances/    OTTR instances written by hand, for tarka expand
  bottr/        bOTTR mapping files
  python/       Polars and semops-system scripts
  shapes/       SHACL shapes of the manual's own
  sparql/       SPARQL checks
  tools/        the spreadsheets' generator, a graph comparer, a query runner
  check.py      runs every example in the manual, in order, and checks each result
```

[Appendix E](e-the-example-files.md) describes every file. To run the lot:

```sh
cd tarka/manual/examples/bookshop-trail
python check.py
```

`check.py` writes its output to `out/`. Set `TARKA_EXAMPLES_OUT` to put it somewhere else, on a
larger or faster drive, say; `out/` then becomes a link to that folder, so every command in the
manual still works as written:

```sh
TARKA_EXAMPLES_OUT=E:/tarka-manual/out python check.py
```

Every command in the following chapters is run from `bookshop-trail/`.

---

| [← 1. What tarka does](01-what-tarka-does.md) | [3. The Bookshop Trail →](03-the-bookshop-trail.md) |
|---|---|
