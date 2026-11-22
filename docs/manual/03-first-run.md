# A first run {#first-run}

This chapter takes a spreadsheet of bookshops to a graph you can query, then validates it
and checks it. It uses the Bookshop Trail, the dataset the Semantechs SPARQL, SHACL and
ontology courses share: thirty-three independent bookshops in real British towns, with
invented everything else. Every command here was run for this edition, and the output is
what it printed.

You need four commands from [Installing](02-installing.md): `tarka`, `shacl`,
`holos-server` and `semops`.

## The files

The example lives in the tarka manual's examples, in the workspace:

```sh
cd tarka/manual/examples/bookshop-trail
mkdir out
```

| File | What it is |
|---|---|
| `data/bookshops.csv` | The spreadsheet: one row per shop, with its town, founding year, staff and website |
| `data/faulty/bookshops.csv` | The same spreadsheet with mistakes in it |
| `tarql/bookshops.rq` | The mapping, a TARQL query |
| `reference/01-vocabulary.ttl` … `09-stock.ttl` | The rest of the trail: the vocabulary, genres, places, people, books, events, the walking routes and the stock |
| `reference/s64-the-whole-trail-in-one-shapes-graph.ttl` | The SHACL course's shapes for the whole trail, lesson s64 |

## 1. Map the spreadsheet

```sh
tarka run -q tarql/bookshops.rq -i data/bookshops.csv -o out/bookshops.ttl
```

The mapping turns each row into a bookshop with typed values and a point on the map. This
is the first shop in `out/bookshops.ttl`:

```{.output}
bt:shop-bookbarrow geo:hasDefaultGeometry bt:geom-shop-bookbarrow ;
	geo:hasGeometry bt:geom-shop-bookbarrow ;
	a geo:Feature , bs:Bookshop ;
	rdfs:label "Bookbarrow"@en ;
	wgs84:lat 54.60668 ;
	wgs84:long -3.13089 ;
	bs:floorArea "80"^^xsd:decimal ;
	bs:founded "1990"^^xsd:gYear ;
	bs:hasCafe false ;
	bs:locatedIn bt:place-keswick ;
	bs:sellsSecondHand true ;
	bs:specialises bt:genre-mountaineering ;
	bs:staffCount 3 ;
	bs:website "https://example.org/shops/bookbarrow"^^xsd:anyURI .
```

A cell in a spreadsheet is a string until the mapping says otherwise. The query's `BIND`
lines do that: `STRDT(?founded, xsd:gYear)` makes the founding year a year, and
`xsd:integer(?staff)` makes the staff count a number. [Mapping tables to RDF](07-mapping.md)
covers TARQL.

## 2. Validate it

The shapes need the rest of the trail beside the shops: they check that a shop's town is a
settlement in the place data, for instance. Pass each file with its own `--data`:

```sh
shacl -w --data out/bookshops.ttl \
  --data reference/01-vocabulary.ttl --data reference/02-genres.ttl \
  --data reference/03-places.ttl --data reference/05-people.ttl \
  --data reference/06-books.ttl --data reference/07-events.ttl \
  --data reference/08-trail.ttl --data reference/09-stock.ttl \
  --shapes reference/s64-the-whole-trail-in-one-shapes-graph.ttl
```

```{.output}
conforms: true
```

`-w` lets warnings stand. Without it the report says `conforms: false`, because the SHACL
specification counts every result against conformance, and the trail has six warnings by
design: two shops you cannot walk to, three authors who influence one another in a circle,
and an event featuring an author who had died. The warnings are still listed either way.

Now map the faulty spreadsheet and validate that:

```sh
tarka run -q tarql/bookshops.rq -i data/faulty/bookshops.csv -o out/faulty-bookshops.ttl
shacl -w --data out/faulty-bookshops.ttl \
  --data reference/01-vocabulary.ttl --data reference/02-genres.ttl \
  --data reference/03-places.ttl --data reference/05-people.ttl \
  --data reference/06-books.ttl --data reference/07-events.ttl \
  --data reference/08-trail.ttl --data reference/09-stock.ttl \
  --shapes reference/s64-the-whole-trail-in-one-shapes-graph.ttl
```

Five violations, among the warnings:

```{.output}
conforms: false
  Violation PatternConstraintComponent | focus <…/shop-bookbarrow> | value "www.example.org/shops/bookbarrow"^^<…#anyURI> | path <…#website>
    At most one website, with a scheme: {$value}.
  Violation MinInclusiveConstraintComponent | focus <…/shop-bookwyrm> | value "0"^^<…#integer> | path <…#staffCount>
    Staff count is a positive integer: {$value}.
  Violation DatatypeConstraintComponent | focus <…/shop-borderprint> | value "c.1972"^^<…#gYear> | path <…#founded>
    A shop records the one year it opened, as an xsd:gYear.
  Violation QualifiedMinCountConstraintComponent | focus <…/shop-marginalia> | value - | path _:1_b39
    {$this} is in some number of countries other than one.
  Violation ClassConstraintComponent | focus <…/shop-marginalia> | value <…/place-wigton> | path <…#locatedIn>
    A shop is in exactly one settlement.
```

