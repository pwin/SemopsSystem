# Security {#security}

`holos-server` is built to sit behind something that authenticates people, and to apply
policy to what that thing has verified. This chapter says what the server trusts, what it
refuses by default, and what to set before anyone but you can reach it.

## What the server trusts

Nothing a client says about itself, unless you tell it to. Without a trust flag every
request is anonymous, whatever headers it carries. With `--trust-forwarded-identity` it
reads `X-Holos-Principal`, `X-Holos-Roles` and `X-Holos-Clearance`; with
`--trust-azure-easy-auth`, Azure built-in authentication's `X-MS-CLIENT-PRINCIPAL`.

Those flags move the trust to whatever sets the headers. That has two consequences, and
both must hold:

1. **The front door strips what the client sent** under those names before it sets its
   own. `holos/deploy/Caddyfile` and the API Management policy in `deploy/azure/apim/` do.
2. **Nothing reaches the server except through the front door.** Bind it to the loopback
   address behind a proxy on the same machine, or close its port to everything but the
   proxy: a firewall rule, a Kubernetes NetworkPolicy, a private network.

The policy itself (which graphs a role may read, predicates nobody may see, classification
labels) is in `holos/ACCESS-CONTROL.md`. It is applied at the index scan, so it holds for
every query shape.

## What it refuses by default

| | Default | Why |
|---|---|---|
| Request bodies over 256 MiB | 413 | A body is read into memory before anything else looks at it |
| `LOAD <file:…>` | 400 unless `--load-dir` is set | Otherwise any client that may update could read any file the server can |
| Queries past 8 GiB of growth | Refused, the process carrying on | One runaway query should not take every other request down with it |
| RDF/XML with over 256 attributes on an element, or over 4,096 namespace declarations | 400 | The XML parser underneath has two denial-of-service defects of exactly that shape |
| A panic while answering a request | The connection drops; the server carries on | One bad request should not stop the others |
| `LOAD` of an `http(s)` URL, `SERVICE` | Refused | A server that fetches URLs a client names can be turned against hosts only it can reach |

## Before you expose it

- [ ] Run it as its own user, never root. The container image and the service scripts do.
- [ ] Put it behind a front door that ends TLS and authenticates, and choose the trust flag
      that matches it. Leave both off for an anonymous public endpoint.
- [ ] Close the port to everything except the front door.
- [ ] Set `--read-only` on an endpoint nobody should write to, or `--store-read-only` on a
      replica.
- [ ] Set `--timeout`: 300 seconds is a sensible start.
- [ ] Set `--max-query-memory` to a third of the memory you can spare, at most.
- [ ] Leave `--load-dir` unset unless clients must load files, and then name a directory
      that holds only what they may load.
- [ ] Turn the console off with `--no-ui` on a server: it loads scripts from a CDN.
- [ ] Keep backups (`--backup-dir`), and try restoring one.
- [ ] Pin the image by digest, and scan it.

## Known weaknesses

The RDF/XML parser depends on a version of `quick-xml` with two published denial-of-service
defects (RUSTSEC-2026-0194 and RUSTSEC-2026-0195). No release of the parser has the fix
yet. The server refuses documents of the shapes that trigger them, which closes the network
path; the parser itself moves to a fixed `quick-xml` when one is released. A crate under
tarka's Parquet support has the same two defects, reachable only from a malicious cloud
storage endpoint. `docs/decisions/0006-azure-nfs-and-hardening.md` keeps the list.
