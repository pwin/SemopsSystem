# Where `resources/` comes from

Copied from [pwin/consolidated-ontology-quality-suite-python](https://github.com/pwin/consolidated-ontology-quality-suite-python)
at commit `d9300e62b8c142367ad14f52d3d2fd7ff6d6021b` (release 0.28.0, 20 November 2026),
`ontology_suite/resources/`:

| Here | There | What it is |
| --- | --- | --- |
| `resources/registry.json` | `resources/registry.json` | The 61-check registry: id, category, severity, title, description, remediation |
| `resources/shapes/*.ttl` | `resources/shapes/*.ttl` | The SHACL formulation of the checks |
| `resources/sparql/**/*.rq` | `resources/sparql/**/*.rq` | The portable formulation: self-contained CONSTRUCT queries producing `sh:ValidationResult`s |
| `resources/repairs/` | `resources/repairs/` | SPARQL Update repair templates and their manifest |

Not copied: `resources/templates/documentation-template.html`, which is the suite's
documentation generator's page, not part of the registry.

Line endings were normalised to LF. Nothing else was changed.

The same files ship inside the VS Code extension
(`consolidated_ontology_suite_webapp/resources/checks-registry/`). This crate is meant to
become the one copy the suite, the extension and the `semops` command all read (proposal,
phase 2), so until the suite and the extension read it, the three copies must be kept equal.
`tests/registry.rs` compares this copy with a checkout of the suite, character for
character apart from line endings, when `ONTOLOGY_SUITE` names one or the checkout sits at
`../upstream/consolidated-ontology-quality-suite-python` beside this workspace; it skips
otherwise.

## Updating

Copy the four paths above from the suite at the new commit, normalise line endings, update
the commit and release here and in `SUITE` in `src/lib.rs`, and regenerate
`tests/fixtures/expected/` with the suite at the same commit (see `tests/parity.rs`).
