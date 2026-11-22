//! The engines' commands as `semops` subcommands, and the binary standing in for them under
//! their own names.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(program: &Path, args: &[&str]) -> Run {
    let output = Command::new(program)
        .args(args)
        .output()
        .expect("the command runs");
    Run {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn semops(args: &[&str]) -> Run {
    run(Path::new(env!("CARGO_BIN_EXE_semops")), args)
}

/// A directory of its own for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("semops-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

fn write(dir: &Path, name: &str, text: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, text).expect("write");
    path.to_str().expect("a UTF-8 path").to_owned()
}

const DATA: &str = "@prefix ex: <https://example.org/> .
ex:inkwell a ex:Shop ; ex:staff 3 .
ex:bookwyrm a ex:Shop ; ex:staff 0 .
";

const SHAPES: &str = "@prefix ex: <https://example.org/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
ex:ShopShape a sh:NodeShape ; sh:targetClass ex:Shop ;
  sh:property [ sh:path ex:staff ; sh:minInclusive 1 ] .
";

#[test]
fn the_help_lists_every_command() {
    let r = semops(&["--help"]);
    assert_eq!(r.code, 0);
    for command in [
        "serve", "load", "query", "update", "validate", "map", "stats", "dump", "backup",
        "compact", "entail", "rules", "check", "diff",
    ] {
        assert!(
            r.stdout.contains(&format!("  {command} ")),
            "{command}: {}",
            r.stdout
        );
    }
}

#[test]
fn each_command_answers_with_its_engine() {
    // --version and --help go through to the engine rather than to semops.
    assert!(
        semops(&["serve", "--version"])
            .stdout
            .starts_with("holos-server ")
    );
    assert!(
        semops(&["validate", "--version"])
            .stdout
            .starts_with("shacl ")
    );
    assert!(semops(&["map", "--version"]).stdout.starts_with("tarka "));
    let query_help = semops(&["query", "--help"]);
    assert_eq!(query_help.code, 0);
    assert!(
        query_help.stdout.contains("holos query"),
        "{}",
        query_help.stdout
    );
}

#[test]
fn validate_exits_as_shacl_does() {
    let dir = scratch("validate");
    let data = write(&dir, "data.ttl", DATA);
    let shapes = write(&dir, "shapes.ttl", SHAPES);
    let r = semops(&["validate", "--data", &data, "--shapes", &shapes]);
    assert_eq!(r.code, 1, "one shop has no staff: {}{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("bookwyrm"), "{}", r.stdout);
    let fixed = write(&dir, "fixed.ttl", &DATA.replace("ex:staff 0", "ex:staff 2"));
    let r = semops(&["validate", "--data", &fixed, "--shapes", &shapes]);
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
    let r = semops(&[
        "validate",
        "--data",
        &dir.join("absent.ttl").to_string_lossy(),
    ]);
    assert_eq!(r.code, 2, "a missing file is not an answer");
}

#[test]
fn load_then_query_a_store() {
    let dir = scratch("store");
    let data = write(&dir, "data.ttl", DATA);
    let store = dir.join("store");
    let store = store.to_str().expect("path");
    let r = semops(&["load", "--store", store, "--data", &data]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stderr.contains("loaded 4 quads"), "{}", r.stderr);
    let r = semops(&[
        "query",
        "--store",
        store,
        "--results",
        "csv",
        "--query",
        "SELECT (COUNT(*) AS ?n) WHERE { ?s a <https://example.org/Shop> }",
    ]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stdout.lines().any(|l| l.trim() == "2"), "{}", r.stdout);
    // load without a store would load into memory and throw it away.
    let r = semops(&["load", "--data", &data]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.contains("needs both"), "{}", r.stderr);
}

#[test]
fn map_a_csv_with_tarql() {
    let dir = scratch("map");
    let csv = write(&dir, "shops.csv", "name,staff\nInkwell,3\nBookwyrm,0\n");
    let query = write(
        &dir,
        "shops.tarql",
        "PREFIX ex: <https://example.org/>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
CONSTRUCT { ?shop a ex:Shop ; ex:staff ?n }
WHERE {
  BIND (IRI(CONCAT(STR(ex:), LCASE(?name))) AS ?shop)
  BIND (xsd:integer(?staff) AS ?n)
}",
    );
    let r = semops(&["map", "run", "-q", &query, "-i", &csv]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stdout.contains("ex:bookwyrm a ex:Shop"), "{}", r.stdout);
    assert!(r.stdout.contains("ex:staff 0"), "{}", r.stdout);
}

#[test]
fn under_another_name_it_is_that_command() {
    let dir = scratch("aliases");
    let exe = std::env::consts::EXE_SUFFIX;
    for (name, answer) in [
        ("holos-server", "holos-server "),
        ("holos", "holos "),
        ("shacl", "shacl "),
        ("tarka", "tarka "),
    ] {
        let copy = dir.join(format!("{name}{exe}"));
        std::fs::copy(env!("CARGO_BIN_EXE_semops"), &copy).expect("copy the binary");
        let r = run(&copy, &["--version"]);
        assert_eq!(r.code, 0, "{name}: {}", r.stderr);
        assert!(r.stdout.starts_with(answer), "{name}: {}", r.stdout);
    }
}
