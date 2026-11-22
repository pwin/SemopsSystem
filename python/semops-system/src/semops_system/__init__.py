"""SemOps System: the pwin RDF toolchain as one Python package.

    import semops_system as so

    store = so.holos.Store()                    # RDF 1.2 store, SPARQL 1.2, GeoSPARQL, SHACL
    shapes = so.shacl.Shapes.from_turtle(...)   # the SHACL engine on its own
    mapping = so.tarka.Mapping.tarql(...)       # tabular data to RDF 1.1
    run = so.check("ontology.ttl")              # the 61 ontology quality checks

One extension module, ``semops_system._native``, holds all of it, built from one commit of
the SemOps System workspace. ``holos``, ``shacl`` and ``tarka`` are the modules the
``holosdb``, ``shacl`` and ``tarka-rdf`` wheels carry, with the same classes and functions,
so their documentation applies unchanged. ``from semops_system.holos import Store`` works
too.

``check`` runs the quality checks as ``semops check`` does: each check as a SHACL shape and
as a SPARQL query, the findings merged. It returns a dict with ``findings`` (a list of
dicts), ``failed`` (the checks whose query did not run) and ``complete``.
"""

from __future__ import annotations

import sys

from . import _native
from ._native import check, check_text, list_checks

holos = _native._holosdb
shacl = _native.shacl
tarka = _native.tarka_rdf

__version__: str = _native.__version__

#: Where each part comes from. Every part is from this one build, so each is the package's
#: own version; the bridge release this replaces installed three separate engine releases.
ENGINES: dict[str, str] = {
    "holosdb": __version__,
    "shacl": __version__,
    "tarka-rdf": __version__,
}

# Registered as submodules, so `import semops_system.holos` and
# `from semops_system.shacl import Shapes` work as well as attribute access.
for _name, _module in (("holos", holos), ("shacl", shacl), ("tarka", tarka)):
    sys.modules[f"{__name__}.{_name}"] = _module
del _name, _module

__all__ = [
    "ENGINES",
    "__version__",
    "check",
    "check_text",
    "holos",
    "list_checks",
    "shacl",
    "tarka",
]
