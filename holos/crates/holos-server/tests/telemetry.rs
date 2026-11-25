//! OpenTelemetry from the running server: spans and metrics reach an OTLP/HTTP receiver,
//! named by the HTTP semantic conventions, and the query text does not.
//!
//! The receiver is a stand-in on localhost that accepts the exporters' protobuf POSTs and
//! keeps them. Protobuf stores strings as their bytes, so a span name or a metric name can be
//! looked for in a body without decoding it.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// One export the receiver was sent.
#[derive(Clone)]
struct Export {
    path: String,
    content_type: String,
    body: Vec<u8>,
}

fn receiver() -> (String, Arc<Mutex<Vec<Export>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    let exports = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&exports);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let log = Arc::clone(&log);
            std::thread::spawn(move || serve(stream, &log));
        }
    });
    (url, exports)
}

fn serve(mut stream: TcpStream, log: &Mutex<Vec<Export>>) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let path = line
            .split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .to_owned();
        let (mut length, mut content_type) = (0usize, String::new());
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header).unwrap_or(0) == 0 || header == "\r\n" {
                break;
            }
            if let Some((k, v)) = header.split_once(':') {
                if k.eq_ignore_ascii_case("content-length") {
                    length = v.trim().parse().unwrap_or(0);
                } else if k.eq_ignore_ascii_case("content-type") {
                    content_type = v.trim().to_owned();
                }
            }
        }
        let mut body = vec![0u8; length];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        log.lock().expect("log").push(Export {
            path,
            content_type,
            body,
        });
        // An empty Export*ServiceResponse is zero bytes of protobuf.
        let _ = stream.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 0\r\n\r\n",
        );
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

