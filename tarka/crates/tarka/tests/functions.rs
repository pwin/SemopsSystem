//! The shared function table (`semops_functions::cases`) through tarka's own evaluation: each
//! case as a TARQL mapping over a one-row CSV, writing whether the function gave the expected
//! value. HOLOS queries and both SHACL validators run the same table.

use holos_engine::oxrdf::{Term, Triple};
use semops_functions::cases;
use tarka::RunOptions;
use tarka::io::{CsvOptions, CsvSource};

const CSV: &str = "column\nvalue\n";

fn mapping(case: &cases::Case) -> String {
    let check = match case.expected {
        Some(expected) => format!("BIND({expected} AS ?e) BIND(COALESCE({}, false) AS ?ok)", cases::AGREES),
        None => "BIND(!BOUND(?r) AS ?ok)".to_owned(),
    };
    format!(
        "{}CONSTRUCT {{ <urn:case> <urn:ok> ?ok . <urn:case> <urn:r> ?r }} WHERE {{ BIND({} AS ?r) {check} }}",
        cases::PREFIXES,
        case.expression
    )
}

#[test]
fn every_function_works_in_a_tarql_mapping() {
    let mut failures = Vec::new();
    for case in cases::CASES {
        let plan = tarka::tarql::parse_tarql(&mapping(case), "case").unwrap_or_else(|e| panic!("{}: {e}", case.expression));
        let source = CsvSource::new(CSV.as_bytes(), CsvOptions { has_headers: true, ..CsvOptions::default() }).unwrap();
        let columns = source.columns().to_vec();
        let mut triples: Vec<Triple> = Vec::new();
        tarka::run(&plan, &columns, source, &mut triples, &RunOptions::default(), &mut |r| panic!("left out: {r}"))
            .unwrap_or_else(|e| panic!("{}: {e}", case.expression));
        let object = |p: &str| triples.iter().find(|t| t.predicate.as_str() == p).map(|t| t.object.clone());
        let holds = matches!(object("urn:ok"), Some(Term::Literal(l)) if l.value() == "true");
        if !holds {
            failures.push(format!("{} gave {:?}", case.expression, object("urn:r").map(|t| t.to_string())));
        }
    }
    assert!(failures.is_empty(), "{} of {} fail:\n{}", failures.len(), cases::CASES.len(), failures.join("\n"));
}
