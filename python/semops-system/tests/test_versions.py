"""The package installs exactly the engine releases versions.toml says belong together."""

import tomllib
from importlib.metadata import requires
from pathlib import Path

import semops_system

VERSIONS = Path(__file__).resolve().parents[3] / "versions.toml"


def pinned_in_versions_toml() -> dict[str, str]:
    pins = tomllib.loads(VERSIONS.read_text(encoding="utf-8"))
    return {dist: engine["version"] for engine in pins["engines"].values() for dist in engine.get("pypi", [])}


def test_dependencies_are_the_versions_toml_pins():
    declared = {}
    for req in requires("semops-system") or []:
        if "extra ==" in req:
            continue
        name, _, version = req.partition("==")
        declared[name.strip()] = version.strip()
    assert declared == pinned_in_versions_toml()


def test_installed_engines_are_the_pinned_ones():
    assert semops_system.ENGINES == pinned_in_versions_toml()
