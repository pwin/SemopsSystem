//! The shared function table (`semops_functions::cases`) as SHACL-SPARQL constraints: one
//! `sh:sparql` constraint per case, each reporting a violation when its function does not
//! give the expected value. A validator that registers the library conforms.

use holos_shacl_engine::model::{Graph, GraphBuilder, TermStore, Vocab, loader};
use holos_shacl_engine::validate::Options;
use oxrdfio::RdfFormat;
use semops_functions::cases;

fn failures(shapes_text: &str) -> Vec<String> {
    let mut store = TermStore::new();
    let vocab = Vocab::new(&mut store);
    let graph = |text: &str, scope: u32, store: &mut TermStore| -> Graph {
        let mut b = GraphBuilder::new();
        loader::parse_str(text, RdfFormat::Turtle, "http://t/", scope, store, &mut b)
            .expect("test document should parse");
        b.build()
    };
    let data = graph(cases::DATA, 0, &mut store);
    let shapes = graph(shapes_text, 1, &mut store);
    let compiled = holos_shacl_engine::shapes::Shapes::compile(&shapes, &store, &vocab)
        .expect("shapes should compile");
    let report = holos_shacl_engine::validate::validate_in_with(
        &data,
        &compiled,
        &shapes,
        &mut store,
        &vocab,
        Options::default(),
    )
    .expect("validation should succeed");
    report
        .results
        .iter()
        .map(|r| {
            let message = r
                .messages
                .iter()
                .map(|m| store.to_oxrdf(*m).to_string())
                .collect::<Vec<_>>()
                .join(" ");
            let value = r
                .value
                .map(|v| store.to_oxrdf(v).to_string())
                .unwrap_or_default();
            format!("{message} gave {value}")
        })
        .collect()
}

#[test]
fn every_function_works_in_a_sparql_constraint() {
    let failures = failures(&cases::shapes());
    assert!(
        failures.is_empty(),
        "{} of {} fail:
{}",
        failures.len(),
        cases::CASES.len(),
        failures.join(
            "
"
        )
    );
}

/// The control: a case that does not hold is reported, so a conforming report above means
/// the constraints ran.
#[test]
fn a_wrong_answer_is_a_violation() {
    let wrong = [
        cases::ok(r#"fn:upper-case("a")"#, r#""b""#),
        cases::error(r#"spif:trim(" x ")"#),
    ];
    assert_eq!(failures(&cases::shapes_for(&wrong)).len(), 2);
}
