"""SemOps System: the pwin RDF toolchain as one Python package.

    import semops_system as so

    store = so.holos.Store()            # RDF 1.2 store, SPARQL 1.2, GeoSPARQL, SHACL
    shapes = so.shacl.Shapes.from_turtle(...)   # the SHACL engine on its own
    mapping = so.tarka.Mapping.tarql(...)       # tabular data to RDF 1.1

`holos`, `shacl` and `tarka` are the `holosdb`, `shacl` and `tarka_rdf` modules themselves,
not copies, so their documentation applies unchanged and objects pass freely between code
that imports either name. `from semops_system.holos import Store` works too.

This release installs the engine releases listed in `ENGINES`, which are the ones that
agree with each other. The native wheel that replaces it keeps these names.
"""

from __future__ import annotations

import sys
from importlib.metadata import version as _version

import holosdb as holos
import shacl
import tarka_rdf as tarka

__version__ = _version("semops-system")

#: The engine release each part of the package comes from.
ENGINES: dict[str, str] = {
    "holosdb": _version("holosdb"),
    "shacl": _version("shacl"),
    "tarka-rdf": _version("tarka-rdf"),
}

# Registered as submodules, so `import semops_system.holos` and
# `from semops_system.shacl import Shapes` work as well as attribute access.
for _name, _module in (("holos", holos), ("shacl", shacl), ("tarka", tarka)):
    sys.modules[f"{__name__}.{_name}"] = _module
del _name, _module

__all__ = ["ENGINES", "__version__", "holos", "shacl", "tarka"]
