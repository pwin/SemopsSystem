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

Apache-2.0. A rewrite of [epimorphics/IntervalServer](https://github.com/epimorphics/IntervalServer);
see NOTICE. The design and the licence position are in
[docs/decisions/0004](../../docs/decisions/0004-placetime.md).
