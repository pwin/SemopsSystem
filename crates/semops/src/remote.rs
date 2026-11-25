//! Azure Storage URLs as command inputs.
//!
//! `semops load --data abfss://data@acct.dfs.core.windows.net/dump.nt.gz` reads the blob
//! straight from storage: before a subcommand runs, every argument naming a blob is fetched
//! to a temporary file and the argument becomes that file's path. The engines' commands
//! never see a URL, so `load`, `query`, `validate`, `map`, `check` and the rest all take
//! one, with their argument parsing unchanged.
//!
//! Only unambiguous storage URLs are fetched: the `abfss`, `abfs`, `az`, `azure` and `adl`
//! schemes, and `https` on `*.blob.core.windows.net` or `*.dfs.core.windows.net`. A plain
//! `http(s)` IRI is left alone, because a named graph or a base IRI is written exactly like
//! one, and fetching it would turn an identifier into a file.
//!
//! Credentials come from the environment as `object_store` reads them: an account key
//! (`AZURE_STORAGE_ACCOUNT_KEY`), a SAS token (`AZURE_STORAGE_SAS_KEY`), a service principal
//! (`AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID`), workload identity
//! (`AZURE_FEDERATED_TOKEN_FILE`), the Azure CLI (`AZURE_USE_AZURE_CLI=true`), or with none
//! of those a managed identity. A public container needs `AZURE_SKIP_SIGNATURE=true`. A SAS
//! in the URL's query string (`…?sv=…&sig=…`) is used as given.
//!
//! The file keeps the blob's name, so the format is still taken from its extension
//! (`.ttl.gz`, `.parquet`). It lives in a directory under the system temporary directory
//! (`TMPDIR`, or `TMP` on Windows), removed when the command finishes.
//!
//! Writing goes the other way and is explicit: [`upload`] and [`download`] copy a file or a
//! directory tree, such as a store checkpoint, to and from a blob prefix, for `semops
//! upload`, `semops download` and `semops backup --to <URL>`. The value of an output flag
//! (`--to`, `--out`, `--output`, `-o`) is never fetched, and a blob URL there is refused
//! rather than written to a local file named like it.

use anyhow::{Context, Result, bail};
use object_store::azure::{MicrosoftAzure, MicrosoftAzureBuilder};
use object_store::{ObjectStore, ObjectStoreExt};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The schemes that only ever name storage.
const SCHEMES: [&str; 5] = ["abfss", "abfs", "az", "azure", "adl"];

/// The hosts on which an `https` URL names a blob.
const HOSTS: [&str; 2] = [".blob.core.windows.net", ".dfs.core.windows.net"];

/// Flags whose value a command writes rather than reads.
const OUTPUT_FLAGS: [&str; 4] = ["--to", "--out", "--output", "-o"];

/// Whether `arg` is a URL this module fetches.
pub fn is_blob_url(arg: &str) -> bool {
    let Some((scheme, rest)) = arg.split_once("://") else {
        return false;
    };
    let scheme = scheme.to_ascii_lowercase();
    if SCHEMES.contains(&scheme.as_str()) {
        return !rest.is_empty();
    }
    if scheme != "https" {
        return false;
    }
    let host = rest.split(['/', '?']).next().unwrap_or_default();
    let host = host.to_ascii_lowercase();
    // A container is named in the path, so a bare account URL is not a blob.
    let has_path = rest[host.len()..].trim_start_matches('/').split('?').next() != Some("");
    HOSTS
        .iter()
        .any(|h| host.ends_with(h) && host.len() > h.len())
        && has_path
}

/// A command line with its blob URLs replaced by local files.
pub struct Localised {
    /// The arguments, with each blob URL replaced by the path it was fetched to.
    pub args: Vec<OsString>,
    /// The fetched files and the URL each came from.
    pub fetched: Vec<(PathBuf, String)>,
    /// Holds the files; dropped, it removes them.
    _dir: Option<tempfile::TempDir>,
}

impl Localised {
    /// The URL `path` was fetched from, if it was.
    pub fn source_of(&self, path: &Path) -> Option<&str> {
        self.fetched
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, url)| url.as_str())
    }
}

