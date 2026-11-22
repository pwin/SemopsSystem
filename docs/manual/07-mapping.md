# Mapping tables to RDF {#mapping}

tarka turns rows into triples. It reads CSV, Parquet and Arrow files, and Polars data
frames from Python, and maps them through either of two languages: TARQL, which is SPARQL
`CONSTRUCT` over the rows, or OTTR, which is templates. tarka has its own manual, in
`tarka/manual/`, built the same way as this one; this chapter is the short version.

## TARQL

A TARQL mapping is a SPARQL `CONSTRUCT` query in which each column is a variable:

```sparql
PREFIX bs: <https://example.org/bookshop-trail/schema#>
CONSTRUCT { ?shop_iri a bs:Bookshop ; bs:founded ?year . }
WHERE {
  BIND(IRI(CONCAT("https://example.org/bookshop-trail/shop-", ?shop)) AS ?shop_iri)
  BIND(STRDT(?founded, <http://www.w3.org/2001/XMLSchema#gYear>) AS ?year)
}
```

```sh
tarka run -q shops.rq -i bookshops.csv -o shops.ttl
```

Every cell arrives as a string, so the `BIND` lines are where a mapping turns cells into
IRIs, numbers and dates. An empty cell leaves its variable unbound, and a triple with an
unbound variable is not written, which is usually what a missing value should do.

| Option | What it does |
|---|---|
| `-i <FILE>` | The input: CSV, Parquet or Arrow. Without it, the file the query names with `FROM` |
| `-o <FILE>` | Where the RDF goes; standard output otherwise |
| `--test [N]` | Map only the first N rows (5 by default), to try a mapping on a sample |
| `--split <COL> <NEW> <SEP>` | Repeat each row once per part of a column split on a separator |
| `-t`, `-d <DELIM>` | Tab-separated input, or another delimiter |

## OTTR templates

An OTTR template names a pattern once and instantiates it per row. The template library
is in stOTTR files, and each root template's parameters are the CSV's columns:

```sh
tarka run -l templates/ -T ex:Bookshop -i bookshops.csv -o shops.ttl
```

`tarka expand` expands OTTR instances, as Lutra does; `tarka lint` checks a library for
undefined templates, wrong arities and cycles; `tarka bottr` runs bOTTR instance maps.

## Shapes for what a mapping makes

`tarka shapes` writes SHACL shapes describing the RDF a mapping produces, which is a quick
way to start a shapes graph, and a check that the mapping does what you meant.

## From Python

```python
import polars as pl
import tarka_rdf

mapping = tarka_rdf.Mapping.tarql_file("shops.rq")
triples = mapping.triplify(pl.read_csv("bookshops.csv"))
```

## Getting RDF 1.2

tarka writes RDF 1.1, because TARQL and OTTR are defined over RDF 1.1. For RDF 1.2, load
the output into the store and build the RDF 1.2 with an update. This annotates every
mapped triple with where it came from:

```sparql
PREFIX ex: <https://example.org/>
INSERT { ?s ?p ?o {| ex:source ex:spreadsheet |} } WHERE { ?s ?p ?o }
```
