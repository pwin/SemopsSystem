# Validating with SHACL {#validating}

SHACL says what well-formed data looks like, and a validator reports everywhere the data
falls short. SemOps System validates two ways: the `shacl` command, over files, and the
store's own validators, over a store.

## The `shacl` command

```sh
shacl --data data.ttl --shapes shapes.ttl
```

`--data` and `--shapes` each take a path, an `http(s)` URL, or `-` for standard input, and
each repeats: several files become one graph. Blank nodes stay separate per file, so `_:a`
in two files is two nodes. Without `--shapes`, the data is its own shapes graph, the
convention for a self-describing document.

The report is a line per result, with its message under it:

```{.output}
conforms: false
  Violation MinInclusiveConstraintComponent | focus <…/shop-bookwyrm> | value "0"^^<…#integer> | path <…#staffCount>
    Staff count is a positive integer: {$value}.
```

The command exits 0 when the data conforms and 1 when it does not.

### The options you will want

| Option | What it does |
|---|---|
| `-w`, `--allow-warnings` | Let warnings and infos stand: only violations make the data fail |
| `--format <FORMAT>` | `human` (the default), or the RDF validation report in `turtle`, `n-triples` and the other RDF syntaxes |
| `-o`, `--output <FILE>` | Write the report to a file |
| `--max-results <N>`, `--abort` | Stop after N results, or at the first: an answer to "is anything wrong" rather than "what is" |
| `--build-index` | Cache the parsed data beside it as `<file>.shix`, so later runs over unchanged data skip parsing |

`shacl --help` lists the rest: inference before validating (`rdfs`, SHACL-AF rules),
the severity that breaks conformance, and the formats it reads.

::: note
The SHACL specification counts every result against conformance, warnings included. A
graph with only warnings in its report does not conform until you say otherwise with `-w`.
:::

### What it covers

SHACL 1.0 and the SHACL 1.2 drafts: Core, SHACL-SPARQL (constraints and custom constraint
components), node expressions, and SHACL-AF rules (triple rules and SPARQL rules). It reads RDF 1.2 data, so a shape can constrain an annotation. Every SPARQL
constraint can call the 140 extension functions the store's queries call.

Messages keep their `{$value}` placeholders except in SPARQL-based constraints, which is
where SHACL defines the substitution. The value is in the result row itself.

## Validating a store

`holos validate` validates files or a store with the validators built into HOLOS:

```sh
holos validate --data data.ttl --shapes shapes.ttl
```

It has two engines. `--engine adapted`, the default, copies the store into the SHACL
engine above and validates the copy: every feature, at the cost of the copy. `--engine
native` validates the live store through its indexes, with no copy, and revalidates
incrementally after a change. It checks SHACL Core itself and hands SHACL-SPARQL
constraints and custom components to the adapted engine, merging the two reports.

From Python the same choice is `Store.validate(shapes, engine="native")`.

## Shapes and the quality checks

The 61 quality checks of [chapter 5](05-checking.md) are themselves SHACL shapes with
SPARQL twins. Write your own shapes for what your data must be, and let `semops check`
cover what any ontology should be.
