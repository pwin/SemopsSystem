//! Azure Storage URLs as inputs, end to end: the real `semops` binary, the real
//! `object_store` Azure client, and a stand-in for the Blob service on localhost.
//!
//! The client runs in emulator mode (`AZURE_STORAGE_USE_EMULATOR`), which is how it talks
//! to Azurite: account `devstoreaccount1`, signed with the emulator's well-known key, over
//! plain HTTP to `AZURITE_BLOB_STORAGE_URL`. The stand-in answers a Get Blob request with
//! the headers the client requires and records what it was asked for.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::{Arc, Mutex};

const PEOPLE: &str = "@prefix ex: <http://example.org/> .\n\
                      ex:a ex:name \"Ada\" .\n\
                      ex:b ex:name \"Brendan\" .\n\
                      ex:c ex:name \"Cora\" .\n";

/// A request the stand-in received: the request line and whether it was signed.
#[derive(Debug, Clone)]
struct Seen {
    line: String,
    signed: bool,
}

/// Serves `body` for every GET on a fresh port, recording each request.
fn blob_service(body: &'static str) -> (String, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            let mut signed = false;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).is_err() || header == "\r\n" || header.is_empty() {
                    break;
                }
                if header
                    .to_ascii_lowercase()
                    .starts_with("authorization: sharedkey ")
                {
                    signed = true;
                }
            }
            log.lock().expect("log").push(Seen {
                line: line.trim_end().to_owned(),
                signed,
            });
            let response = format!(
                "HTTP/1.1 200 OK\r\n\
                 Content-Length: {}\r\n\
                 Content-Type: text/turtle\r\n\
                 Last-Modified: Tue, 24 Nov 2026 10:00:00 GMT\r\n\
                 ETag: \"0x8DD0000000000\"\r\n\
                 x-ms-blob-type: BlockBlob\r\n\
                 Connection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (url, seen)
}

fn semops(service: &str, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_semops"))
        .args(args)
        // The emulator, and nothing from the machine running the test.
        .env("AZURE_STORAGE_USE_EMULATOR", "true")
        .env("AZURITE_BLOB_STORAGE_URL", service)
        .env_remove("AZURE_STORAGE_ACCOUNT_NAME")
        .env_remove("AZURE_STORAGE_ACCOUNT_KEY")
        .env_remove("AZURE_STORAGE_SAS_KEY")
        .output()
        .expect("semops runs");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn a_query_reads_its_data_from_blob_storage() {
    let (service, seen) = blob_service(PEOPLE);
    let (code, stdout, stderr) = semops(
        &service,
        &[
            "query",
            "--data",
            "az://data/people/people.ttl",
            "--query",
            "SELECT (COUNT(*) AS ?n) WHERE { ?s <http://example.org/name> ?o }",
        ],
    );
    assert_eq!(code, 0, "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(
        stdout.contains('3'),
        "three names, from the blob:\n{stdout}"
    );

    let seen = seen.lock().expect("log").clone();
    assert_eq!(seen.len(), 1, "one download: {seen:?}");
    assert!(
        seen[0]
            .line
            .starts_with("GET /devstoreaccount1/data/people/people.ttl "),
        "{seen:?}"
    );
    assert!(seen[0].signed, "signed with the account key: {seen:?}");
}

/// The format comes from the blob's name, so a `.nt` blob parses as N-Triples.
#[test]
fn the_blob_name_decides_the_format() {
    let (service, _) =
        blob_service("<http://example.org/a> <http://example.org/p> <http://example.org/b> .\n");
    let (code, stdout, stderr) = semops(
        &service,
        &[
            "query",
            // holos's own parser takes `--data FILE`, not `--data=FILE`; the `=` form is
            // rewritten too (see the unit tests) for the commands whose parsers take it.
            "--data",
            "az://data/triples.nt",
            "--query",
            "ASK { <http://example.org/a> <http://example.org/p> <http://example.org/b> }",
        ],
    );
    assert_eq!(code, 0, "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(stdout.contains("true"), "{stdout}");
}

/// A plain IRI is not a blob: it reaches the command as typed, and nothing is fetched.
#[test]
fn an_iri_argument_is_not_fetched() {
    let (service, seen) = blob_service(PEOPLE);
    let dir = tempfile::tempdir().expect("temp dir");
    let data = dir.path().join("people.ttl");
    std::fs::write(&data, PEOPLE).expect("write");
    let (code, stdout, stderr) = semops(
        &service,
        &[
            "query",
            "--data",
            data.to_str().expect("path"),
            "--base",
            "https://example.org/",
            "--query",
            "ASK { ?s ?p ?o }",
        ],
    );
    assert_eq!(code, 0, "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(seen.lock().expect("log").is_empty());
}

#[test]
fn repair_will_not_write_back_to_a_url() {
    let (service, _) = blob_service(PEOPLE);
    let (code, _, stderr) = semops(&service, &["repair", "az://data/people.ttl", "--write"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(
        stderr.contains("cannot write back to az://data/people.ttl"),
        "{stderr}"
    );
}

/// `semops check` as a CI gate over an ontology kept in storage: the same findings as the
/// local file gives (see tests/check.rs).
#[test]
fn check_runs_on_a_blob() {
    let domain = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../semops-checks/tests/fixtures/ontology/domain.ttl");
    let text: &'static str = Box::leak(
        std::fs::read_to_string(domain)
            .expect("fixture")
            .into_boxed_str(),
    );
    let (service, seen) = blob_service(text);
    let url = "https://devstoreaccount1.blob.core.windows.net/onto/domain.ttl";
    let (code, stdout, stderr) = semops(&service, &["check", url]);
    assert_eq!(code, 1, "domain.ttl has violations\n{stdout}\n{stderr}");
    assert!(stdout.contains("64 findings:"), "{stdout}");
    let seen = seen.lock().expect("log").clone();
    assert!(
        seen[0]
            .line
            .starts_with("GET /devstoreaccount1/onto/domain.ttl "),
        "{seen:?}"
    );
}
