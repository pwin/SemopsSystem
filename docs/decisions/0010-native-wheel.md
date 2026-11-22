# 0010. One Python wheel: `semops-system`, native

Status: accepted and implemented, 22 November 2026, in `crates/semops-python` and
`python/semops-system`.

## Context

Phase 3 of the proposal ships "one Python wheel (abi3, as `holosdb` already is)", with
`holosdb`, `shacl` and `tarka-rdf` becoming meta-packages that re-export it. The first
release under the name, `semops-system` 0.29.0.dev1, was a bridge: a pure-Python package
that pinned the three engines' own wheels and presented their modules as
`semops_system.holos`, `.shacl` and `.tarka`. It had none of the workspace's own work: the
quality checks and the changes since the engines' last releases were missing.

## Decision

**One extension module, built from the workspace.** `crates/semops-python` is a pyo3
cdylib, `semops_system._native`. It links the three engines' Python modules as libraries
and adds each as a submodule (`_holosdb`, `shacl`, `tarka_rdf`), with the classes and
functions those wheels have. The engines' module functions became `pub` for it, and
`tarka-python` gained an `rlib` target. To them it adds:

- `check(*paths, engine="both")`: the quality checks, as `semops check` runs them;
- `check_text(text, format, engine, base)`: the same over text;
- `list_checks()`.

A run is a dict of `findings`, `failed` (the checks whose query did not run) and
`complete`. The checks release the GIL while they run.

**The names stay.** `semops_system/__init__.py` presents the submodules as `holos`, `shacl`
and `tarka` and registers them in `sys.modules`, as the bridge did. Code written against
the bridge keeps working, which the unchanged capability tests show. `ENGINES` stays, and
each entry is now the package's own version.

**maturin builds it.** `python/semops-system/pyproject.toml` takes maturin as its build
backend. It points at `crates/semops-python` and reads the version from Cargo, so the wheel
is the workspace's release (0.29.0). Its one runtime dependency is `polars`, which tarka
converts data frames through, as the tarka-rdf wheel requires.

**The stable ABI.** Each engine asked for a different abi3 floor (3.9, 3.10 and 3.11). In
one build pyo3 takes the lowest, so the extension is built for the 3.9 limited API, and the
package requires Python 3.11, as holosdb and the bridge did.

**Wheels for five platforms, and an sdist.** `.github/workflows/workflow.yml` builds wheels
on HOLOS's platform matrix, with HOLOS's fixes for RocksDB's C++ build:

- Linux x86_64 and aarch64 (manylinux 2_28);
- macOS arm64 and x86_64;
- Windows x64.

It then installs each wheel on Linux, macOS and Windows, at CPython 3.11 and 3.13, and runs
the package's tests against the installed wheel. The filename and the trusted-publisher
jobs are unchanged.

**The sdist builds.** maturin copies a crate's path dependencies into the sdist, but not the
`[patch]` targets the workspace uses for two Oxigraph crates in `vendor/`, so the first
sdist could not resolve. `semops-python` now names both as path dependencies, which puts
them in the archive. The manifest of each crate in `vendor/` gained
`exclude = ["Cargo.toml.orig"]`, because
`cargo package` refuses a file of that name. The CI sdist job checks that the extracted
archive resolves under its lock file.

## Not done

- **The old names as meta-packages.** `holosdb`, `shacl` and `tarka-rdf` still publish as
  their own wheels from their own repositories. Making each depend on and re-export
  `semops-system` is a change to three published packages, and waits for this one to be
  released.
- **The `semops` command in the wheel.** `pip install semops-system` gives the modules,
  not the command line.
- **musllinux and Windows on Arm wheels.** pip builds those platforms from the sdist.
