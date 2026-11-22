# Running the server {#server}

`holos-server` serves a store over HTTP: SPARQL 1.2 query and update, the Graph Store
Protocol for whole graphs, a query console, and optionally the placetime URIs. It is one
process with no other services behind it.

## Starting it

With no flags the store is in memory and the server listens on the loopback address:

```sh
holos-server
```

The flags you will use most:

| Flag | What it does |
|---|---|
| `--store <DIR>` | Keep the store on disk, in a RocksDB directory. Without it, nothing survives a restart |
| `--data <FILE>` | Load a file at start-up. Repeatable; `.gz` files are read as they decompress |
| `--listen <ADDR>` | The address to listen on. Default `127.0.0.1:7878`; `0.0.0.0:7878` for every interface |
| `--read-only` | Answer 403 to every write over HTTP |
| `--timeout <SECONDS>` | Abandon a query after this long |
| `--reorder` | Order each query's patterns by estimated cardinality; much faster on badly written queries |
| `--no-ui` | Do not serve the console |

[Reference](11-reference.md#holos-server) lists every flag.

## The endpoints

| Request | What it does |
|---|---|
| `GET /query?query=…`, `POST /query` | A SPARQL 1.2 query. Results as JSON, XML, CSV or TSV by the `Accept` header |
| `POST /update` | A SPARQL 1.2 update |
| `GET`, `PUT`, `POST`, `DELETE /graph?graph=<IRI>` | Read, replace, add to or remove a whole graph (`?default` for the default graph) |
| `GET /` | The query console |
| `GET /health` | `ok`, for load balancers and probes |
| `GET /stats` | The store's counts and the server's version, as JSON |
| `POST /backup` | A consistent checkpoint, when `--backup-dir` and `--backup-role` are set: see [chapter 8](08-storage.md) |

A query from the command line:

```sh
curl -s http://127.0.0.1:7878/query --data-urlencode 'query=SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }'
```

An update, adding a triple with an RDF 1.2 annotation on it:

```sh
curl -s http://127.0.0.1:7878/update -H 'Content-Type: application/sparql-update' --data '
PREFIX ex: <https://example.org/>
INSERT DATA { ex:alice ex:age 42 {| ex:source ex:census |} }'
```

```{.output}
{"inserted":3,"deleted":0,"graphsCreated":0,"graphsDropped":0}
```

Three triples: the statement, a reifier that names it, and the annotation on the reifier.

A relative IRI in a query or an update resolves against `https://<host>/query` or
`https://<host>/update`, where the host is the one the request named. `CONSTRUCT { <s> <p> 1 }
WHERE {}` sent to `data.example` gives `<https://data.example/s>`.

## What a request cannot do

The server guards itself against its own clients. These are on by default:

| Request | Answer |
|---|---|
| A body larger than 256 MiB | 413. `--max-body` changes the limit |
| `LOAD <file:…>` in an update | 400, unless `--load-dir` names a directory; then only files inside it |
| RDF/XML with more than 256 attributes on one element | 400, before the parser sees it |
| A query that grows the process past 8 GiB | Refused with an error, the server carrying on. `--max-query-memory` changes the limit |

[Security](09-security.md) explains each.

## Who is asking

The server authenticates nobody. It can apply a policy (graphs a principal may read,
predicates nobody may see, classification levels) to an identity something in front of it
has verified, and it reads that identity only when told to:

| Flag | Where the identity comes from |
|---|---|
| none | Nowhere: every request is anonymous |
| `--trust-forwarded-identity` | `X-Holos-Principal`, `X-Holos-Roles` and `X-Holos-Clearance`, set by a proxy or API gateway |
| `--trust-azure-easy-auth` | `X-MS-CLIENT-PRINCIPAL`, set by Azure Container Apps or App Service built-in authentication |

With either flag, whoever reaches the server's port can claim any identity, so the port
must be reachable only through the thing that sets the headers. [Security](09-security.md)
and `holos/ACCESS-CONTROL.md` describe the policy itself.

## Placetime URIs

`--placetime` makes the server answer URIs for calendar intervals, instants and points,
each described in RDF from the URI alone. The patterns are the ones reference.data.gov.uk
used from 2010 to 2016, so data that cites them keeps its meaning under a new host:

```sh
holos-server --placetime
curl -s -i http://127.0.0.1:7878/id/quarter/2006-Q1
```

```{.output}
HTTP/1.1 303 See Other
Location: https://127.0.0.1:7878/doc/quarter/2006-Q1
```

`/id/…` names the thing and redirects to `/doc/…`, its description, in Turtle, N-Triples,
RDF/XML or JSON-LD by the `Accept` header or an extension (`.ttl`, `.nt`, `.rdf`, `.jsonld`):

| Path | Names |
|---|---|
| `id/year/2006`, `id/quarter/2006-Q1`, `id/day/1752-09-14` … | British calendar intervals: Julian until 2 September 1752, Gregorian after |
| `id/gregorian-month/2006-02`, `id/julian-day/1582-10-04` | Gregorian and Julian calendar intervals |
| `id/gregorian-instant/2006-01-01T00:00:00` | An instant |
| `id/gregorian-interval/2006-01-01T00:00:00/P3M` | An interval of any length |
| `id/geopoint/wgs84/X-0.1276Y51.5072` | A point, longitude first, described with GeoSPARQL |

The URIs are minted under `https://` and the host the client named.
`--placetime-base https://data.example/ref/` mints them under a fixed base instead.

### In gist

The documents describe the URIs as reference.data.gov.uk did, in its intervals vocabulary
with OWL-Time and SCOVO. `--placetime-vocabulary gist` describes the same URIs in gist
14.1.0 instead:

```sh
holos-server --placetime --placetime-vocabulary gist
curl -s -H 'Host: time.example' http://127.0.0.1:7878/doc/day/1752-09-14.ttl
```

```{.output}
<https://time.example/id/day/1752-09-14> a pt:CalendarDay ;
	skos:prefLabel "British Day:1752-09-14" ;
	foaf:isPrimaryTopicOf <https://time.example/doc/day/1752-09-14> ;
	gist:conformsTo ptd:_Calendar_British ;
	gist:description "Thursday the 14th of September in the British calendar year 1752" ;
	gist:endDateTime "1752-09-15T00:00:00"^^xsd:dateTime ;
	gist:hasMagnitude _:a21cb73160ad78154c98aaea0ae1e92d ;
	gist:isCategorizedBy ptx:_DayOfWeek_Thursday ;
	gist:isDirectPartOf <https://time.example/id/month/1752-09> , <https://time.example/id/week/1752-W36> ;
	gist:isMemberOf <https://time.example/id/day> ;
	gist:isPartOf <https://time.example/id/half/1752-H2> , <https://time.example/id/quarter/1752-Q3> , <https://time.example/id/year/1752> ;
	gist:precedesDirectly <https://time.example/id/day/1752-09-15> ;
	gist:startDateTime "1752-09-14T00:00:00"^^xsd:dateTime .
```

The document starts with its prefixes, and goes on to the day's duration, its neighbours,
containers and hours, and the document's own formats. Week 36 holds seven days across the
calendar change, from Monday 31 August to Sunday 17 September.

Every interval is a `gist:TimeInterval` with a start, an end and a duration in seconds.
Containment is `gist:isDirectPartOf` and `gist:isPartOf`, order is `gist:precedesDirectly`,
and a point is a `gist:GeoPoint`. The calendar classes, the calendars and the categories
come from the placetime ontology, which extends gist and adds no properties. It ships in
`crates/semops-placetime/ontology/` with SHACL shapes that check a description, and gist
itself beside it. `docs/decisions/0007-placetime-in-gist.md` sets out the model.

## Running it for real

Run it as a service ([Installing](02-installing.md#service)) or in a container, with
`--store` on a disk of its own, and put a front door in front of it if anyone but you will
reach it. `holos/deploy/README.md` walks through nine deployment patterns, from a demo to a
locked-down endpoint, and `holos/OPERATIONS.md` is the operator's reference.
