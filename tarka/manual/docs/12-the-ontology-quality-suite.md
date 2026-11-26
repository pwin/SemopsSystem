# 12. The ontology quality checks

SHACL checks what you wrote rules for, and SPARQL what you thought to ask. The ontology quality
checks are a registry of checks someone else thought of: sixty-one of them, over the ontology,
the mappings and the data, each with an identifier, a severity and a remedy. They are the gate of
the [SemOps Manual](https://www.semantechs.co.uk/SemOps_Manual/)'s toolchain, and
[its Chapter 10](https://www.semantechs.co.uk/SemOps_Manual/10-ingest-and-transform.html) runs
them around the ingest stage.

The registry runs in two places:

- **`semops check`**, in SemOps System, the same release as tarka. It runs the checks over an
  ontology and its data natively, in about a second: 42 of the 61, each as a SHACL shape and as
  a SPARQL query, with the findings merged.
- **The [Ontology Quality Suite](https://pypi.org/project/ontology-quality-suite/)**, in Python.
  It runs the other checks too: the ones that need OWL reasoning, the conformance checks, and the
  analysis of TARQL mappings. SemOps System has no equivalent for that analysis yet.

So this chapter runs the suite's `sketch` on the mappings, before any data is read, and
`semops check` on what tarka made.

```sh
pip install ontology-quality-suite        # for sketch; semops comes with tarka (Chapter 2)
```

## Before any data: sketch the mappings

`sketch` reads TARQL queries as text, works out the graph they would make, and holds it up
against the ontology: does the mapping use terms the ontology declares, in the way it declares
them?

```sh
ontology-quality-suite sketch --queries tarql/ \
    --query-pattern "bookshops.rq,stocks.rq,stock.rq,events.rq,works.rq" \
    --ontology reference/01-vocabulary.ttl --out-dir out/oqs/sketch --fail-on never
```

The first time this manual's queries were sketched, with the suite at 0.27.0, it reported:

```text
Findings: 29 total (4 Violation, 2 Warning, 23 Info)
Sketch used 5 query file(s); 48 triples, 34 entities.
```

`findings.txt` in the output folder says what each is, where, and how to fix it. Three of the
six serious ones were fair, and changed the queries:

- **TQL-001, Warning.** *?price is bound in 3 query files by 2 structurally different
  expressions.* It was: a shelf price in `stock.rq`, a recommended price in `works.rq`, a ticket
  price in `events.rq`, all called `?price`. Nothing was wrong in any one query, but a reader
  moving between them would assume one meaning. They are `?shelf_price_value`, `?rrp_value` and
  `?ticket_price_value` now.
- **CNF-001, Warning, and two CNF-003, Violation.** `works.rq` made a translation's class a
  variable, `?work a bs:Work, ?translation_class`, bound to `bs:Translation` only for a
  translation. The output was right, but the sketch could not see through the variable: it took
  `?translation_class` for an undeclared class, and so found `bs:translationOf` used on something
  that was not a `bs:Translation`. The query now makes the *subject* conditional instead, and
  keeps the class constant, which any reader can see:

  ```sparql
  ?translation a bs:Translation ;
      bs:translationOf ?original_iri ;
      bs:translatedBy  ?translator_iri .
  …
  BIND(IF(BOUND(?translation_of), ?work, ?unset) AS ?translation)
  ```

  The output is the same graph, triple for triple ([Chapter 11](11-sparql-checks.md)'s round trip
  says so). A mapping that a static check can read is easier for people to read, too.

The other two violations were the suite's. `bs:locatedIn` and `bs:founded` have a union for a
domain, `[ owl:unionOf ( bs:Bookshop bs:Publisher ) ]`, and a bookshop is in it; the suite at
0.27.0 did not read a union in `rdfs:domain`. A finding is a question, and that one had an
answer. The suite at 0.28.0 reads the union, and the sketch now says:

```text
Findings: 22 total (0 Violation, 0 Warning, 22 Info)
Sketch used 5 query file(s); 48 triples, 37 entities.
```

What remains is information:

- **CNF-005, Info (19).** Classes the ontology declares that the sketch never populates: places,
  people, publishers. Expected: they are reference data, and no spreadsheet makes them.
- **TQL-003, Info (3).** `?isbn`, `?kind` and `?language` are used in a template and bound
  nowhere in the query. In TARQL that is ordinary, since each is a column, and the suite says so,
  asking only that someone check the columns exist.

## After the run: check the data

`semops check` reads its files into one graph, the ontology with the data, so declarations are
checked against their use and the other way round. It wants what tarka made and the reference
data it points into, so that the things the output links to have their types:

```sh
semops check reference/01-vocabulary.ttl \
    out/bookshops.ttl out/stocks.ttl out/stock.ttl out/events.ttl \
    out/works.ttl out/segments.ttl out/connections.ttl \
    reference/02-genres.ttl reference/03-places.ttl reference/05-people.ttl
```

It writes one line per finding, then a count:

```text
Violation STR-006  https://example.org/bookshop-trail/book-a-cold-coming  Object https://example.org/bookshop-trail/series-minster-mysteries of triple (…) is never given an rdf:type anywhere in the graph.
…
545 findings: 11 Violation, 532 Warning, 2 Info
```

and exits with status 1, because something at or above `--fail-on` (a violation, by default) was
found. Five hundred and forty-five is a number to sort, not to fear. By check:

| Findings | Check | Severity | What it is about |
|---:|---|---|---|
| 269 | QUA-004 | Warning | resources without `skos:prefLabel` |
| 94 | QUA-009 | Warning | classes and properties without one `skos:prefLabel` per language |
| 94 | QUA-010 | Warning | classes and properties without `skos:definition` |
| 41 | DAT-002 | Warning | references to things not described: 30 DBpedia places, 11 series |
| 17 | QUA-001 | Warning | classes and properties without `rdfs:label` |
| 16 | STR-004 | Warning | classes with no formal definition |
| 11 | STR-006 | Violation | objects with no type: the same 11 series |
| 3 | (others) | | a property without domain or range, a duplicate label, no `owl:versionIRI` |

Read it in two piles.

**About the ontology, not the transformation.** QUA-001, -004, -009, -010 and STR-004, 490
findings, say the vocabulary labels its terms with `rdfs:label` rather than SKOS, and defines
them in comments. That is a choice the Ontology course makes and discusses; it is the modeller's
conversation, and no mapping can settle it.

**Real findings.** Eleven works are `bs:inSeries` a series that nothing in the data describes
(STR-006 and DAT-002): the committee keeps no spreadsheet of series, so the links point into
nothing. tarka mapped exactly what it was given, and the check shows what was never given. That
is the finding a data team needs: not a broken mapping, but a missing source. (The thirty
DBpedia references are external links, as intended.)

What `semops check` does not do is worth knowing too:

- **No reasoning.** The checks run over the graph as it is. The suite's `data` stage, which
  infers with OWL RL first, counted 78 LOG-002 violations here at 0.27.0: the places are
  `owl:sameAs` their DBpedia resources, so after reasoning a shop was `bs:locatedIn` two names of
  one place. `semops check` reasons nothing, and finds none.
- **No conformance or vocabulary checks**, and none on TARQL mappings: those are the suite's.
- **No scoping by namespace.** The suite's `--own-namespace` kept the ontology's own terms
  apart; here every file's terms are checked. Choose the files to scope it.

`--format` writes the findings as `json`, as `csv` (the suite's `full_results.csv` columns), as
`junit` for a test-results view, or as `sarif` for code scanning, each finding at the line its
node first appears on. `--engine` runs the shapes, the queries or both (the default), and
`--list` lists the registry.

## Where each fits

- **Sketch in review.** It reads queries, not data, so it runs on every pull request that
  changes a mapping, in seconds.
- **Check after each run.** Track the counts between runs: a change in them is the signal.
  [SemOps Manual §10.4](https://www.semantechs.co.uk/SemOps_Manual/10-ingest-and-transform.html)
  shows how the counts move between versions, and why.
- **Gate on what you own.** `semops check` fails on a violation by default; `--fail-on` moves
  the line (`Warning`, `Info`, or `never` to report only). [Chapter 14](14-a-pipeline.md) runs it
  as a gate, through SemOps System's GitHub Action, with the findings in code scanning.

The same checks run from Python, as `semops_system.check(*paths)`, and in the browser through
`semops-wasm`. [SemOps Manual, Chapter 9](https://www.semantechs.co.uk/SemOps_Manual/09-continuous-integration.html)
covers the checks in continuous integration, and its
[Chapter 14](https://www.semantechs.co.uk/SemOps_Manual/14-coverage-and-gaps.html) what they
cover and what they do not.

---

| [← 11. SPARQL checks](11-sparql-checks.md) | [13. Into a store →](13-into-a-store.md) |
|---|---|
