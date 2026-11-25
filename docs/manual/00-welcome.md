# The SemOps System Manual {#welcome}

::: standfirst
SemOps System is one toolchain for knowledge graphs: an RDF 1.2 store and SPARQL 1.2
server, a SHACL validator, a mapper from tables to RDF, and the ontology quality checks
that gate a release. This manual says how to install it, how to run each part, and how to
check what you build with it.
:::

```{=html}
<div class="facts">
<div class="fact"><strong>RDF 1.2</strong><span>and SPARQL 1.2, query and update</span></div>
<div class="fact"><strong>SHACL</strong><span>1.0 and 1.2, with SHACL-SPARQL and rules</span></div>
<div class="fact"><strong>61</strong><span>ontology quality checks</span></div>
<div class="fact"><strong>140</strong><span>SPARQL extension functions</span></div>
</div>
```

The toolchain used to be about ten separately versioned packages. It is now one Cargo
workspace, built, tested and released together, so every part you install from one
version agrees with every other part. The commands in this manual were run against that
workspace, and their output is reproduced as the software printed it.

## How the manual is arranged

| Chapter | What it covers |
|---|---|
| [What SemOps System is](01-what-it-is.md) | The parts, what each is for, and how they fit together |
| [Installing](02-installing.md) | Every way to install it: from source, in a container, from Python or npm, as a service, on Azure |
| [A first run](03-first-run.md) | Twenty minutes from a CSV file to a validated, checked, queryable graph |
| [Running the server](04-server.md) | The SPARQL endpoint, the Graph Store, identity, limits, and the placetime URIs |
| [Checking an ontology](05-checking.md) | `semops check`, the 61-check registry, and using it as a CI gate |
| [Validating with SHACL](06-validating.md) | The `shacl` command and validation inside the store |
| [Mapping tables to RDF](07-mapping.md) | tarka: TARQL queries and OTTR templates over CSV, Parquet and data frames |
| [Stores, backups and network storage](08-storage.md) | Persistent stores, checkpoints, read replicas, and which network filesystems are safe |
| [Security](09-security.md) | What the server trusts, what it refuses, and the settings for an exposed deployment |
| [Running on Azure](10-azure.md) | AKS and Container Apps, Entra ID, and the NFS storage options |
| [Reference](11-reference.md) | Every server flag, every environment variable, the exit codes |
| [What is not finished](12-not-finished.md) | The parts of the plan that are not built yet, and what to use meanwhile |

## Conventions

Commands are shown on their own, without a prompt. Where the output matters it follows in
a box with a dashed edge, trimmed where it is long:

```{.output}
holos-server 0.31.0 (<commit>) listening on http://127.0.0.1:7878
```

File paths are relative to the root of the workspace, the directory you cloned. Where a
command differs between Linux or macOS and Windows, both are given.

## Other editions

The manual is published three ways from the same text: this web edition, an EPUB for
e-readers, and an A4 PDF for print. The source is the `docs/manual/` directory of the
[SemOps System repository](https://github.com/pwin/SemopsSystem), and
`python docs/manual/build.py` builds all three.
