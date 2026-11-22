"""The capability floor (docs/decisions/0003), as checks a release must pass.

Each test exercises one thing SemOps System must do through the package's own names:
RDF 1.2, SPARQL 1.2, SEP-0002 dates, GeoSPARQL, the extension function libraries, SHACL
through both the store and the standalone engine, and tarka mapping, including the
SPARQL 1.2 step that lifts tarka's RDF 1.1 output to RDF 1.2.
"""

import polars as pl
import pytest

import semops_system as so
from semops_system.holos import Store

EX = "http://example.org/"
PREFIXES = f"""
PREFIX ex: <{EX}>
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
PREFIX geo: <http://www.opengis.net/ont/geosparql#>
PREFIX geof: <http://www.opengis.net/def/function/geosparql/>
PREFIX fn: <http://www.w3.org/2005/xpath-functions#>
PREFIX afn: <http://jena.apache.org/ARQ/function#>
PREFIX spif: <http://spinrdf.org/spif#>
PREFIX sh: <http://www.w3.org/ns/shacl#>
"""


def ask(store: Store, where: str) -> bool:
    result = store.query(PREFIXES + f"ASK {{ {where} }}")
    assert isinstance(result, bool)
    return result


def one(store: Store, select: str, var: str):
    rows = store.query(PREFIXES + select)
    assert len(rows) == 1, rows
    return rows[0].term(var)


def test_submodules_are_the_engines_modules():
    import semops_system.holos
    from semops_system.shacl import Shapes  # noqa: F401
    from semops_system.tarka import Mapping  # noqa: F401

    assert semops_system.holos is so.holos is so._native._holosdb
    assert so.shacl is so._native.shacl and so.tarka is so._native.tarka_rdf


def test_persistent_store_is_compiled_in():
    assert so.holos.has_rocksdb()


# ---------------------------------------------------------------- the quality checks

ONTOLOGY = """
@prefix owl:  <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <https://example.org/shop#> .

<https://example.org/shop> a owl:Ontology .
ex:Shop a owl:Class ; rdfs:label "Shop"@en .
ex:Bookshop a owl:Class ; rdfs:subClassOf ex:Shop .
"""


def test_the_quality_checks_run_on_text_and_files(tmp_path):
    checks = so.list_checks()
    assert len(checks) == 61 and all(c["id"] and c["default_severity"] for c in checks)
    run = so.check_text(ONTOLOGY, "ttl")
    assert run["complete"], run["failed"]
    assert {f["severity"] for f in run["findings"]} <= {"Violation", "Warning", "Info"}
    # The same ten findings `semops check` gives on this file.
    assert len(run["findings"]) == 10
    path = tmp_path / "shop.ttl"
    path.write_text(ONTOLOGY, encoding="utf-8")
    assert len(so.check(str(path))["findings"]) == 10
    assert len(so.check(str(path), engine="sparql")["findings"]) >= 1
    with pytest.raises(ValueError):
        so.check(str(path), engine="owl")


# ---------------------------------------------------------------- RDF 1.2 and SPARQL 1.2


def test_rdf12_triple_terms_and_annotations():
    store = Store()
    store.update(PREFIXES + 'INSERT DATA { ex:alice ex:age 42 {| ex:source ex:census |} }')
    # The annotation reifies the asserted triple: a reifier with rdf:reifies <<( … )>>.
    assert ask(store, "ex:alice ex:age 42 . ?r rdf:reifies <<( ex:alice ex:age 42 )>> ; ex:source ex:census")


def test_sparql12_triple_functions():
    store = Store()
    store.update(PREFIXES + "INSERT DATA { ex:r rdf:reifies <<( ex:a ex:p ex:b )>> }")
    assert one(store, "SELECT ?s WHERE { ?r rdf:reifies ?t FILTER(isTRIPLE(?t)) BIND(SUBJECT(?t) AS ?s) }", "s") == f"<{EX}a>"


def test_sep0002_dates_compare_by_value():
    assert ask(Store(), 'FILTER("2024-01-31"^^xsd:date < "2024-11-04"^^xsd:date)')
    assert ask(Store(), 'FILTER("2024-01-31T00:00:00Z"^^xsd:dateTime + "P1M"^^xsd:yearMonthDuration > "2024-02-01T00:00:00Z"^^xsd:dateTime)')


# ---------------------------------------------------------------- GeoSPARQL


def test_geosparql_functions_are_present():
    names = so.holos.geosparql_functions()
    assert any(n.endswith("sfWithin") for n in names)
    assert any(n.endswith("buffer") for n in names)


def test_geosparql_topology_and_distance():
    store = Store()
    assert ask(store, 'FILTER(geof:sfWithin("POINT(1 1)"^^geo:wktLiteral, "POLYGON((0 0, 0 2, 2 2, 2 0, 0 0))"^^geo:wktLiteral))')
    assert not ask(store, 'FILTER(geof:sfWithin("POINT(3 3)"^^geo:wktLiteral, "POLYGON((0 0, 0 2, 2 2, 2 0, 0 0))"^^geo:wktLiteral))')


# ---------------------------------------------------------------- extension functions


