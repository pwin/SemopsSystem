#!/usr/bin/env python3
"""Check that every consumer of the toolchain runs the releases in versions.toml.

    python scripts/check_pins.py --repos DIR

DIR holds a checkout of each consumer repository, named as in versions.toml. A consumer
whose checkout is absent is reported and skipped. The packages checked are the platform's
(semops-system, semops-wasm, semops-wasm-node) and each engine's. A consumer whose lockfiles
name none of them is reported as unchecked, since it cannot be said to be in line. The exit
status is 1 if any locked version differs from versions.toml or a consumer is unchecked, 2 if
nothing could be checked, and 0 otherwise.

Standard library only, so it runs anywhere Python 3.11+ does.
"""

from __future__ import annotations

import argparse
import json
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def expected_versions(pins: dict) -> dict[tuple[str, str], str]:
    """(ecosystem, package name) -> the version versions.toml says it must be.

    The platform's packages and each engine's. Leaving out the platform's made a consumer that
    had moved to semops-wasm invisible: its lockfile named no package this looked for, so it
    was neither in line nor out of it, and the report said nothing about it.
    """
    out: dict[tuple[str, str], str] = {}
    for component in [pins["platform"], *pins["engines"].values()]:
        for ecosystem in ("pypi", "npm"):
            names = component.get(ecosystem, [])
            for name in [names] if isinstance(names, str) else names:
                out[(ecosystem, name)] = component["version"]
    return out


def locked_npm(path: Path) -> dict[str, str]:
    """Package name -> version, from a package-lock.json (lockfile v2 or v3)."""
    data = json.loads(path.read_text(encoding="utf-8"))
    found: dict[str, str] = {}
    for key, entry in data.get("packages", {}).items():
        if not key:
            continue  # the root project itself
        name = key.rsplit("node_modules/", 1)[-1]
        # A nested copy (node_modules/a/node_modules/b) is still something that runs.
        if "version" in entry:
            found.setdefault(name, entry["version"])
            if found[name] != entry["version"]:
                found[name] = f"{found[name]}, {entry['version']}"
    return found


def locked_uv(path: Path) -> dict[str, str]:
    """Package name -> version, from a uv.lock."""
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    return {p["name"]: p["version"] for p in data.get("package", []) if "version" in p}


READERS = {"package-lock.json": ("npm", locked_npm), "uv.lock": ("pypi", locked_uv)}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--repos", type=Path, required=True, help="directory of consumer checkouts")
    parser.add_argument("--pins", type=Path, default=ROOT / "versions.toml")
    args = parser.parse_args()

    pins = tomllib.loads(args.pins.read_text(encoding="utf-8"))
    want = expected_versions(pins)
    checked = 0
    mismatches = 0
    unchecked = 0

    for consumer, spec in pins["consumers"].items():
        repo = args.repos / consumer
        if not repo.is_dir():
            print(f"skip  {consumer}: no checkout at {repo}")
            continue
        checked_before = checked
        read_any = False
        for lockfile in spec["lockfiles"]:
            path = repo / lockfile
            if not path.is_file():
                print(f"skip  {consumer}/{lockfile}: not present")
                continue
            ecosystem, read = READERS[Path(lockfile).name]
            locked = read(path)
            read_any = True
            for (eco, name), version in sorted(want.items()):
                if eco != ecosystem or name not in locked:
                    continue
                checked += 1
                if locked[name] == version:
                    print(f"ok    {consumer}: {name} {version}")
                else:
                    mismatches += 1
                    print(f"DRIFT {consumer}: {name} is {locked[name]}, versions.toml says {version}")
        if read_any and checked == checked_before:
            unchecked += 1
            print(f"NONE  {consumer}: its lockfiles name no package versions.toml pins")

    if checked == 0:
        print("nothing checked: no consumer lockfile names a pinned package")
        return 2
    print(f"\n{checked} pins checked, {mismatches} out of line, {unchecked} consumers unchecked")
    return 1 if mismatches or unchecked else 0


if __name__ == "__main__":
    sys.exit(main())