/// Fetches every blob URL in `args`, either a whole argument or the value of `--flag=URL`.
///
/// `args[0]`, the program name, is never rewritten. With no blob URL the arguments come back
/// unchanged and nothing is created.
pub fn localise(args: Vec<OsString>) -> Result<Localised> {
    localise_with(args, &mut |url: &str, to: &Path| fetch(url, to))
}

/// [`localise`], with the fetch supplied, so the rewriting can be tested without a network.
pub fn localise_with(
    args: Vec<OsString>,
    fetch: &mut dyn FnMut(&str, &Path) -> Result<u64>,
) -> Result<Localised> {
    let mut dir: Option<tempfile::TempDir> = None;
    let mut fetched: Vec<(PathBuf, String)> = Vec::new();
    let mut out = Vec::with_capacity(args.len());
    let mut after_output: Option<String> = None;

    for (i, arg) in args.into_iter().enumerate() {
        let output_flag = after_output.take();
        let Some(text) = arg.to_str().filter(|_| i > 0) else {
            out.push(arg);
            continue;
        };
        if OUTPUT_FLAGS.contains(&text) {
            after_output = Some(text.to_owned());
        }
        // What a command writes is never fetched; a blob URL there would become a local file
        // named like it, so it is refused.
        let written = match (output_flag, text.split_once('=')) {
            (Some(flag), _) => Some((flag, text)),
            (None, Some((flag, value))) if OUTPUT_FLAGS.contains(&flag) => {
                Some((flag.to_owned(), value))
            }
            _ => None,
        };
        if let Some((flag, value)) = written {
            if is_blob_url(value) {
                bail!(
                    "{flag} {}: a command cannot write to Blob Storage; write a local file and \
                     `semops upload` it (`semops backup --to <URL>` does both)",
                    redact(value)
                );
            }
            out.push(arg);
            continue;
        }
        let (prefix, url) = match text.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") && is_blob_url(value) => {
                (format!("{flag}="), value)
            }
            _ if is_blob_url(text) => (String::new(), text),
            _ => {
                out.push(arg);
                continue;
            }
        };
        let path = match fetched.iter().find(|(_, u)| u == url) {
            // The same URL twice is one download.
            Some((path, _)) => path.clone(),
            None => {
                if dir.is_none() {
                    dir = Some(
                        tempfile::Builder::new()
                            .prefix("semops-fetch-")
                            .tempdir()
                            .context("creating a directory for fetched blobs")?,
                    );
                }
                let base = dir
                    .as_ref()
                    .map(tempfile::TempDir::path)
                    .unwrap_or(Path::new("."));
                // One subdirectory per URL: two blobs may share a name.
                let slot = base.join(fetched.len().to_string());
                std::fs::create_dir(&slot)
                    .with_context(|| format!("creating {}", slot.display()))?;
                let path = slot.join(file_name(url));
                fetch(url, &path).with_context(|| format!("fetching {}", redact(url)))?;
                fetched.push((path.clone(), url.to_owned()));
                path
            }
        };
        let mut rewritten = OsString::from(prefix);
        rewritten.push(path.as_os_str());
        out.push(rewritten);
    }
    Ok(Localised {
        args: out,
        fetched,
        _dir: dir,
    })
}

/// The blob's own name, the last path segment without the query, so its extension survives.
fn file_name(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let name = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    // A name that is not a usable file name falls back to a neutral one.
    if name.is_empty() || name.contains("://") || name.contains(['\\', ':']) {
        "blob".to_owned()
    } else {
        name.to_owned()
    }
}

/// `url` without its query string, which may carry a SAS signature: fit for an error message.
pub fn redact(url: &str) -> &str {
    url.split('?').next().unwrap_or(url)
}

/// Downloads one blob to `to`, streaming, and returns its size.
fn fetch(url: &str, to: &Path) -> Result<u64> {
    let (store, path) = open(url)?;
    runtime()?.block_on(async {
        use futures::StreamExt;
        let mut stream = store.get(&path).await?.into_stream();
        let mut file = std::io::BufWriter::new(
            std::fs::File::create(to).with_context(|| format!("creating {}", to.display()))?,
        );
        let mut written = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk)?;
            written += chunk.len() as u64;
        }
        file.flush()?;
        Ok::<u64, anyhow::Error>(written)
    })
}

