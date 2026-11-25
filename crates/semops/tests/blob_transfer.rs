//! `semops upload`, `semops download` and `semops backup --to <URL>`, end to end: the real
//! binary and the real `object_store` Azure client in emulator mode, against an in-memory
//! stand-in for the Blob service that keeps what it is sent.
//!
//! The stand-in answers the five requests these commands make: Put Blob, Put Block and Put
//! Block List (a file larger than the client's buffer goes up in blocks), Get Blob with Get
//! Blob Properties, and List Blobs.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct State {
    /// Committed blobs, by `container/name`.
    blobs: BTreeMap<String, Vec<u8>>,
    /// Staged blocks, by `container/name` and block id.
    blocks: BTreeMap<(String, String), Vec<u8>>,
    /// Every request, as `METHOD target`.
    seen: Vec<String>,
}

type Shared = Arc<Mutex<State>>;

fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8(out).expect("utf8")
}

const LAST_MODIFIED: &str = "Tue, 24 Nov 2026 10:00:00 GMT";

fn respond(stream: &mut impl Write, status: &str, headers: &[(&str, String)], body: &[u8]) {
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn etag(body: &[u8]) -> String {
    format!(
        "\"0x{:016X}\"",
        body.iter()
            .fold(0xcbf29ce484222325u64, |h, &b| (h ^ u64::from(b))
                .wrapping_mul(0x100000001b3))
    )
}

fn handle(state: &Shared, mut stream: std::net::TcpStream) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.is_empty() {
        return;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default().to_owned();
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).is_err() || header == "\r\n" || header.is_empty() {
            break;
        }
        if let Some((k, v)) = header.split_once(':')
            && k.eq_ignore_ascii_case("content-length")
        {
            length = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }

    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let query: BTreeMap<String, String> = query
        .split('&')
        .filter_map(|kv| kv.split_once('=').map(|(k, v)| (decode(k), decode(v))))
        .collect();
    // /devstoreaccount1/<container>/<blob...>
    let segments: Vec<String> = path
        .trim_start_matches('/')
        .split('/')
        .skip(1)
        .map(decode)
        .collect();
    let container = segments.first().cloned().unwrap_or_default();
    let name = segments[1.min(segments.len())..].join("/");
    let key = format!("{container}/{name}");

    let mut st = state.lock().expect("state");
    st.seen.push(format!("{method} {target}"));
    let comp = query.get("comp").map(String::as_str);
    match (method.as_str(), comp) {
        ("PUT", Some("block")) => {
            let id = query.get("blockid").cloned().unwrap_or_default();
            st.blocks.insert((key, id), body);
            respond(&mut stream, "201 Created", &[], b"");
        }
        ("PUT", Some("blocklist")) => {
            let xml = String::from_utf8_lossy(&body).into_owned();
            let mut blob = Vec::new();
            for piece in xml.split('<').filter_map(|p| {
                p.strip_prefix("Uncommitted>")
                    .or_else(|| p.strip_prefix("Latest>"))
            }) {
                let id = piece.trim().to_owned();
                let block = st
                    .blocks
                    .remove(&(key.clone(), id))
                    .expect("a staged block");
                blob.extend(block);
            }
            let tag = etag(&blob);
            st.blobs.insert(key, blob);
            respond(
                &mut stream,
                "201 Created",
                &[("ETag", tag), ("Last-Modified", LAST_MODIFIED.into())],
                b"",
            );
        }
        ("PUT", _) => {
            let tag = etag(&body);
            st.blobs.insert(key, body);
            respond(
                &mut stream,
                "201 Created",
                &[("ETag", tag), ("Last-Modified", LAST_MODIFIED.into())],
                b"",
            );
        }
        ("GET", Some("list")) => {
            let prefix = format!(
                "{container}/{}",
                query.get("prefix").cloned().unwrap_or_default()
            );
            let mut xml = String::from(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<EnumerationResults><Blobs>",
            );
            for (k, v) in st
                .blobs
                .range(prefix.clone()..)
                .take_while(|(k, _)| k.starts_with(&prefix))
            {
                let blob_name = &k[container.len() + 1..];
                xml.push_str(&format!(
                    "<Blob><Name>{blob_name}</Name><Properties><Last-Modified>{LAST_MODIFIED}</Last-Modified>\
                     <Etag>{}</Etag><Content-Length>{}</Content-Length><Content-Type>application/octet-stream</Content-Type>\
                     <BlobType>BlockBlob</BlobType></Properties></Blob>",
                    etag(v).trim_matches('"'),
                    v.len()
                ));
            }
            xml.push_str("</Blobs><NextMarker /></EnumerationResults>");
            respond(
                &mut stream,
                "200 OK",
                &[("Content-Type", "application/xml".into())],
                xml.as_bytes(),
            );
        }
        ("GET" | "HEAD", _) => match st.blobs.get(&key) {
            Some(v) => {
                let headers = [
                    ("ETag", etag(v)),
                    ("Last-Modified", LAST_MODIFIED.into()),
                    ("Content-Type", "application/octet-stream".into()),
                    ("x-ms-blob-type", "BlockBlob".into()),
                ];
                if method == "HEAD" {
                    // HEAD carries the length of what GET would return, and no body.
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n",
                        v.len(),
                        headers
                            .iter()
                            .map(|(k, v)| format!("{k}: {v}\r\n"))
                            .collect::<String>()
                    );
                    let _ = stream.write_all(head.as_bytes());
                } else {
                    let body = v.clone();
                    respond(&mut stream, "200 OK", &headers, &body);
                }
            }
            None => respond(
                &mut stream,
                "404 The specified blob does not exist.",
                &[("x-ms-error-code", "BlobNotFound".into())],
                b"",
            ),
        },
        _ => respond(&mut stream, "400 Bad Request", &[], b""),
    }
}

