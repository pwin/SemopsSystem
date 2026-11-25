# What is not finished {#not-finished}

SemOps System follows a plan with four phases, set out in the proposal it was built from.
This chapter lists the parts of that plan this version does not have, so you can tell a
gap from a mistake, and says what to use meanwhile.

## Packaging

| Not yet | Meanwhile |
|---|---|
| Installers: winget, Homebrew, `.deb`, `.rpm` | The release archives |

## Checks and validation

| Not yet | Meanwhile |
|---|---|
| The suite's Python-only checks in `semops check`: reasoner-backed checks, version comparison, TARQL analysis (17 of the 61) | The Ontology Quality Suite, `pip install ontology-quality-suite` |
| `semops check --inference rdfs`, severity policies, repairs, HTML reports | The suite |
| One SHACL validator: HOLOS still carries two, beside the `shacl` command | Each works, and they agree on the W3C suites |
| User-defined SHACL-AF functions (`sh:SPARQLFunction`) | Write the logic into the constraint's query |

## Azure

| Not yet | Meanwhile |
|---|---|
| A deployment of the templates to a subscription | The templates are linted and schema-checked in CI; the first deployment confirms resource versions |
| OTLP over gRPC or TLS from the server, so it can send to the Container Apps managed agent or a remote collector directly | An OpenTelemetry Collector beside it, receiving plain HTTP ([chapter 10](10-azure.md#telemetry)) |
| Marketplace listings for the GitHub Action and an Azure DevOps extension | The Helm chart and the Bicep template; `semops check --format junit` or `sarif` in a script step |

## Security

The RDF/XML parser waits for a release built on a fixed `quick-xml`; until then the server
refuses the documents that would trigger its two defects. [Security](09-security.md#known-weaknesses)
has the detail.
