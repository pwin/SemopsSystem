"""One build: every part of the package comes from the same release of the workspace."""

import tomllib
from importlib.metadata import requires, version
from pathlib import Path

import semops_system

CARGO = Path(__file__).resolve().parents[3] / "Cargo.toml"


def test_the_version_is_the_workspace_release():
    workspace = tomllib.loads(CARGO.read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    assert semops_system.__version__ == workspace
    assert version("semops-system") == workspace


def test_every_part_is_from_this_build():
    assert set(semops_system.ENGINES.values()) == {semops_system.__version__}
    assert semops_system.holos.__version__ == semops_system.__version__


def test_nothing_else_is_required():
    # The bridge release depended on holosdb, shacl and tarka-rdf; the native wheel carries
    # them. polars, which tarka converts data frames through, is the `polars` extra.
    requirements = requires("semops-system") or []
    assert [r for r in requirements if "extra ==" not in r] == []
    assert any(r.startswith("polars") and 'extra == "polars"' in r.replace("'", '"') for r in requirements)