/// The server, killed when the test is done with it.
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start(port: u16, env: &[(&str, &str)]) -> (Server, Arc<Mutex<String>>) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_holos-server"));
    command
        .args(["--listen", &format!("127.0.0.1:{port}"), "--no-ui"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for name in [
        "OTEL_EXPORTER_OTLP_ENDPOINT",
        "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
        "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
        "OTEL_SDK_DISABLED",
        "OTEL_SERVICE_NAME",
        "OTEL_EXPORTER_OTLP_PROTOCOL",
        "OTEL_EXPORTER_OTLP_TRACES_PROTOCOL",
        "OTEL_EXPORTER_OTLP_METRICS_PROTOCOL",
    ] {
        command.env_remove(name);
    }
    command.envs(env.iter().copied());
    let mut child = command.spawn().expect("holos-server starts");
    let stderr = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&stderr);
    let pipe = child.stderr.take().expect("stderr");
    std::thread::spawn(move || {
        for line in BufReader::new(pipe).lines().map_while(Result::ok) {
            let mut s = sink.lock().expect("stderr");
            s.push_str(&line);
            s.push('\n');
        }
    });
    let server = Server(child);
    let deadline = Instant::now() + Duration::from_secs(30);
    while get(port, "/health").is_none() {
        assert!(
            Instant::now() < deadline,
            "the server did not start: {}",
            stderr.lock().unwrap()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    (server, stderr)
}

/// A GET, returning the status line, or `None` when nothing answers.
fn get(port: u16, target: &str) -> Option<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    write!(
        stream,
        "GET {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    response.lines().next().map(str::to_owned)
}

/// Waits for a line on the server's standard error, which a reader thread collects.
fn wait_for(stderr: &Mutex<String>, needle: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let text = stderr.lock().expect("stderr").clone();
        if text.contains(needle) || Instant::now() > deadline {
            return text;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|w| w == needle.as_bytes())
}

#[test]
fn spans_and_metrics_reach_the_collector_without_the_query() {
    let (endpoint, exports) = receiver();
    let port = free_port();
    let (_server, stderr) = start(
        port,
        &[
            ("OTEL_EXPORTER_OTLP_ENDPOINT", endpoint.as_str()),
            ("OTEL_SERVICE_NAME", "holos-under-test"),
            ("OTEL_METRIC_EXPORT_INTERVAL", "300"),
            ("OTEL_BSP_SCHEDULE_DELAY", "100"),
        ],
    );
    let banner = wait_for(&stderr, "telemetry traces and metrics over OTLP/HTTP");
    assert!(
        banner.contains("telemetry traces and metrics over OTLP/HTTP"),
        "{banner}"
    );

    // A query whose text names a value nobody should find in the telemetry.
    let status = get(
        port,
        "/query?query=ASK%7B%3Cx%3Asecret-subject%3E%3Fp%3Fo%7D",
    )
    .expect("answer");
    assert!(status.contains("200"), "{status}");
    assert!(get(port, "/nowhere").expect("answer").contains("404"));

    let deadline = Instant::now() + Duration::from_secs(20);
    let (traces, metrics) = loop {
        let seen = exports.lock().expect("log").clone();
        let traces: Vec<_> = seen
            .iter()
            .filter(|e| e.path == "/v1/traces")
            .cloned()
            .collect();
        let metrics: Vec<_> = seen
            .iter()
            .filter(|e| e.path == "/v1/metrics")
            .cloned()
            .collect();
        let all_spans = traces.iter().any(|e| contains(&e.body, "GET /query"));
        let measured = metrics
            .iter()
            .any(|e| contains(&e.body, "http.server.request.duration"));
        if all_spans && measured {
            break (traces, metrics);
        }
        assert!(
            Instant::now() < deadline,
            "no export arrived: {} so far",
            seen.len()
        );
        std::thread::sleep(Duration::from_millis(100));
    };

    for e in traces.iter().chain(&metrics) {
        assert_eq!(e.content_type, "application/x-protobuf", "{}", e.path);
        assert!(
            contains(&e.body, "holos-under-test"),
            "the service name, in {}",
            e.path
        );
        assert!(
            !contains(&e.body, "secret-subject"),
            "the query text left the server in {}",
            e.path
        );
    }
    let spans: Vec<u8> = traces.iter().flat_map(|e| e.body.clone()).collect();
    for expected in [
        "GET /query",
        "GET /health",
        "http.route",
        "http.response.status_code",
    ] {
        assert!(contains(&spans, expected), "{expected} in the spans");
    }
    // A path no route serves has no route, so its span is named by its method alone.
    assert!(
        !contains(&spans, "/nowhere"),
        "an unmatched path is not a route"
    );
    let measured: Vec<u8> = metrics.iter().flat_map(|e| e.body.clone()).collect();
    assert!(contains(&measured, "http.server.active_requests"));
}

#[test]
fn an_https_endpoint_is_refused_at_start_up() {
    let port = free_port();
    let (_server, stderr) = start(
        port,
        &[(
            "OTEL_EXPORTER_OTLP_ENDPOINT",
            "https://collector.example:4318",
        )],
    );
    let text = wait_for(&stderr, "metrics off");
    assert!(
        text.contains("traces off: https://collector.example:4318 is https"),
        "{text}"
    );
    assert!(text.contains("metrics off"), "{text}");
    assert!(!text.contains("telemetry traces"), "{text}");
}

/// What the Container Apps managed agent injects: a gRPC endpoint. Refused, with the fix.
#[test]
fn a_grpc_protocol_is_refused_at_start_up() {
    let port = free_port();
    let (_server, stderr) = start(
        port,
        &[
            (
                "OTEL_EXPORTER_OTLP_ENDPOINT",
                "http://otel.service.k8se-apps:4317/",
            ),
            ("OTEL_EXPORTER_OTLP_PROTOCOL", "grpc"),
        ],
    );
    let text = wait_for(&stderr, "metrics off");
    assert!(
        text.contains("traces off: the OTLP protocol is grpc"),
        "{text}"
    );
    assert!(
        text.contains("metrics off: the OTLP protocol is grpc"),
        "{text}"
    );
    assert!(!text.contains("telemetry traces"), "{text}");
}
