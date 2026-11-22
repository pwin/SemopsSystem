# semops-placetime

URIs for calendar intervals, instants and WGS84 points, each described in RDF from the URI
alone, with no store behind it. The interval URIs keep the patterns reference.data.gov.uk
served from 2010 to 2016, under a base of your choosing.

```rust
use semops_placetime::{resolve, Base, Outcome};

let base = Base::new("https://data.example/").unwrap();
match resolve(&base, "doc/quarter/2006-Q1") {
    Outcome::Document(doc) => println!("{} triples about {}", doc.graph.len(), doc.topic),
    other => println!("{other:?}"),
}
```

| Path after the base | Names |
| --- | --- |
| `id/year/2006`, `id/quarter/2006-Q1`, `id/day/1752-09-14`, `id/second/2006-01-01T00:00:00` | British calendar intervals (Julian until 2 September 1752) |
| `id/gregorian-month/2006-02`, `id/julian-day/1582-10-04` | Gregorian and Julian calendar intervals |
| `id/gregorian-instant/2006-01-01T00:00:00` | An instant |
| `id/gregorian-interval/2006-01-01T00:00:00/P3M` | An interval of any duration |
| `id/geopoint/wgs84/X-0.1276Y51.5072` | A point, described with GeoSPARQL |

`holos-server --placetime` serves them over HTTP. The crate does no I/O: `resolve` says
which redirect or document a path should get, and the caller serialises the graph.

## Two vocabularies

By default a document is written as reference.data.gov.uk wrote it: its intervals
vocabulary, OWL-Time and SCOVO, and WGS84 and GeoSPARQL for points. `resolve_with` and
`describe_with` take a `Vocabulary`, and `Vocabulary::Gist` writes the same things in
gist 14.1.0 instead:

```rust
use semops_placetime::{describe_with, Base, Vocabulary};

let base = Base::new("https://data.example/").unwrap();
let graph = describe_with(&base, "gregorian-month/2024-02", Vocabulary::Gist).unwrap();
```

```turtle
<https://data.example/id/gregorian-month/2024-02> a pt:CalendarMonth ;
    gist:conformsTo ptd:_Calendar_Gregorian ;
    gist:isCategorizedBy ptx:_MonthOfYear_February ;
    gist:startDateTime "2024-02-01T00:00:00"^^xsd:dateTime ;
    gist:endDateTime "2024-03-01T00:00:00"^^xsd:dateTime ;
    gist:hasMagnitude [ a gist:Magnitude ; gist:hasAspect gistd:_Aspect_duration ;
        gist:hasUnitOfMeasure gistd:_UnitOfMeasure_second ; gist:numericValue 2505600 ] ;
    gist:isDirectPartOf <https://data.example/id/gregorian-quarter/2024-Q1> ;
    gist:isPartOf <https://data.example/id/gregorian-half/2024-H1>, <https://data.example/id/gregorian-year/2024> ;
    gist:precedesDirectly <https://data.example/id/gregorian-month/2024-03> .
```

The classes, calendars and categories are in the placetime ontology,
[`ontology/placetime.ttl`](ontology/placetime.ttl) (`ONTOLOGY`), which imports gist and
declares no properties of its own. [`ontology/placetime-shapes.ttl`](ontology/placetime-shapes.ttl)
(`SHAPES`) checks a description in SHACL: validate it together with the ontology. gist
14.1.0 itself is bundled in [`ontology/gist/`](ontology/gist/) so the import resolves
offline, with the starter set of the
[gist reference data](https://github.com/semanticarts/gistReferenceData), where the second
and the duration aspect are defined. [docs/decisions/0007](../../docs/decisions/0007-placetime-in-gist.md) has the
modelling choices.

## Licence

Apache-2.0. A rewrite of [epimorphics/IntervalServer](https://github.com/epimorphics/IntervalServer);
see NOTICE. The design and the licence position are in
[docs/decisions/0004](../../docs/decisions/0004-placetime.md). The bundled gist release and
reference data are Semantic Arts', under CC BY 4.0; NOTICE has the attribution.
