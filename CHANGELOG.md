# Changes

SemOps System's own releases. The engines keep their own changelogs, in `holos/`, `shacl/`
and `tarka/`, for their history before the workspace.

## Unreleased

- A pre-bound `$this` inside an RDF 1.2 triple term, as in
  `?r rdf:reifies <<( $this ?p ?o )>>`, is the focus node. SHACL-SPARQL substituted the focus
  node everywhere but there, so the pattern matched other focus nodes' reified statements:
  a constraint reported the wrong nodes, and the right ones more than once. In `shacl` and in
  HOLOS, for `sh:select` and `sh:ask` validators alike.
- HOLOS binds the variables inside an RDF 1.2 triple term in a `SERVICE` clause. They were
  left out of the query sent to the service, so in
  `SERVICE <…> { ?r rdf:reifies <<( ?s ?p ?o )>> }` only `?r` came back bound.
- HOLOS refuses a rule whose `CONSTRUCT` template has a blank node inside a triple term, as
  it already refused one outside: the blank node is fresh every round, so the rule would
  never stop deriving.

## 0.31.0, 25 November 2026

- Each engine is published from the workspace on its own, at the workspace version:
  `holosdb`, `shacl` and `tarka-rdf` on PyPI, and `holos-wasm`, `shacl-wasm` and their
  `-node` builds on npm, by `engines.yml`. The engines' old repositories stop releasing
  (#23).
- Each release carries the engines' commands on their own, beside `semops` (#23).
- `semops-system` no longer requires Polars. `pip install "semops-system[polars]"` adds it,
  for mapping data frames (#23).
- A SPARQL constraint's result carries `sh:value` when `?value` is a term the query computed
  (`BIND`, an aggregate, `STR()`) and neither graph holds, as SHACL 5.3.2 requires; it used to
  be left out while the message still showed it. `?path` counts only when it is an IRI. In
  `shacl` and in HOLOS (#24, reported as SHACL_Engine#3).
- One SHACL engine. HOLOS validates SHACL-SPARQL, node expressions and rules with `shacl`,
  the crate published on its own, and its adapted copy, `holos-shacl-engine`, is gone.
  `shacl` gains the incremental-revalidation API HOLOS needs: `Graph::apply` and the shapes'
  dependency index. W3C results are unchanged
  ([ADR 0011](docs/decisions/0011-one-shacl-engine.md), #27).
- Every `semops` subcommand takes an Azure Blob Storage URL wherever it takes an input
  file (`abfss://`, `az://`, `https://<account>.blob.core.windows.net/…`), with credentials
  from the environment, a managed identity included
  ([ADR 0012](docs/decisions/0012-blob-urls-as-inputs.md), #28).
- Checkpoints to Blob Storage. `semops upload` and `semops download` copy a file or a
  directory to and from a blob prefix, and `semops backup --to <URL>` checkpoints a store
  and uploads it. The AKS chart's backup job uploads each checkpoint when `backup.blob.url`
  is set, signing in with workload identity, and `deploy/azure/backup/` has a lifecycle
  rule for retention. An output flag given a blob URL is now refused instead of fetched
  ([ADR 0013](docs/decisions/0013-checkpoints-to-blob-storage.md), #29).
- OpenTelemetry from the server: a span and a duration for each request, by the HTTP
  semantic conventions, over OTLP/HTTP, when `OTEL_EXPORTER_OTLP_ENDPOINT` is set. No query
  text is sent. The AKS chart's `telemetry.otlpEndpoint` points the writer and replicas at
  a collector, and `deploy/azure/telemetry/collector.yaml` forwards to Application Insights
  ([ADR 0014](docs/decisions/0014-telemetry.md), #30).

## 0.30.0, 23 November 2026

- `semops diff OLD NEW` compares two versions of an ontology and suggests a semantic-version
  bump, as the Ontology Quality Suite's `version-diff` does (#14).
- `semops repair FILE` applies the quality checks' repairs to a file, as the suite and the
  VS Code extension do (#15).
- `pip install semops-system` installs the `semops` command, with every subcommand (#17).
- The repository is an `azd` template for the Container Apps option (#16), and a GitHub
  Action and an Azure Pipelines template run `semops check` as a CI gate (#19).
- The first published artifacts: the `semops` binaries and the image `ghcr.io/pwin/semops`
  (#13), `semops-system` on PyPI, and `semops-wasm` and `semops-wasm-node` on npm (#20).

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
