# 0014. Telemetry: OpenTelemetry over OTLP/HTTP

Status: accepted and implemented, 25 November 2026, in
`holos/crates/holos-server/src/telemetry.rs` and the AKS chart.

## Context

The proposal's Azure table: "Export OpenTelemetry traces and metrics to Azure Monitor /
Application Insights." The server logged to standard error and answered `/stats`, and that
was all. Container logs reach Log Analytics, but nothing measured requests: how many, how
long, which failed.

`holos-server` is synchronous by design: `tiny_http` and a pool of worker threads, with no
async runtime. Every request passes through one `dispatch` call, and every response goes out
through one function, `respond_with`.

## Decision

**OpenTelemetry's Rust SDK (0.33), exporting OTLP as protobuf over HTTP, configured only by
the standard variables.** It is off unless `OTEL_EXPORTER_OTLP_ENDPOINT` (or a per-signal
endpoint) is set. `OTEL_SERVICE_NAME` defaults to `holos`. The `OTEL_*` variables for
intervals, resource attributes and turning signals off are honoured as the SDK defines
them. The exporters use the blocking HTTP client on their own threads, so the server gains
no runtime. It is a default feature, `otel`, of `holos-server`, so `semops serve` and the
image have it.

**What is recorded follows the HTTP semantic conventions:**

- a span per request, kind server, named `METHOD route`, with `http.request.method`,
  `http.route` and `http.response.status_code`. A 5xx, or a request that sent no response,
  is an error.
- the histogram `http.server.request.duration` in seconds, with the conventions' buckets
  plus 30, 60, 120 and 300 s, because a SPARQL query may run up to the server's timeout.
- the up-down counter `http.server.active_requests`.

The route is one of a fixed set: the server's own paths, the Graph Store path with
`{graph}` beneath it, `{placetime}`, `*` for `OPTIONS`, and none for a path nothing serves.
That keeps the metrics' cardinality bounded whatever clients send. The status is taken where
it is set: `respond_with` records it in a thread-local, and the worker reads it after
`dispatch`.

**What is not recorded:** the query string, the body, headers, the principal. A SPARQL query
can carry the very values it asks about, and a telemetry backend sits outside the access
policy that decides who may see them. A test sends a query naming a distinctive IRI and
checks that no export contains it.

**Azure through a collector.** An OpenTelemetry Collector receives the export on its HTTP
port and forwards traces and metrics to Application Insights with its `azuremonitor`
exporter. `deploy/azure/telemetry/collector.yaml` is an example. On AKS the chart's
`telemetry.otlpEndpoint` points the writer and the replicas at it, tagging each
`holos.role=writer` or `replica`. On Container Apps the collector runs as a sidecar. The
managed OpenTelemetry agent was the obvious route there, but it accepts only gRPC, and its
Application Insights destination takes no metrics (Microsoft's documentation, June 2026).

**Refused rather than failing quietly.** The HTTP client is built without TLS and speaks only
`http/protobuf`. An `https` endpoint, or `OTEL_EXPORTER_OTLP_PROTOCOL=grpc` as the managed
agent injects, would fail on every export with nothing said. The server refuses that signal
at start-up with a message that names the fix. The chart refuses an `https` endpoint when
it renders.

## Two dependency consequences

- **`reqwest` 0.13 joins 0.12.** `opentelemetry-http` 0.32 and 0.33 both need 0.13, while
  `object_store` and Polars use 0.12, so the `semops` binary carries both. The 0.31 line
  would have avoided that, at the cost of a year-old SDK.
- **`vendor/spareval`'s hash crates are pinned together.** Adding these crates made cargo
  re-resolve `sha1` and `sha2` to 0.11 while `md-5` stayed at 0.10. `spareval` uses all
  three through one `digest` trait, so `MD5()` and the `SHA*()` functions stopped
  compiling. Its three ranges, each `>=0.10,<0.12`, are now `0.10`, recorded in its
  PROVENANCE.md.

## Tests

`holos/crates/holos-server/tests/telemetry.rs` starts the real server against a stand-in
OTLP receiver. Protobuf stores strings as their bytes, so names can be looked for without
decoding. The tests check that:

- traces and metrics arrive as `application/x-protobuf` and carry the service name
- the spans include `GET /query` and `GET /health`, with route and status
- an unmatched path is not exported as a route
- both metrics are present
- the query text appears in no export
- an `https` endpoint is refused at start-up
- a `grpc` protocol is refused at start-up, using the managed agent's own injected values

CI renders the chart with `telemetry.otlpEndpoint` set, and checks that an `https` one is
refused.

## Not done

- **OTLP over gRPC, or over TLS, from the server.** Either would let it send to the managed
  agent or to a remote collector directly. gRPC means `tonic` and an async runtime beside
  the worker threads, and TLS means choosing a crypto provider for `reqwest` 0.13, whose
  `rustls` feature brings `aws-lc-rs` and its C build.
- **Logs over OTLP.** Standard error already reaches Log Analytics from both Container Apps
  and AKS.
- **Query-level spans** (parse, plan, evaluate), and store metrics such as quad counts and
  compactions.
- **A live Application Insights workspace.** The collector example has not been run
  against one.
