# semops-checks

The ontology quality suite's check registry, compiled in: 61 checks, their SHACL shapes,
their portable SPARQL queries and the repair templates, from
[ontology-quality-suite](https://github.com/pwin/consolidated-ontology-quality-suite-python)
0.28.0 (see `PROVENANCE.md`). With the default `run` feature it also runs them on HOLOS:

```rust
use std::path::Path;
use semops_checks::run;

let triples = run::load(&[Path::new("ontology.ttl"), Path::new("data.ttl")])?;
let report = run::check(&triples)?;
for f in &report.findings {
    println!("{} {} {}", f.severity, f.check_id.as_deref().unwrap_or("?"), f.message);
}
```

The findings are the ones `ontology-suite check` reports with the same `--engine` (the portable
queries on HOLOS, the shapes on the SHACL engine, or both), held to the suite's
own output by `tests/parity.rs`. `semops check` is the command-line front end.

Without the `run` feature the crate is the embedded files and a typed registry
(`Registry`, `files`, `shapes`, `queries`), for a build that brings its own engine.
