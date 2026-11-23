"""The `semops` command the wheel installs: the binary's command, run through the extension."""

import json
import subprocess
import sys

import semops_system

ONTOLOGY = """
@prefix owl:  <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <https://example.org/shop#> .
<https://example.org/shop> a owl:Ontology .
ex:Shop a owl:Class ; rdfs:label "Shop"@en .
ex:Bookshop a owl:Class ; rdfs:subClassOf ex:Shop .
"""


def semops(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, "-m", "semops_system", *args],
        capture_output=True,
        text=True,
        encoding="utf-8",
    )


def test_the_command_is_the_workspace_release():
    run = semops("--version")
    assert run.returncode == 0, run.stderr
    assert run.stdout.strip() == f"semops {semops_system.__version__}"


def test_every_engine_is_a_subcommand():
    for command, name in (("serve", "holos-server"), ("validate", "shacl"), ("map", "tarka")):
        run = semops(command, "--version")
        assert run.returncode == 0, run.stderr
        assert run.stdout.startswith(name + " "), run.stdout


def test_check_reports_and_gates(tmp_path):
    path = tmp_path / "shop.ttl"
    path.write_text(ONTOLOGY, encoding="utf-8")
    run = semops("check", str(path), "--format", "json", "--fail-on", "Warning")
    assert run.returncode == 1, "the ontology has warnings"
    findings = json.loads(run.stdout)
    assert len(findings) == 10
    sarif = semops("check", str(path), "--format", "sarif", "--fail-on", "never")
    assert sarif.returncode == 0
    assert json.loads(sarif.stdout)["version"] == "2.1.0"


def test_a_missing_file_is_exit_2(tmp_path):
    run = semops("check", str(tmp_path / "absent.ttl"))
    assert run.returncode == 2
    assert "absent.ttl" in run.stderr