Each row names the shop, the value, the property and the rule it broke: a website with no
scheme, a shop with no staff, a founding year written `c.1972`, and a town, Wigton, that
the place data does not know. The command exits 1 when the data does not conform, so a
script or a build can stop on it.

::: note
The messages keep their `{$value}` and `{$this}` placeholders. SHACL defines that
substitution for SPARQL-based constraints, and the `shacl` command leaves the messages of
the core constraints as the shape wrote them. The value is in the row above the message.
:::

## 3. Load it and query it

Start a server with the shops and the rest of the trail in memory:

```sh
holos-server --data out/bookshops.ttl \
  --data reference/01-vocabulary.ttl --data reference/02-genres.ttl \
  --data reference/03-places.ttl --data reference/05-people.ttl \
  --data reference/06-books.ttl --data reference/07-events.ttl \
  --data reference/08-trail.ttl --data reference/09-stock.ttl
```

```{.output}
  store    in memory (no --store); nothing persists past exit
loaded 588 quads from out/bookshops.ttl
loaded 509 quads from reference/01-vocabulary.ttl
loaded 215 quads from reference/02-genres.ttl
…
holos-server 0.29.0 (<commit>) listening on http://127.0.0.1:7878
  console  http://127.0.0.1:7878/
  query    http://127.0.0.1:7878/query
```

Open <http://127.0.0.1:7878/> for the query console, or ask from a second terminal. Which
shops are in Hay-on-Wye, oldest first?

```sh
curl -s http://127.0.0.1:7878/query -H 'Accept: text/csv' --data-urlencode 'query=
PREFIX bs:   <https://example.org/bookshop-trail/schema#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?shop ?founded WHERE {
  ?s a bs:Bookshop ; rdfs:label ?shop ; bs:founded ?founded ;
     bs:locatedIn <https://example.org/bookshop-trail/place-hay-on-wye> .
} ORDER BY ?founded'
```

```{.output}
shop,founded
Castle Steps Books,1962
The Clock Tower,1977
```

Each shop has a point, so GeoSPARQL works on them. Which shops are within 100 km of The
Bookwyrm in York?

```sh
curl -s http://127.0.0.1:7878/query -H 'Accept: text/csv' --data-urlencode 'query=
PREFIX bs:   <https://example.org/bookshop-trail/schema#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
PREFIX geo:  <http://www.opengis.net/ont/geosparql#>
PREFIX geof: <http://www.opengis.net/def/function/geosparql/>
PREFIX uom:  <http://www.opengis.net/def/uom/OGC/1.0/>
SELECT ?shop (ROUND(?m / 1000) AS ?km) WHERE {
  <https://example.org/bookshop-trail/shop-bookwyrm> geo:hasGeometry/geo:asWKT ?here .
  ?s a bs:Bookshop ; rdfs:label ?shop ; geo:hasGeometry/geo:asWKT ?there .
  BIND(geof:distance(?here, ?there, uom:metre) AS ?m)
  FILTER(?m > 0 && ?m < 100000)
} ORDER BY ?m'
```

```{.output}
shop,km
Endpapers,0
The Harbour Page,67
Cotton Quarto,93
```

Endpapers is also in York, a few hundred metres away, so it rounds to nought. Stop the
server with Ctrl+C: it kept everything in memory, so nothing is left behind.
[Running the server](04-server.md) covers keeping a store on disk.

## 4. Check it

`semops check` runs the 61 ontology quality checks over everything you give it, as one
graph:

```sh
semops check reference/01-vocabulary.ttl out/bookshops.ttl
```

```{.output}
Violation STR-006  https://example.org/bookshop-trail/shop-bookbarrow  Object https://example.org/bookshop-trail/genre-mountaineering of triple …
…
444 findings: 66 Violation, 377 Warning, 1 Info
```

The 66 violations are STR-006: each shop points at a genre and a town that nothing in the
graph declares. They are declared, in the genre and place files, which this command was
not given. Give it them:

```sh
semops check reference/01-vocabulary.ttl reference/02-genres.ttl \
  reference/03-places.ttl out/bookshops.ttl
```

```{.output}
…
391 findings: 0 Violation, 389 Warning, 2 Info
```

No violations, and the command exits 0. The warnings are documentation the vocabulary
lacks: terms with no `skos:prefLabel` or definition, classes with no formal definition,
places linked to DBpedia resources the graph does not describe. `--fail-on Warning` makes
those fail the command too, which is the setting for a vocabulary you publish.
[Checking an ontology](05-checking.md) has the rest.

## What you have done

You mapped a spreadsheet to RDF, validated it against shapes and found the mistakes in a
faulty copy, served it over SPARQL with a geographic query, and checked it against the
quality registry. Each step was one command with an exit status a build can act on, so the
same steps can run unattended on every change to the spreadsheet or the vocabulary.
