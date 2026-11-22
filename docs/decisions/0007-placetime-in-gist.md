# 0007. Placetime in gist

Status: accepted and implemented, 22 November 2026, in `crates/semops-placetime` (the
ontology, its shapes, and the second describer) and `holos-server`
(`--placetime-vocabulary`).

## Context

[0004](0004-placetime.md) serves the reference.data.gov.uk interval URIs and describes them
as IntervalServer did: the reference.data.gov.uk intervals vocabulary, OWL-Time, SCOVO, and
for points WGS84 and GeoSPARQL. The request was for placetime to work from an ontology based
on gist 14.1.0, Semantic Arts' upper ontology, so that the same URIs can be described to a
graph built on gist.

## Decision: a second vocabulary, chosen per server

The URIs and their patterns do not change. What changes is the vocabulary a document is
written in, and the server takes it from configuration:

```sh
holos-server --placetime --placetime-vocabulary gist
```

The default stays `data.gov.uk`. Data already written against reference.data.gov.uk's terms
keeps matching, and the documents a client fetched last year read the same today. In the
crate, `resolve` and `describe` keep that vocabulary, and `resolve_with` and `describe_with`
take a `Vocabulary`.

The vocabulary is per server rather than per request. Content negotiation chooses a syntax,
and a profile header (`Accept-Profile`) could choose a vocabulary, but no client asks for
one, and two descriptions behind one URI would make a cached document depend on a header
caches do not key on.

## Decision: the placetime ontology

`crates/semops-placetime/ontology/placetime.ttl` extends gist. It follows gist's own
conventions: gist's three-way namespace split, SKOS annotations with exactly one
`skos:prefLabel` and one `skos:definition` per class, plain string literals, no inverse
properties, `owl:someValuesFrom` rather than minimum cardinality 1, and instance names of
the form `_Type_name`.

| Namespace | Prefix | Holds |
| --- | --- | --- |
| `https://www.semantechs.co.uk/ns/ontology/placetime/` | `pt:` | 16 classes |
| `https://www.semantechs.co.uk/ns/taxonomy/placetime/` | `ptx:` | Days of the week, months, quarters and halves, as `gist:Category` |
| `https://www.semantechs.co.uk/ns/data/placetime/` | `ptd:` | The British, Gregorian and Julian calendars |

**It declares no properties.** Every statement about an interval or a point uses a gist
property:

| Said | With |
| --- | --- |
| What kind of interval | `pt:CalendarYear` … `pt:CalendarSecond`, `pt:Iso8601Week`, `pt:Instant`, or `gist:TimeInterval`; each a subclass of `gist:TimeInterval` |
| When it starts and ends | `gist:startDateTime`, `gist:endDateTime`, as `xsd:dateTime` |
| How long it lasts | `gist:hasMagnitude` a `gist:Magnitude` with `gist:hasAspect gistd:_Aspect_duration`, `gist:hasUnitOfMeasure gistd:_UnitOfMeasure_second` and a `gist:numericValue` in whole seconds |
| Which calendar defines it | `gist:conformsTo` a `pt:Calendar`, which is a `gist:Specification` |
| Which month, quarter, half or weekday | `gist:isCategorizedBy` a category |
| What it is part of | `gist:isDirectPartOf` the next interval up, `gist:isPartOf` the ones above that |
| What comes next | `gist:precedesDirectly` |
| Which URI set it belongs to | `gist:isMemberOf` the set, a `gist:Collection` |
| A point | `gist:GeoPoint` with `gist:latitude` and `gist:longitude` as `xsd:double`, and `geo:asWKT` so GeoSPARQL can use it |

These choices needed deciding:

- **The end is the moment the interval stops**, the start of the next one. The duration is
  then the end less the start, as gist's definition of a time interval implies. SCOVO's
  `scv:max`, the last second inside, has no counterpart.
- **A duration is exact, in seconds.** February 2024 is 2,505,600 seconds, and the British
  year 1752 is 355 days. Calendar durations such as "one month" vary in length, gist 13
  dropped its duration units, and `xsd:duration` is not an OWL 2 datatype, so a nominal
  `P1M` is not recorded.
- **An instant is a time interval of no length**: start equal to end, duration zero. gist
  has no instant class, and this lets an instant be compared with any interval through the
  same three properties. A `pt:Instant` class that is not a time interval, with only
  `gist:atDateTime`, was the alternative. gist's scope note says `gist:atDateTime` is not
  meant to be asserted directly.
