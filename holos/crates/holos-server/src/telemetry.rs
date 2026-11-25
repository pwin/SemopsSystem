//! OpenTelemetry: one span and one duration per request, exported over OTLP/HTTP.
//!
//! Off unless an OTLP endpoint is configured, through the standard variables:
//! `OTEL_EXPORTER_OTLP_ENDPOINT`, or per signal `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` and
//! `OTEL_EXPORTER_OTLP_METRICS_ENDPOINT`, with `OTEL_EXPORTER_OTLP_HEADERS` for a collector
//! that wants a key. `OTEL_SERVICE_NAME` names the service (`holos` by default),
//! `OTEL_RESOURCE_ATTRIBUTES` adds to it, `OTEL_METRIC_EXPORT_INTERVAL` and
//! `OTEL_BSP_SCHEDULE_DELAY` set the export cadence, and `OTEL_SDK_DISABLED=true` or
//! `OTEL_TRACES_EXPORTER`/`OTEL_METRICS_EXPORTER=none` turn it, or one signal, off.
//!
//! On Azure an OpenTelemetry Collector takes it from there: its HTTP receiver gets the
//! export, and its `azuremonitor` exporter sends traces and metrics to Application Insights.
//! Nothing here is Azure-specific. (The Container Apps managed agent accepts only gRPC, and
//! its Application Insights destination takes no metrics, so on Container Apps the collector
//! runs as a sidecar.)
//!
//! What is recorded follows the semantic conventions for HTTP servers:
//!
//! * a span per request, kind server, named `METHOD route`, with `http.request.method`,
//!   `http.route` and `http.response.status_code`;
//! * the histogram `http.server.request.duration`, in seconds, with the same attributes;
//! * the up-down counter `http.server.active_requests`.
//!
//! What is not: the query string, the request body, any header, the principal. A SPARQL
//! query can carry the very values it asks about, and a telemetry backend is outside the
//! access policy that decides who may see them. The route is one of a fixed set, so the
//! metrics' cardinality is bounded whatever clients send.
//!
//! The exporters run on their own threads with a blocking HTTP client, so the server stays
//! free of an async runtime. That client has no TLS and speaks only http/protobuf, so the
//! endpoint is a collector's plain-HTTP receiver, which forwards over TLS. An `https`
//! endpoint, or `OTEL_EXPORTER_OTLP_PROTOCOL=grpc`, is refused at start-up rather than
//! failing on every export.

use std::borrow::Cow;
use std::cell::Cell;

thread_local! {
    /// The status of the response this worker thread sent last.
    static STATUS: Cell<u16> = const { Cell::new(0) };
}

/// Called where every response is sent, so the request's telemetry knows its status.
pub(crate) fn record_status(status: u16) {
    STATUS.with(|s| s.set(status));
}

fn take_status() -> u16 {
    STATUS.with(|s| s.replace(0))
}

/// The server's telemetry: exporting, or nothing at all.
pub(crate) struct Telemetry {
    #[cfg(feature = "otel")]
    exporting: Option<otel::Exporting>,
}

/// A request being answered.
pub(crate) struct InFlight {
    #[cfg(feature = "otel")]
    inner: otel::Request,
}

impl Telemetry {
    /// Telemetry as the environment configures it; off when it names no endpoint.
    pub(crate) fn from_env() -> Self {
        Self {
            #[cfg(feature = "otel")]
            exporting: otel::Exporting::from_env(),
        }
    }

    /// One line for the start-up banner, when exporting.
    pub(crate) fn describe(&self) -> Option<String> {
        #[cfg(feature = "otel")]
        if let Some(e) = &self.exporting {
            return Some(e.describe());
        }
        None
    }

    /// Starts a request's span and counts it as active.
    #[allow(unused_variables)]
    pub(crate) fn start(&self, method: &str, route: Option<Cow<'static, str>>) -> Option<InFlight> {
        // Whatever an earlier request on this thread left, it is not this one's status.
        take_status();
        #[cfg(feature = "otel")]
        if let Some(e) = &self.exporting {
            return Some(InFlight {
                inner: e.start(method, route),
            });
        }
        None
    }

