# 0004. Placetime: the reference.data.gov.uk interval URIs, and points

Status: accepted and implemented, 22 November 2026, in `crates/semops-placetime` (the URIs
and their descriptions, no I/O) and `holos-server` (`--placetime`, `--placetime-base`).

## Context

From 2010 to 2016 reference.data.gov.uk answered URIs for calendar intervals:
`http://reference.data.gov.uk/id/quarter/2006-Q1` named the first quarter of 2006 and
redirected to a description of it, with its months, its neighbours, the year and half it sits
in, and the instants it begins and ends at. Statistical data published then cites these URIs
as its time dimension. The servlet behind them, Epimorphics' IntervalServer (Java, Apache-2.0),
is archived, GDS no longer runs it, and on 22 November 2026 the domain no longer resolved.

The requirement was a side function of the SemOps server that serves the same URIs under
the service's own host, or any configured URL; rewritten in Rust; keeping the URI patterns;
Gregorian and Julian calendars; `https` rather than `http`; and places as well as times.

## Decision

**The patterns are IntervalServer's**, after a base that is now the service's:

| Path after the base | Names | Calendar |
| --- | --- | --- |
| `id/year/2006`, `half/2006-H1`, `quarter/2006-Q1`, `month/2006-01`, `week/2006-W01`, `day/2006-01-31`, `hour/2006-01-31T09`, `minute/…T09:30`, `second/…T09:30:00` | Calendar-aligned intervals | British: Julian to Wednesday 2 September 1752, Gregorian from Thursday 14 September |
| `id/gregorian-year/2006` … `id/gregorian-second/…` | The same, Gregorian throughout | Proleptic Gregorian |
| `id/julian-year/1582` … `id/julian-second/…` | The same, Julian throughout | Proleptic Julian. **New**, in the Gregorian set's pattern |
| `id/gregorian-instant/2006-01-01T00:00:00` | An instant, to the second, unzoned | Gregorian |
| `id/gregorian-interval/2006-01-01T00:00:00/P3M` | An interval: a start and an `xsd:duration` | Gregorian |
| `id/geopoint/wgs84/X-0.1276Y51.5072` | A WGS84 point, longitude then latitude in decimal degrees | **New**, after placetime.com |
| `id/year`, `id/gregorian-month`, `id/uk-calendar`, `id/gregorian-calendar`, `id/julian-calendar`, `id/interval`, `id/gregorian-instant`, `id/gregorian-interval`, `id/geopoint/wgs84` | The URI sets themselves, as VoID datasets | — |

`/id/…` answers 303 See Other to `/doc/…`, which serves the description in the format the
`Accept` header prefers, or the one an extension names (`.ttl`, `.nt`, `.rdf`, `.jsonld`, and
`.n3` and `.json` for links made to the old service). A thing asked for in a spelling other
than its canonical one (`P0Y03M` for `P3M`, `X-01.50` for `X-1.5`) gets 301 to the canonical
URI, so each thing has one name. Anything else under `/id/` or `/doc/` is 404.

**The base is `https://{Host}/`, or what `--placetime-base` says.** With `--placetime`,
every URI in an answer is minted from the host the client named; the scheme is `https`
whatever the request arrived on, because a server behind a TLS front door receives plain
HTTP and would otherwise publish `http://` names. `X-Forwarded-Host` replaces `Host` only
under `--trust-forwarded-identity`, the operator's existing statement that forwarded
headers are trustworthy, and either header must be a host name or address with an optional
port, or the answer is 400: the header goes into every URI of the answer. With
`--placetime-base https://data.example/ref/` the URIs are `https://data.example/ref/id/…`
for every request, answered at `/ref/id/…` on this server. Placetime is off unless one of
the two flags is given.

**The vocabularies keep their namespaces**, `http://reference.data.gov.uk/def/intervals/`,
OWL-Time's 2006 namespace, SCOVO and the rest. They are the names of published terms, not
addresses this service answers at, and changing their scheme would make data that uses them
stop matching. `https` applies to the URIs this service mints.

**Every interval says which Gregorian interval it equals.** A British or Julian month or
year can differ in length from the Gregorian one starting on the same day, so the equal
interval's duration is computed where it must be: the British year 1752, which lost eleven
days, is `gregorian-interval/1752-01-12T00:00:00/P11M20D`. Day numbers make this exact; there
is no calendar library, as there was no time zone in the originals. IntervalServer's choices
are kept where they were choices: the British year starts on 1 January throughout, though the
legal year began on 25 March until 1751; Julian and British weeks are numbered by the ISO
8601 rule over the days each calendar has, so the British 1752 has 51 weeks; adding a month
keeps the day of the month, cut back to the month's length, as `java.util.Calendar` did.