/// The store and the object path a URL names, with credentials from the environment.
fn open(url: &str) -> Result<(MicrosoftAzure, object_store::path::Path)> {
    let (address, sas) = match url.split_once('?') {
        Some((address, query)) if query.split('&').any(|kv| kv.starts_with("sig=")) => {
            (address, Some(query))
        }
        Some((address, _)) => (address, None),
        None => (url, None),
    };
    let parsed = url::Url::parse(address).context("not a URL")?;
    let (scheme, path) = object_store::ObjectStoreScheme::parse(&parsed)
        .map_err(|e| anyhow::anyhow!("not a storage URL: {e}"))?;
    if scheme != object_store::ObjectStoreScheme::MicrosoftAzure {
        bail!("only Azure Storage URLs are supported");
    }
    let mut builder = MicrosoftAzureBuilder::from_env().with_url(address);
    if let Some(sas) = sas {
        builder = builder.with_config(object_store::azure::AzureConfigKey::SasKey, sas.to_owned());
    }
    Ok((builder.build()?, path))
}

/// A blob URL ending in `/` names a prefix, as a path ending in `/` names a directory.
fn is_prefix(url: &str) -> bool {
    redact(url).ends_with('/')
}

/// `url` with `name` appended as one more path segment, the query kept.
fn join(url: &str, name: &str) -> String {
    let (address, query) = match url.split_once('?') {
        Some((a, q)) => (a, Some(q)),
        None => (url, None),
    };
    let mut joined = format!("{}/{name}", address.trim_end_matches('/'));
    if let Some(q) = query {
        joined.push('?');
        joined.push_str(q);
    }
    joined
}

/// Where `semops upload FROM URL` puts `from`, as `cp -r` would: under a URL ending in `/` it
/// keeps its own name, otherwise the URL names it. A directory's files go under the result.
pub fn upload_target(from: &Path, url: &str) -> String {
    if is_prefix(url) {
        let name = from
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "upload".to_owned());
        join(url, &name)
    } else {
        url.to_owned()
    }
}

/// What a transfer moved.
#[derive(Debug, Default)]
pub struct Copied {
    pub files: usize,
    pub bytes: u64,
}

/// Uploads a file, or every file under a directory, to the blob or prefix `url` names, as
/// [`upload_target`] decides. Files stream from disk; a large one goes up in blocks.
pub fn upload(from: &Path, url: &str) -> Result<Copied> {
    upload_to(from, &upload_target(from, url))
}

/// [`upload`], to exactly `target`: the blob a file becomes, or the prefix a directory's
/// files go under.
pub fn upload_to(from: &Path, target: &str) -> Result<Copied> {
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    if from.is_dir() {
        walk(from, from, &mut files)?;
        files.sort();
    } else if from.is_file() {
        files.push((from.to_owned(), String::new()));
    } else {
        bail!("{} is neither a file nor a directory", from.display());
    }
    let runtime = runtime()?;
    let mut copied = Copied::default();
    for (path, relative) in files {
        let url = if relative.is_empty() {
            target.to_owned()
        } else {
            join(target, &relative)
        };
        let (store, blob) = open(&url)?;
        let bytes = runtime
            .block_on(put_file(store, blob, &path))
            .with_context(|| format!("uploading {} to {}", path.display(), redact(&url)))?;
        copied.files += 1;
        copied.bytes += bytes;
    }
    Ok(copied)
}

/// Every file under `dir`, with its path relative to `root` written with `/`.
fn walk(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, String)>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            walk(root, &path, out)?;
        } else {
            let relative = path
                .strip_prefix(root)
                .expect("under the root it was found in")
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.push((path, relative));
        }
    }
    Ok(())
}

async fn put_file(
    store: MicrosoftAzure,
    blob: object_store::path::Path,
    from: &Path,
) -> Result<u64> {
    use std::io::Read;
    use tokio::io::AsyncWriteExt;
    let store: std::sync::Arc<dyn object_store::ObjectStore> = std::sync::Arc::new(store);
    // Up to its capacity a BufWriter makes one Put Blob; past it, blocks and a block list.
    let mut writer = object_store::buffered::BufWriter::new(store, blob);
    let mut file =
        std::fs::File::open(from).with_context(|| format!("opening {}", from.display()))?;
    let mut buffer = vec![0u8; 1 << 20];
    let mut sent = 0u64;
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buffer[..n]).await?;
        sent += n as u64;
    }
    writer.shutdown().await?;
    Ok(sent)
}