fn service() -> (String, Shared) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    let state: Shared = Arc::default();
    let shared = Arc::clone(&state);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let state = Arc::clone(&shared);
            std::thread::spawn(move || handle(&state, stream));
        }
    });
    (url, state)
}

fn semops(service: &str, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_semops"))
        .args(args)
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

fn files_under(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).expect("read dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path
                    .strip_prefix(dir)
                    .expect("under")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, std::fs::read(&path).expect("read"));
            }
        }
    }
    out
}

/// A directory goes up file by file, a large file in blocks, and comes back byte for byte.
#[test]
fn a_directory_round_trips_through_blob_storage() {
    let (url, state) = service();
    let work = tempfile::tempdir().expect("temp dir");
    let src = work.path().join("ckpt");
    std::fs::create_dir_all(src.join("sub")).expect("mkdir");
    std::fs::write(src.join("CURRENT"), b"MANIFEST-000005\n").expect("write");
    std::fs::write(src.join("sub/notes.txt"), b"nested").expect("write");
    // Past the client's 10 MiB buffer, so it is staged as blocks and committed as a list.
    let big: Vec<u8> = (0..12 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(src.join("000012.sst"), &big).expect("write");

    let (code, _, stderr) = semops(
        &url,
        &["upload", src.to_str().expect("path"), "az://backups/"],
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(stderr.contains("uploaded 3 files"), "{stderr}");
    {
        let st = state.lock().expect("state");
        let names: Vec<&str> = st.blobs.keys().map(String::as_str).collect();
        assert_eq!(
            names,
            [
                "backups/ckpt/000012.sst",
                "backups/ckpt/CURRENT",
                "backups/ckpt/sub/notes.txt"
            ]
        );
        assert!(
            st.seen.iter().any(|r| r.contains("comp=block&")),
            "the large file went up in blocks"
        );
        assert_eq!(st.blobs["backups/ckpt/000012.sst"], big);
    }

    let back = work.path().join("restored");
    let (code, _, stderr) = semops(
        &url,
        &[
            "download",
            "az://backups/ckpt/",
            back.to_str().expect("path"),
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(files_under(&back), files_under(&src), "byte for byte");

    let (code, _, stderr) = semops(
        &url,
        &[
            "download",
            "az://backups/ckpt/",
            back.to_str().expect("path"),
        ],
    );
    assert_eq!(code, 2, "an existing destination is refused: {stderr}");
    assert!(stderr.contains("already exists"), "{stderr}");
}

/// A single file keeps its name under a prefix, and a blob URL downloads to a file.
#[test]
fn a_single_file_round_trips() {
    let (url, _) = service();
    let work = tempfile::tempdir().expect("temp dir");
    let report = work.path().join("report.sarif");
    std::fs::write(&report, b"{\"runs\":[]}").expect("write");
    let (code, _, stderr) = semops(
        &url,
        &["upload", report.to_str().expect("path"), "az://reports/ci/"],
    );
    assert_eq!(code, 0, "{stderr}");
    let back = work.path().join("again.sarif");
    let (code, _, stderr) = semops(
        &url,
        &[
            "download",
            "az://reports/ci/report.sarif",
            back.to_str().expect("path"),
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(std::fs::read(back).expect("read"), b"{\"runs\":[]}");
}

/// The proposal's backup: a checkpoint to Blob Storage, and a restore from it that a query
/// can read.
#[test]
fn a_store_is_backed_up_to_blob_storage_and_restored() {
    let (url, state) = service();
    let work = tempfile::tempdir().expect("temp dir");
    let data = work.path().join("people.ttl");
    std::fs::write(
        &data,
        "@prefix ex: <http://example.org/> .\nex:a ex:name \"Ada\" .\nex:b ex:name \"Brendan\" .\nex:c ex:name \"Cora\" .\n",
    )
    .expect("write");
    let store = work.path().join("db");
    let (code, _, stderr) = semops(
        &url,
        &[
            "load",
            "--store",
            store.to_str().expect("path"),
            "--data",
            data.to_str().expect("path"),
        ],
    );
    assert_eq!(code, 0, "{stderr}");

    let (code, _, stderr) = semops(
        &url,
        &[
            "backup",
            "--store",
            store.to_str().expect("path"),
            "--to",
            "az://backups/",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(stderr.contains("uploaded the checkpoint"), "{stderr}");
    let leftovers: Vec<_> = std::fs::read_dir(work.path())
        .expect("read dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(".db-holos-"))
        .collect();
    assert!(leftovers.is_empty(), "the local checkpoint is removed");

    let prefix = {
        let st = state.lock().expect("state");
        let current = st
            .blobs
            .keys()
            .find(|k| k.starts_with("backups/holos-") && k.ends_with("/CURRENT"))
            .expect("a checkpoint with a CURRENT file")
            .clone();
        current.trim_end_matches("CURRENT").to_owned()
    };
    assert!(
        prefix.len() == "backups/holos-20261125T093000Z/".len(),
        "{prefix}"
    );

    let restored = work.path().join("restored");
    let (code, _, stderr) = semops(
        &url,
        &[
            "download",
            &format!("az://{prefix}"),
            restored.to_str().expect("path"),
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    let (code, stdout, stderr) = semops(
        &url,
        &[
            "query",
            "--store",
            restored.to_str().expect("path"),
            "--query",
            "SELECT (COUNT(*) AS ?n) WHERE { ?s <http://example.org/name> ?o }",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains('3'),
        "the restored store has the three names:\n{stdout}"
    );
}