    /// Ends a request: its duration and status recorded, its span closed. A request that
    /// sent no response, because it failed or panicked first, is recorded without a status
    /// and as an error.
    #[allow(unused_variables)]
    pub(crate) fn finish(&self, in_flight: Option<InFlight>) {
        let status = take_status();
        #[cfg(feature = "otel")]
        if let (Some(e), Some(f)) = (&self.exporting, in_flight) {
            e.finish(f.inner, status);
        }
    }
}

#[cfg(feature = "otel")]
mod otel {
    use std::borrow::Cow;
    use std::time::Instant;

    use opentelemetry::metrics::{Histogram, MeterProvider as _, UpDownCounter};
    use opentelemetry::trace::{Span as _, SpanKind, Status, Tracer as _, TracerProvider as _};
    use opentelemetry::KeyValue;
    use opentelemetry_sdk::metrics::SdkMeterProvider;
    use opentelemetry_sdk::trace::{SdkTracer, SdkTracerProvider, Span};
    use opentelemetry_sdk::Resource;

    /// The semantic conventions' boundaries, then longer ones: a SPARQL query may run for
    /// minutes, up to the server's `--timeout`.
    const BOUNDARIES: [f64; 18] = [
        0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0, 30.0,
        60.0, 120.0, 300.0,
    ];

    pub(super) struct Exporting {
        traces: Option<(SdkTracerProvider, SdkTracer)>,
        metrics: Option<Metrics>,
        endpoint: String,
    }

    struct Metrics {
        // Kept so its reader thread lives as long as the server.
        _provider: SdkMeterProvider,
        duration: Histogram<f64>,
        active: UpDownCounter<i64>,
    }

    pub(super) struct Request {
        start: Instant,
        method: String,
        route: Option<Cow<'static, str>>,
        span: Option<Span>,
    }