- **The unit of duration is `gistd:_UnitOfMeasure_second`, from the gist reference data.**
  Since 13.0 gist ships no units. Semantic Arts publishes them separately, as
  [gistReferenceData](https://github.com/semanticarts/gistReferenceData) (1.0.0, CC BY 4.0,
  much of it from QUDT). There the second is a `gist:UnitOfMeasure` with conversion factor
  1.0 in `_UnitGroup_duration`, the group whose exponent of second is 1, and
  `gistd:_Aspect_duration` measures in that group. A duration in seconds is therefore in the
  base unit and needs no conversion. The ontology repeats the second's type, label,
  conversion factor and group, as gist core repeats the aspects its restrictions use, so a
  duration's unit is described without the reference data loaded. A test checks that the
  reference data makes each of those statements too.
- **Every interval a document names is complete.** Neighbours, containers and parts carry
  their class, calendar, category, start, end and duration as well as their label. One
  duration node is shared by every interval of that length in the document. A minute's
  document is about 630 triples and a day's about 320.
- **The document layer is unchanged.** `foaf:Document`, `foaf:primaryTopic`,
  `dcterms:hasFormat`, and VoID for the URI sets' patterns and examples describe the HTTP
  documents and the URI space, not the things in them. They stay, labelled with
  `skos:prefLabel` instead of `rdfs:label`.
- **The class axioms state structure, not data constraints.** A month is directly part of
  some quarter and categorised by some month of the year, and the interval classes are
  pairwise disjoint. Each class keeps `rdfs:seeAlso` to its reference.data.gov.uk
  counterpart, for anyone mapping between the two.

The namespace is on semantechs.co.uk, which nobody resolves yet. The terms are names, as
reference.data.gov.uk's are, and the ontology ships in the crate (`semops_placetime::ONTOLOGY`).

## Decision: shapes, and the tests built on them

`ontology/placetime-shapes.ttl` (`semops_placetime::SHAPES`) checks a description in SHACL:

- each time interval has one start, one end no earlier than the start, and one duration in
  whole seconds;
- the duration equals the end less the start, a SHACL-SPARQL constraint;
- each calendar interval conforms to one calendar;
- a half year, quarter, month or day carries one category of the right kind;
- an instant starts and ends together;
- a point's latitude and longitude are in range.

The tests run them on the workspace's SHACL engine over documents of every kind. The
samples include the British year of the change, the gap week, a leap February, a Julian leap
century, an instant, intervals and points, plus the URI sets. The ontology sits beside each
document, so its `rdfs:subClassOf` statements carry the targets up to `gist:TimeInterval`.
Tampered documents prove the shapes fire: a duration one second out, a missing calendar and
category, an instant with length. Further tests check two things against the release: every
gist and placetime term a document uses is declared in gist 14.1.0 or the ontology, and the
ontology declares no property of its own. HOLOS queries gist documents in the server's
tests: the British 1752's months sum to 355 days, the day before the gap ends as
14 September begins, and GeoSPARQL finds gist points.

`semops check` over the ontology with gist finds no violations and nothing about a placetime
term. Without gist loaded it reports gist's properties as undeclared, since it does not
follow `owl:imports`.

## gist and its reference data are bundled

`ontology/gist/gistCore14.1.0.ttl` is the 14.1.0 release, byte for byte (marked `-text` in
`.gitattributes`, so its mixed line endings and checksum survive). It is there so the
ontology's `owl:imports` resolves without a network, and so the tests check terms against
the release the ontology names.

`ontology/gist/uomReferenceData-starter-set.ttl` is the reference data's starter set at
release 1.0.0, also unchanged. The full set is 1.7 MB and the starter set holds the units of
time. It has no ontology header, so it cannot be imported: load it beside gist, as its README
says. The tests check the unit and aspect against it.

Both are CC BY 4.0, which allows redistribution with attribution. The crate's NOTICE gives
the attribution, the sources and the SHA-256s.

`semops check` over the ontology, gist and the starter set together finds nothing about a
placetime term. Its 75 violations are in the reference data and gist. 68 of them are
disciplines the starter set categorises by and does not define.

## Not done

- Serving the ontology from the placetime server, at a `def/` path. Its namespace is not this
  host's.
- A per-request choice of vocabulary (see above).
- Zoned instants and intervals, as in 0004.
- Tying a month's category to its start in the shapes. A Julian month starts on a different
  Gregorian date, so such a check would hold only for the Gregorian calendar. The Rust tests
  check the category of the intervals they describe.