/// Downloads the blob `url` names to the file `to`, or every blob under a prefix into the
/// directory `to`, keeping their paths. A URL ending in `/` is a prefix; one that names no
/// blob is tried as one. `to` must not exist, so a restore never mixes two checkpoints.
pub fn download(url: &str, to: &Path) -> Result<Copied> {
    if to.exists() {
        bail!("{} already exists", to.display());
    }
    let runtime = runtime()?;
    let (store, path) = open(url)?;
    if !is_prefix(url) {
        match runtime.block_on(store.head(&path)) {
            Ok(_) => {
                let bytes = fetch(url, to)?;
                return Ok(Copied { files: 1, bytes });
            }
            Err(object_store::Error::NotFound { .. }) => {}
            Err(e) => return Err(e).with_context(|| format!("reading {}", redact(url))),
        }
    }
    let listed: Vec<object_store::ObjectMeta> = runtime.block_on(async {
        use futures::TryStreamExt;
        store.list(Some(&path)).try_collect().await
    })?;
    if listed.is_empty() {
        bail!("nothing at {}", redact(url));
    }
    let base = path.parts().count();
    let mut copied = Copied::default();
    for meta in listed {
        let parts: Vec<String> = meta
            .location
            .parts()
            .skip(base)
            .map(|p| p.as_ref().to_owned())
            .collect();
        // A blob name is the remote side's to choose; one that climbs out of `to` is refused.
        if parts.is_empty() || parts.iter().any(|p| p == ".." || p.contains(['\\', ':'])) {
            bail!("refusing the blob name {}", meta.location);
        }
        let dest = parts.iter().fold(to.to_owned(), |d, p| d.join(p));
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let blob_url = parts.iter().fold(url.to_owned(), |u, p| join(&u, p));
        copied.bytes += fetch(&blob_url, &dest)?;
        copied.files += 1;
    }
    Ok(copied)
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("starting the runtime for a transfer")
}

/// The time now, as `20261125T093000Z`: the name `deploy/backup.sh` gives a checkpoint.
pub fn utc_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    stamp(secs)
}