    fn set(name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|v| !v.trim().is_empty())
    }

    fn off(name: &str) -> bool {
        set(name).is_some_and(|v| v.trim().eq_ignore_ascii_case("none"))
    }

    impl Exporting {
        pub(super) fn from_env() -> Option<Self> {
            if set("OTEL_SDK_DISABLED").is_some_and(|v| v.trim().eq_ignore_ascii_case("true")) {
                return None;
            }
            let common = set("OTEL_EXPORTER_OTLP_ENDPOINT");
            let traces_to = set("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT").or(common.clone());
            let metrics_to = set("OTEL_EXPORTER_OTLP_METRICS_ENDPOINT").or(common.clone());
            // The exporters speak OTLP as protobuf over plain HTTP. Anything else would fail on
            // every export, quietly, so it is refused here, where someone will read it: an
            // `https` endpoint, or a protocol of `grpc`, which is what the Container Apps
            // managed agent injects.
            let usable = |signal: &str, to: &Option<String>, protocol_var: &str| -> bool {
                let Some(url) = to else { return false };
                let protocol = set(protocol_var).or_else(|| set("OTEL_EXPORTER_OTLP_PROTOCOL"));
                if let Some(p) = protocol.filter(|p| p.trim() != "http/protobuf") {
                    eprintln!(
                        "telemetry: {} off: the OTLP protocol is {}, and this server sends \
                         http/protobuf; point OTEL_EXPORTER_OTLP_ENDPOINT at an OpenTelemetry \
                         Collector's HTTP receiver (port 4318) and set the protocol to \
                         http/protobuf or leave it unset",
                        signal,
                        p.trim()
                    );
                    return false;
                }
                if url.trim().to_ascii_lowercase().starts_with("https:") {
                    eprintln!(
                        "telemetry: {} off: {} is https, and OTLP export here is plain \
                         HTTP; send it to an OpenTelemetry Collector's HTTP receiver, which \
                         forwards over TLS",
                        signal, url
                    );
                    return false;
                }
                true
            };
            let traces_on = !off("OTEL_TRACES_EXPORTER")
                && usable("traces", &traces_to, "OTEL_EXPORTER_OTLP_TRACES_PROTOCOL");
            let metrics_on = !off("OTEL_METRICS_EXPORTER")
                && usable(
                    "metrics",
                    &metrics_to,
                    "OTEL_EXPORTER_OTLP_METRICS_PROTOCOL",
                );
            if !traces_on && !metrics_on {
                return None;
            }

            // The environment's resource (OTEL_SERVICE_NAME, OTEL_RESOURCE_ATTRIBUTES), with
            // `holos` for a service nobody named.
            let mut resource = Resource::builder();
            if set("OTEL_SERVICE_NAME").is_none() {
                resource = resource.with_service_name("holos");
            }
            let resource = resource.build();

            let traces = if traces_on {
                match opentelemetry_otlp::SpanExporter::builder()
                    .with_http()
                    .build()
                {
                    Ok(exporter) => {
                        let provider = SdkTracerProvider::builder()
                            .with_batch_exporter(exporter)
                            .with_resource(resource.clone())
                            .build();
                        let tracer = provider.tracer("holos-server");
                        Some((provider, tracer))
                    }
                    Err(e) => {
                        eprintln!("telemetry: traces off, the OTLP exporter failed: {e}");
                        None
                    }
                }
            } else {
                None
            };
            let metrics = if metrics_on {
                match opentelemetry_otlp::MetricExporter::builder()
                    .with_http()
                    .build()
                {
                    Ok(exporter) => {
                        let provider = SdkMeterProvider::builder()
                            .with_periodic_exporter(exporter)
                            .with_resource(resource)
                            .build();
                        let meter = provider.meter("holos-server");
                        let duration = meter
                            .f64_histogram("http.server.request.duration")
                            .with_unit("s")
                            .with_description("Duration of HTTP server requests.")
                            .with_boundaries(BOUNDARIES.to_vec())
                            .build();
                        let active = meter
                            .i64_up_down_counter("http.server.active_requests")
                            .with_unit("{request}")
                            .with_description("Number of active HTTP server requests.")
                            .build();
                        Some(Metrics {
                            _provider: provider,
                            duration,
                            active,
                        })
                    }
                    Err(e) => {
                        eprintln!("telemetry: metrics off, the OTLP exporter failed: {e}");
                        None
                    }
                }
            } else {
                None
            };
            if traces.is_none() && metrics.is_none() {
                return None;
            }
            let endpoint = traces_to
                .filter(|_| traces.is_some())
                .or(metrics_to)
                .unwrap_or_default();
            Some(Self {
                traces,
                metrics,
                endpoint,
            })
        }

        pub(super) fn describe(&self) -> String {
            let signals = match (self.traces.is_some(), self.metrics.is_some()) {
                (true, true) => "traces and metrics",
                (true, false) => "traces",
                _ => "metrics",
            };
            format!("{signals} over OTLP/HTTP to {}", self.endpoint)
        }

        pub(super) fn start(&self, method: &str, route: Option<Cow<'static, str>>) -> Request {
            let method_attr = KeyValue::new("http.request.method", method.to_owned());
            if let Some(m) = &self.metrics {
                m.active.add(1, std::slice::from_ref(&method_attr));
            }
            let span = self.traces.as_ref().map(|(_, tracer)| {
                let name = match &route {
                    Some(r) => format!("{method} {r}"),
                    None => method.to_owned(),
                };
                let mut attributes = vec![method_attr];
                if let Some(r) = &route {
                    attributes.push(KeyValue::new("http.route", r.to_string()));
                }
                tracer
                    .span_builder(name)
                    .with_kind(SpanKind::Server)
                    .with_attributes(attributes)
                    .start(tracer)
            });
            Request {
                start: Instant::now(),
                method: method.to_owned(),
                route,
                span,
            }
        }

        pub(super) fn finish(&self, request: Request, status: u16) {
            let seconds = request.start.elapsed().as_secs_f64();
            let mut attributes = vec![KeyValue::new("http.request.method", request.method)];
            if let Some(r) = request.route {
                attributes.push(KeyValue::new("http.route", r.into_owned()));
            }
            if status != 0 {
                attributes.push(KeyValue::new(
                    "http.response.status_code",
                    i64::from(status),
                ));
            }
            if let Some(m) = &self.metrics {
                m.duration.record(seconds, &attributes);
                m.active.add(-1, &attributes[..1]);
            }
            if let Some(mut span) = request.span {
                if status != 0 {
                    span.set_attribute(KeyValue::new(
                        "http.response.status_code",
                        i64::from(status),
                    ));
                }
                // The conventions: a 5xx is a server span's error, a 4xx is the client's.
                if status == 0 || status >= 500 {
                    span.set_status(Status::error(if status == 0 {
                        "no response was sent"
                    } else {
                        "server error"
                    }));
                }
                span.end();
            }
        }
    }
}