**Points are described for GeoSPARQL.** A point is a `geo:Geometry` and `sf:Point` with a
`geo:asWKT` literal in CRS84 (`POINT(-0.1276 51.5072)`), a W3C Basic Geo `wgs84:Point` with
`wgs84:lat` and `wgs84:long`, a label (`WGS84 Point: 51.5072° N, 0.1276° W`) and an
`rdfs:seeAlso` to its RFC 5870 `geo:` URI. Coordinates are canonical decimals with at most ten
places. `holos-server`'s tests load point descriptions into a store and find them with
`geof:sfWithin`, and order a British day and its Gregorian equal by their beginning instants.

**Corrected from IntervalServer:** a half year's duration is `P6M` (it said `PT1S`); an
hour's previous hour is one hour earlier (it was the same hour a day earlier); a week's label
names its calendar (it always said "Gregorian Week"); "fourth" (it said "forth"). Each has a
test.

**Not reproduced:** the provenance block on every IntervalServer document, which named
data.gov.uk as publisher and rights holder, claimed Crown copyright and pointed to the Open
Government Licence. Those were that deployment's claims. A document here says what it is
about and which formats it comes in, and nothing about who publishes it.

## The bundled ontology files and their licence

IntervalServer's repository bundles `intervals.n3`, `intervals.ttl`, `intervals.rdf` (the
reference.data.gov.uk intervals vocabulary, three serialisations), `intervals.n3.tbc` (a
TopBraid editor file), `reference.n3` and `uri-set.n3` (the URI set vocabularies), and, among
the Java sources, `void.rdf`, a copy of the VoID vocabulary. Checked on 22 November 2026:

- **None of the files states a licence, rights holder or creator.** The only grant that
  covers them is the repository's `LICENCE`: Apache-2.0, "Copyright 2010-2016 Epimorphics
  Limited".
- **The service's own code says otherwise about its output.** `Constants.java` sets
  `COPYRIGHT_ASSIGNMENT = "Crown Copyright (c)"`, `PUBLISHER_NAME = "data.gov.uk"` and
  `LICENSE_URI = ".../id/open-government-licence"`, and every document carried those. The
  vocabularies were published by data.gov.uk at reference.data.gov.uk, so they may be Crown
  copyright, which Epimorphics could not license under Apache-2.0; the Open Government Licence
  would then be the operative grant. Both licences permit reuse with attribution, but the
  files do not say which applies, and the published pages that might have said cannot be
  read: reference.data.gov.uk no longer resolves, the DBpedia Archivo snapshot refused the
  connection, and the Wayback Machine could not be fetched from here.
- **`void.rdf` is not Epimorphics' work.** It is a copy of the VoID vocabulary
  (`http://rdfs.org/ns/void#`, DERI and the W3C Semantic Web Interest Group). Neither the
  file, nor the vocabulary's maintenance repository (cygri/void, `rdfs/void.ttl`), states a
  licence; the W3C Note describing VoID is © W3C under its document-use rules. The repository's
  Apache-2.0 grant cannot extend to it.

So **none of the files is bundled**. The crate uses the vocabularies' IRIs, which are names
and need no licence, and generates every statement it serves from the URI asked for. The
crate itself is Apache-2.0, not the workspace's MIT OR Apache-2.0, because it is a rewrite of
Apache-2.0 code that keeps its patterns and wording; `crates/semops-placetime/NOTICE`
attributes it and lists the changes.

## Not done

- The UK government business calendar (`government-year`, `-half`, `-quarter`, `-week`,
  the year from 1 April). It was out of scope; the calendar code would take it as a fourth
  calendar.
- Serving the intervals vocabulary itself at `/def/intervals`. Its namespace is
  reference.data.gov.uk's, and the licence position above would need settling first.
- Zoned instants and intervals, which IntervalServer also left to placetime.com.
- Places beyond points: areas, bounding boxes, administrative geographies.
- The VoID linksets IntervalServer listed in its set documents; the sets here give their
  subsets, item type, URI pattern, an example and a source.
- SPARQL functions that mint these URIs (for example a `placetime:quarter(?date)`), which
  `semops-functions` could add.

## The SPARQL endpoint's base follows

`holos-server` resolved relative IRIs in a query or update against `http://{Host}/query`
(or `/update`). It now uses `https://{Host}/…`, for the same reason as placetime: behind a
TLS front door the scheme on the connection is not the scheme the service is published at.
`CONSTRUCT { <s> <p> 1 } WHERE {}` sent to `data.example` now returns
`<https://data.example/s>`, where it used to return `<http://data.example/s>`. A query that
writes absolute IRIs is unaffected.