/// [`utc_stamp`] for a given Unix time, by Howard Hinnant's days-to-civil, which needs no
/// calendar crate.
fn stamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_urls_are_recognised_and_iris_are_not() {
        for url in [
            "abfss://data@acct.dfs.core.windows.net/dumps/a.nt.gz",
            "az://data/a.ttl",
            "azure://data/a.ttl",
            "adl://acct.azuredatalakestore.net/a.ttl",
            "https://acct.blob.core.windows.net/data/a.ttl",
            "https://acct.dfs.core.windows.net/data/dir/a.parquet",
            "https://acct.blob.core.windows.net/data/a.ttl?sv=2024&sig=abc",
            "HTTPS://Acct.Blob.Core.Windows.Net/data/a.ttl",
        ] {
            assert!(is_blob_url(url), "{url} should be fetched");
        }
        for not in [
            "http://example.org/graph",
            "https://example.org/data.ttl",
            "https://blob.core.windows.net/data/a.ttl",
            "https://acct.blob.core.windows.net/",
            "https://acct.blob.core.windows.net",
            "https://acct.blob.core.windows.net.evil.example/a.ttl",
            "data.ttl",
            "--data",
            "az://",
        ] {
            assert!(!is_blob_url(not), "{not} should be left alone");
        }
    }

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn urls_become_paths_that_keep_the_blob_name() {
        let mut calls = Vec::new();
        let mut fake = |url: &str, to: &Path| -> Result<u64> {
            calls.push(url.to_owned());
            std::fs::write(to, b"<a> <b> <c> .\n")?;
            Ok(14)
        };
        let local = localise_with(
            os(&[
                "semops",
                "load",
                "--store",
                "db",
                "--data",
                "abfss://data@acct.dfs.core.windows.net/dumps/a.nt.gz",
                "--data=https://acct.blob.core.windows.net/data/b.ttl?sv=1&sig=s",
                "--base",
                "https://example.org/",
                "--data",
                "abfss://data@acct.dfs.core.windows.net/dumps/a.nt.gz",
            ]),
            &mut fake,
        )
        .unwrap();
        let args: Vec<String> = local
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(&args[..5], ["semops", "load", "--store", "db", "--data"]);
        assert!(args[5].ends_with("a.nt.gz"), "{}", args[5]);
        assert!(Path::new(&args[5]).is_file());
        assert!(
            args[6].starts_with("--data=") && args[6].ends_with("b.ttl"),
            "{}",
            args[6]
        );
        assert_eq!(
            args[7..9],
            ["--base", "https://example.org/"],
            "an IRI is left alone"
        );
        assert_eq!(args[10], args[5], "the same URL is fetched once");
        assert_eq!(calls.len(), 2);
        assert_eq!(
            local.source_of(Path::new(&args[5])),
            Some("abfss://data@acct.dfs.core.windows.net/dumps/a.nt.gz")
        );

        let dir = Path::new(&args[5])
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_owned();
        drop(local);
        assert!(
            !dir.exists(),
            "the fetched files go when the command is done"
        );
    }

    #[test]
    fn a_command_line_without_urls_is_untouched_and_creates_nothing() {
        let args = os(&["semops", "query", "--data", "a.ttl", "--query", "ASK {}"]);
        let local = localise_with(args.clone(), &mut |_: &str, _: &Path| -> Result<u64> {
            panic!("nothing to fetch")
        })
        .unwrap();
        assert_eq!(local.args, args);
        assert!(local.fetched.is_empty() && local._dir.is_none());
    }

    #[test]
    fn a_failed_fetch_names_the_url_without_its_signature() {
        let err = localise_with(
            os(&[
                "semops",
                "load",
                "https://acct.blob.core.windows.net/d/a.ttl?sv=1&sig=SECRET",
            ]),
            &mut |_: &str, _: &Path| -> Result<u64> { bail!("403") },
        )
        .err()
        .expect("the fetch failed");
        let text = format!("{err:#}");
        assert!(
            text.contains("https://acct.blob.core.windows.net/d/a.ttl"),
            "{text}"
        );
        assert!(!text.contains("SECRET"), "{text}");
    }

    #[test]
    fn an_output_flag_is_never_fetched_and_refuses_a_blob_url() {
        let mut never = |_: &str, _: &Path| -> Result<u64> { panic!("outputs are not fetched") };
        for args in [
            &[
                "semops",
                "backup",
                "--store",
                "db",
                "--to",
                "az://backups/x",
            ][..],
            &[
                "semops",
                "check",
                "a.ttl",
                "--out=https://acct.blob.core.windows.net/r/x.sarif",
            ],
            &[
                "semops",
                "map",
                "run",
                "-o",
                "abfss://c@acct.dfs.core.windows.net/out.nt",
            ],
        ] {
            let err = localise_with(os(args), &mut never).err().expect("refused");
            assert!(
                format!("{err:#}").contains("cannot write to Blob Storage"),
                "{err:#}"
            );
        }
        let args = os(&["semops", "diff", "a", "b", "--out", "r.txt"]);
        let local = localise_with(args.clone(), &mut never).unwrap();
        assert_eq!(local.args, args);
    }

    #[test]
    fn upload_targets_follow_cp() {
        let ckpt = Path::new("var/backups/holos-20261125T020000Z");
        assert_eq!(
            upload_target(ckpt, "az://backups/"),
            "az://backups/holos-20261125T020000Z"
        );
        assert_eq!(
            upload_target(ckpt, "az://backups/nightly"),
            "az://backups/nightly"
        );
        assert_eq!(
            upload_target(
                Path::new("r.sarif"),
                "https://a.blob.core.windows.net/c/?sv=1&sig=s"
            ),
            "https://a.blob.core.windows.net/c/r.sarif?sv=1&sig=s"
        );
    }

    #[test]
    fn stamps_are_utc_calendar_times() {
        assert_eq!(stamp(0), "19700101T000000Z");
        assert_eq!(stamp(951_782_400), "20000229T000000Z");
        assert_eq!(stamp(1_795_599_000), "20261125T093000Z");
    }

    #[test]
    fn file_names_fall_back_when_the_url_has_none() {
        assert_eq!(file_name("az://data/dir/a.ttl.gz"), "a.ttl.gz");
        assert_eq!(
            file_name("https://acct.blob.core.windows.net/d/a.ttl?sv=1&sig=x"),
            "a.ttl"
        );
        assert_eq!(file_name("az://data/dir/"), "dir");
        assert_eq!(file_name("az://"), "blob");
    }
}