@pytest.mark.parametrize(
    ("expr", "expected"),
    [
        ('fn:upper-case("semops")', '"SEMOPS"'),
        ('fn:substring-before("tarka-rdf", "-")', '"tarka"'),
        ("afn:localname(<http://example.org/thing>)", '"thing"'),
        ('spif:titleCase("semops system")', '"Semops System"'),
        ('spif:buildString("{?1}-{?2}", "a", "b")', '"a-b"'),
    ],
)
def test_extension_functions_in_store_queries(expr, expected):
    assert one(Store(), f"SELECT ?v WHERE {{ BIND({expr} AS ?v) }}", "v") == expected


# ---------------------------------------------------------------- SHACL

SHAPES = f"""
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix ex: <{EX}> .
ex:PersonShape a sh:NodeShape ;
  sh:targetClass ex:Person ;
  sh:property [ sh:path ex:age ; sh:datatype xsd:integer ; sh:maxCount 1 ] ;
  sh:sparql [
    sh:message "an adult must be 18 or over" ;
    sh:select "SELECT $this WHERE {{ $this <{EX}adult> true ; <{EX}age> ?a FILTER(?a < 18) }}"
  ] .
"""
DATA_BAD = f"""
@prefix ex: <{EX}> .
ex:bob a ex:Person ; ex:age 12 ; ex:adult true .
"""
DATA_GOOD = f"""
@prefix ex: <{EX}> .
ex:carol a ex:Person ; ex:age 40 ; ex:adult true .
"""


def test_shacl_engine_core_and_sparql_constraints():
    shapes = so.shacl.Shapes.from_turtle(SHAPES)
    assert shapes.validate_turtle(DATA_GOOD).conforms
    report = shapes.validate_turtle(DATA_BAD)
    assert not report.conforms
    assert len(report.results) == 1


def store_with(tmp_path, data: str) -> Store:
    path = tmp_path / "data.ttl"
    path.write_text(data, encoding="utf-8")
    store = Store()
    store.load(str(path))
    return store


def shapes_file(tmp_path, text: str) -> str:
    path = tmp_path / "shapes.ttl"
    path.write_text(text, encoding="utf-8")
    return str(path)


CORE_SHAPES = SHAPES.split("  sh:sparql [")[0].rstrip().rstrip(";") + " .\n"
DATA_TWO_AGES = f"""
@prefix ex: <{EX}> .
ex:dave a ex:Person ; ex:age 30, 31 .
"""


@pytest.mark.parametrize("engine", ["native", "adapted"])
def test_store_validates_core_with_both_validators(engine, tmp_path):
    store = store_with(tmp_path, DATA_TWO_AGES)
    result = store.validate(shapes_file(tmp_path, CORE_SHAPES), engine=engine)
    assert result["conforms"] is False
    assert result["violations"] == 1  # sh:maxCount 1


def test_store_validates_shacl_sparql_with_the_adapted_validator(tmp_path):
    store = store_with(tmp_path, DATA_BAD)
    result = store.validate(shapes_file(tmp_path, SHAPES), engine="adapted")
    assert result["conforms"] is False
    assert result["violations"] == 1


def test_native_validator_checks_shacl_sparql_too(tmp_path):
    """The native, incremental validator checks SHACL Core itself and hands SHACL-SPARQL
    constraints to the adapted engine, merging the reports. (HOLOS 0.27.0, which the bridge
    release installed, refused such shapes instead.)"""
    bad = store_with(tmp_path, DATA_BAD).validate(shapes_file(tmp_path, SHAPES), engine="native")
    assert bad["conforms"] is False and bad["violations"] == 1
    assert bad["results"][0]["component"].endswith("#SPARQLConstraintComponent")
    good = store_with(tmp_path, DATA_GOOD).validate(shapes_file(tmp_path, SHAPES), engine="native")
    assert good["conforms"] is True


# ---------------------------------------------------------------- tarka, and the RDF 1.2 lift

TARQL = f"""
PREFIX ex: <{EX}>
PREFIX apf: <http://jena.apache.org/ARQ/property#>
CONSTRUCT {{ ?person ex:tag ?tag }}
WHERE {{
  BIND(IRI(CONCAT("{EX}", ?id)) AS ?person)
  ?tag apf:strSplit (?tags ";")
}}
"""


def tarka_output() -> pl.DataFrame:
    frame = pl.DataFrame({"id": ["alice", "bob"], "tags": ["red;blue", "green"]})
    return so.tarka.Mapping.tarql(TARQL).triplify(frame)


def test_tarka_maps_with_apf_property_functions():
    triples = tarka_output()
    assert triples.height == 3
    assert set(triples["object"]) == {'"red"', '"blue"', '"green"'}


def test_sparql12_lifts_tarka_output_to_rdf12():
    """tarka stays RDF 1.1; a SPARQL 1.2 update in the engine builds the RDF 1.2."""
    store = Store()
    lines = " ".join(f"{s} {p} {o} ." for s, p, o in tarka_output().iter_rows())
    store.update(f"INSERT DATA {{ {lines} }}")
    # Annotate every mapped tag with where it came from: each asserted triple gets a reifier.
    store.update(
        PREFIXES
        + "INSERT { ?s ex:tag ?t {| ex:source ex:spreadsheet |} } WHERE { ?s ex:tag ?t }"
    )
    rows = store.query(PREFIXES + "SELECT ?r WHERE { ?r rdf:reifies <<( ?s ex:tag ?t )>> ; ex:source ex:spreadsheet }")
    assert len(rows) == 3
