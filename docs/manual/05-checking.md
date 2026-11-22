# Checking an ontology {#checking}

`semops check` runs the Ontology Quality Suite's 61 checks over an ontology and its data,
and exits with a status a build can gate on. The checks are compiled into the command: it
needs no Python, no network and no files beside the ones you give it.

## Running it

```sh
semops check ontology.ttl data.ttl
```

Every file is read into one graph, so declarations in the ontology are checked against
their use in the data and the other way round. A class used as a type in the data and
declared nowhere is a finding (STR-001); so is a property whose domain names a class nobody
declares (STR-005).

Each finding is one line: its severity, the check, the term it is about, and what is wrong.
The last line counts them:

```{.output}
Warning   STR-004  https://example.org/bookshop-trail/schema#Work  Class … has no formal definition …
Info      QUA-007  https://example.org/bookshop-trail/schema  Ontology … has no owl:versionIRI …
391 findings: 0 Violation, 389 Warning, 2 Info
```

## The checks

`semops check --list` prints all 61, with their severity and category:

```{.output}
STR-001  Violation  structural   Undefined class used as rdf:type
STR-002  Violation  structural   Undefined property used
…
61 checks, from ontology-quality-suite 0.28.0 (d9300e6)
```

| Category | Prefix | What it looks at |
|---|---|---|
| Structural | `STR` | Classes and properties used but not declared, untyped subjects and objects, domains and ranges that name nothing |
| Logical | `LOG` | Contradictions: a class disjoint with its own ancestor, a property with two inverses, a functional property the data breaks |
| Quality | `QUA` | Documentation: labels, definitions, version information |
| Style | `STY` | Naming: class and property name conventions, labels without a language tag |
| Data | `DAT` | Values: literals invalid for their datatype, IRIs nothing describes, duplicated values |
| Efficiency | `EFF` | The shape of the graph: deep class chains, blank node ratios, fan-out from one subject |
| Reasoning | `REA` | Contradictions among individuals: one in two disjoint classes, an asymmetric property both ways, an irreflexive property on itself |
| Conformance, vocabulary, TARQL | `CNF`, `VOC`, `TQL` | Run by the suite's Python stages only, not by this command |

The registry and the checks come from the Ontology Quality Suite, the Python tool, and
`semops check` reports what it reports: its findings were compared with the suite's, field
by field, on the suite's own test data and three of its examples. Seventeen of the
registry's checks run only from the suite's Python stages (reasoners, version comparison,
TARQL analysis), so the command runs 42.

## Severity and the exit status

| Exit status | Means |
|---|---|
| 0 | Nothing at or above `--fail-on` was found |
| 1 | Something was |
| 2 | The run could not be made: a file would not load, or a check failed to execute |

`--fail-on` is `Violation` by default. `--fail-on Warning` fails on warnings too, which suits
a vocabulary you publish; `--fail-on never` always exits 0, for a report you read rather
than a gate.

A check that fails to execute finds nothing, and nothing looks like a clean graph. So a
check that did not run fails the command with status 2, whatever `--fail-on` says, and the
command names the check on its standard error.

## Formats

| `--format` | For |
|---|---|
| `text` (default) | Reading at a terminal |
| `json` | Another program: an array of findings with every field |
| `csv` | A spreadsheet: the same columns as the suite's `full_results.csv` |
| `junit` | A test results view, such as Azure DevOps's Tests tab: one test per check |
| `sarif` | Code scanning, such as GitHub's: each finding as a SARIF 2.1.0 result |

`--out <FILE>` writes the report to a file instead of standard output. The exit status is
the same whatever the format.

In JUnit each of the 61 checks is a test. A check fails when it has findings at or above
`--fail-on`, and its findings are the failure's text. Findings below the gate go in the
test's output, and a check that could not run is an error.

SARIF has no way to point at an RDF node, only at a place in a file. Each result points at
the line where its focus node first appears in the input, by its full IRI or else by its
local name after a prefix, and names the node itself as the result's logical location. A
finding about a blank node points at the first file, with no line.

## Engines

Each check has two formulations, a SHACL shape and a portable SPARQL query, and `--engine`
picks which run:

| `--engine` | Runs |
|---|---|
| `both` (default) | Both, merged; a finding both report is one finding listing both |
| `sparql` | The queries only, on HOLOS. The same findings as `both`, faster |
| `shacl` | The shapes only, on the SHACL engine. Fewer findings: some checks have only a query |

## In a build

A GitHub Actions step that fails the build on a violation:

```yaml
- name: Check the ontology
  run: semops check ontology/*.ttl
```

To see the findings in GitHub's code scanning as well, write SARIF and upload it, even when
the check fails:

```yaml
- name: Check the ontology
  run: semops check ontology/*.ttl --format sarif --out semops.sarif
- uses: github/codeql-action/upload-sarif@v4
  if: always()
  with:
    sarif_file: semops.sarif
```

In Azure Pipelines, JUnit puts each check in the run's Tests tab:

```yaml
- script: semops check ontology/*.ttl --format junit --out semops-junit.xml
  displayName: Check the ontology
- task: PublishTestResults@2
  condition: always()
  inputs:
    testResultsFormat: JUnit
    testResultsFiles: semops-junit.xml
```

The container image carries the command too, for a pipeline that does not install Rust:

```sh
docker run --rm --entrypoint semops -v "$PWD:/work" -w /work semops:0.29.0 \
  check ontology/*.ttl --format csv --out findings.csv
```
