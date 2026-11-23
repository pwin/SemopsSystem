# Changes

SemOps System's own releases. The engines keep their own changelogs, in `holos/`, `shacl/`
and `tarka/`, for their history before the workspace.

## 0.30.0, 23 November 2026

- `semops diff OLD NEW` compares two versions of an ontology and suggests a semantic-version
  bump, as the Ontology Quality Suite's `version-diff` does (#14).
- `semops repair FILE` applies the quality checks' repairs to a file, as the suite and the
  VS Code extension do (#15).
- `pip install semops-system` installs the `semops` command, with every subcommand (#17).
- The repository is an `azd` template for the Container Apps option (#16).

## 0.29.0, 23 November 2026

The first release under the workspace's version. On PyPI, `semops-system` 0.29.0 is the
first native wheel, replacing the 0.29.0.dev1 bridge to the engines' separate wheels.

- One Cargo workspace for HOLOS, SHACL_Engine and tarka, with one version and one CI.
- `semops check`: the 61 ontology quality checks, run natively (#4), with JUnit and SARIF
  reports for CI (#12).
- `semops`, one binary: `serve`, `load`, `query`, `update`, `validate`, `map`, `check` and
  the store commands, with the old command names as aliases of it (#9).
- `semops-wasm` and `semops-wasm-node`: the store, the validator and the quality checks in
  one WebAssembly module (#10).
- `semops-system`, one native Python wheel carrying the three engines and the checks (#11).
- Placetime: the reference.data.gov.uk interval URIs, WGS84 points, and a gist 14.1.0
  vocabulary for them (#3, #7, #8).
- Azure: a Helm chart for AKS, Bicep for Container Apps with NFS storage, an API
  Management identity policy, and NFS-safe storage in the server (#5).
- A release workflow for the binaries and the multi-arch container image (#13), and the
  manual as a website, an EPUB and a PDF (#6).
