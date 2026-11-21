//! The function table every evaluator is tested against.
//!
//! One table of expressions and the values they must give, run through each evaluator: this
//! crate's own tests, HOLOS queries, both SHACL validators' SPARQL constraints, and tarka's
//! mappings. A function that works in one and not another fails a test, rather than turning
//! up as a support question.
//!
//! An expected value is SPARQL term syntax. A literal must have the expected datatype and an
//! equal value (so `"3"^^xsd:decimal` and `3.0` are the same answer); an IRI must be the same
//! IRI. `None` means the call is an error: the expression is unbound.

/// One call and its answer.
#[derive(Clone, Copy, Debug)]
pub struct Case {
    pub expression: &'static str,
    pub expected: Option<&'static str>,
}

/// A case that gives `expected`.
#[must_use]
pub const fn ok(expression: &'static str, expected: &'static str) -> Case {
    Case {
        expression,
        expected: Some(expected),
    }
}

/// A case that is an error.
#[must_use]
pub const fn error(expression: &'static str) -> Case {
    Case {
        expression,
        expected: None,
    }
}

/// The prefixes the expressions use.
pub const PREFIXES: &str = "PREFIX fn: <http://www.w3.org/2005/xpath-functions#>
PREFIX math: <http://www.w3.org/2005/xpath-functions/math#>
PREFIX afn: <http://jena.apache.org/ARQ/function#>
PREFIX spif: <http://spinrdf.org/spif#>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
";

/// The table. Deterministic calls only: what gives the time, a random number or a fresh
/// identifier is tested where it is defined.
pub const CASES: &[Case] = &[
    // fn: strings
    ok(r#"fn:upper-case("abc")"#, r#""ABC""#),
    ok(r#"fn:string-length("héllo")"#, "5"),
    ok(r#"fn:substring("abcdef", 2, 3)"#, r#""bcd""#),
    ok(r#"fn:substring("12345", -3, 5)"#, r#""1""#),
    ok(r#"fn:substring("12345", "NaN"^^xsd:double)"#, r#""""#),
    ok(r#"fn:substring-before("tattoo", "attoo")"#, r#""t""#),
    ok(r#"fn:normalize-space("  a  b ")"#, r#""a b""#),
    ok(r#"fn:translate("bar", "abc", "ABC")"#, r#""BAr""#),
    ok(r#"fn:compare("a", "b")"#, "-1"),
    ok(r#"fn:matches("Kaboom", "^k", "i")"#, "true"),
    ok(
        r#"fn:replace("abracadabra", "a(.)", "a$1$1")"#,
        r#""abbraccaddabbra""#,
    ),
    error(r#"fn:replace("abc", "x*", "y")"#),
    ok(r#"fn:encode-for-uri("a b/c")"#, r#""a%20b%2Fc""#),
    ok(
        r#"fn:iri-to-uri("http://example.com/a b")"#,
        r#""http://example.com/a%20b""#,
    ),
    ok(
        r#"fn:escape-html-uri("http://example.com/é")"#,
        r#""http://example.com/%C3%A9""#,
    ),
    // fn: numbers, keeping their types
    ok("fn:abs(-3)", "3"),
    ok("fn:abs(-2.5)", "2.5"),
    ok(r#"fn:round("-2.5"^^xsd:double)"#, r#""-2"^^xsd:double"#),
    ok("fn:round(2.5)", r#""3"^^xsd:decimal"#),
    ok("fn:round-half-to-even(2.5)", r#""2"^^xsd:decimal"#),
    ok("fn:round(3.14159, 2)", "3.14"),
    ok("fn:ceiling(-1.5)", r#""-1"^^xsd:decimal"#),
    ok(r#"fn:floor("2.7"^^xsd:double)"#, r#""2"^^xsd:double"#),
    ok(r#"fn:number("12.5")"#, r#""12.5"^^xsd:double"#),
    ok(r#"fn:number("abc")"#, r#""NaN"^^xsd:double"#),
    ok(
        r##"fn:format-number(12345.678, "#,##0.00")"##,
        r#""12,345.68""#,
    ),
    ok(r##"fn:format-number(0.256, "#%")"##, r#""26%""#),
    ok(r#"fn:boolean("")"#, "false"),
    ok("fn:not(0)", "true"),
    // fn: dates, times and durations
    ok(
        r#"fn:year-from-dateTime("2026-08-25T13:45:07Z"^^xsd:dateTime)"#,
        "2026",
    ),
    ok(
        r#"fn:seconds-from-dateTime("2026-08-25T13:45:07.5Z"^^xsd:dateTime)"#,
        "7.5",
    ),
    ok(
        r#"fn:timezone-from-dateTime("2026-08-25T13:45:07+01:00"^^xsd:dateTime)"#,
        r#""PT1H"^^xsd:dayTimeDuration"#,
    ),
    ok(r#"fn:month-from-date("2026-08-25"^^xsd:date)"#, "8"),
    ok(r#"fn:hours-from-time("13:45:07"^^xsd:time)"#, "13"),
    ok(r#"fn:days-from-duration("P3DT10H"^^xsd:duration)"#, "3"),
    ok(
        r#"fn:adjust-dateTime-to-timezone("2026-08-25T13:00:00+01:00"^^xsd:dateTime, "PT0S"^^xsd:dayTimeDuration)"#,
        r#""2026-08-25T12:00:00Z"^^xsd:dateTime"#,
    ),
    ok(
        r#"fn:format-dateTime("2026-08-25T13:05:07+01:00"^^xsd:dateTime, "[D1o] [MNn] [Y], [h]:[m01] [PN]")"#,
        r#""25th August 2026, 1:05 PM""#,
    ),
    ok(
        r#"fn:format-date("2026-08-25"^^xsd:date, "[FNn]")"#,
        r#""Tuesday""#,
    ),
    error(r#"fn:year-from-dateTime("not a date")"#),
    // math:
    ok("math:pow(2, 10)", r#""1024"^^xsd:double"#),
    ok("math:sqrt(16)", r#""4"^^xsd:double"#),
    ok("math:log10(1000)", r#""3"^^xsd:double"#),
    ok("math:exp10(2)", r#""100"^^xsd:double"#),
    ok("math:cos(0)", r#""1"^^xsd:double"#),
    ok("math:log(0)", r#""-INF"^^xsd:double"#),
    ok("math:pi()", r#""3.141592653589793"^^xsd:double"#),
    error(r#"math:sqrt("sixteen"@en)"#),
    // afn:
    ok("afn:localname(<http://example.com/ns#thing>)", r#""thing""#),
    ok(
        "afn:namespace(<http://example.com/item/123>)",
        r#""http://example.com/item/123""#,
    ),
    ok(
        "<http://jena.hpl.hp.com/ARQ/function#localname>(<http://example.com/a>)",
        r#""a""#,
    ),
    error(r#"afn:localname("not an IRI")"#),
    ok(r#"afn:substr("abcdef", 1, 3)"#, r#""bc""#),
    error(r#"afn:substr("abc", 2, 5)"#),
    ok(r#"afn:strjoin("-", "a", "b", "c")"#, r#""a-b-c""#),
    ok(
        r#"afn:sha1sum("abc")"#,
        r#""a9993e364706816aba3e25717850c26c9cd0d89d""#,
    ),
    ok("afn:max(3, 2.5)", "3"),
    ok(r#"afn:sprintf("%05.1f|%x", 3.14159, 255)"#, r#""003.1|ff""#),
    // spif:
    ok(
        r#"spif:buildString("{?1} and {?2}", "x", "y")"#,
        r#""x and y""#,
    ),
    ok(
        r#"spif:buildURI("<http://example.com/{?1}>", "x")"#,
        "<http://example.com/x>",
    ),
    ok(
        r#"spif:titleCase("hello wide world")"#,
        r#""Hello Wide World""#,
    ),
    ok(r#"spif:unCamelCase("someLongName")"#, r#""some Long Name""#),
    ok(r#"spif:indexOf("héllo", "llo")"#, "2"),
    ok(r#"spif:encodeURL("a b")"#, r#""a%20b""#),
    ok(r#"spif:decodeURL("a%20b")"#, r#""a b""#),
    ok(
        r#"spif:replaceAll("2026-08-25", "(\\d+)-(\\d+)-(\\d+)", "$3/$2/$1")"#,
        r#""25/08/2026""#,
    ),
    ok(
        r#"spif:parseDate("25/08/2026", "dd/MM/yyyy")"#,
        r#""2026-08-25"^^xsd:date"#,
    ),
    ok(
        r#"spif:parseDate("13:05", "HH:mm")"#,
        r#""13:05:00"^^xsd:time"#,
    ),
    error(r#"spif:parseDate("31/02/2026", "dd/MM/yyyy")"#),
    ok(
        r#"spif:dateFormat("2026-08-05"^^xsd:date, "EEE, d MMM yyyy")"#,
        r#""Wed, 5 Aug 2026""#,
    ),
    ok(
        r##"spif:decimalFormat(1234567.891, "#,##0.00")"##,
        r#""1,234,567.89""#,
    ),
    ok(r#"spif:isValidURI("http://example.com/a")"#, "true"),
    ok(r#"spif:isValidURI("not a uri")"#, "false"),
    ok("spif:mod(7, 3)", "1"),
    ok("spif:mod(-7, 3)", "-1"),
    ok(r#"spif:cast("42", xsd:integer)"#, "42"),
    ok(
        r#"spif:timeMillis("1970-01-01T00:00:01Z"^^xsd:dateTime)"#,
        r#""1000"^^xsd:long"#,
    ),
];

/// The SPARQL condition that holds when `?r` is the answer `?e`.
pub const AGREES: &str = "((isLITERAL(?r) && isLITERAL(?e) && DATATYPE(?r) = DATATYPE(?e) && (?r = ?e || STR(?r) = STR(?e))) || (!isLITERAL(?r) && sameTerm(?r, ?e)))";

/// A query with one solution, binding `?ok` to whether the case holds.
#[must_use]
pub fn query(case: &Case) -> String {
    match case.expected {
        Some(expected) => format!(
            "{PREFIXES}SELECT ?ok ?r WHERE {{ BIND({} AS ?r) BIND({expected} AS ?e) BIND(COALESCE({AGREES}, false) AS ?ok) }}",
            case.expression
        ),
        None => format!(
            "{PREFIXES}SELECT ?ok ?r WHERE {{ BIND({} AS ?r) BIND(!BOUND(?r) AS ?ok) }}",
            case.expression
        ),
    }
}

/// A shapes graph (Turtle) with one SPARQL constraint per case on `<urn:semops:cases:focus>`,
/// each reporting a violation when its case does not hold. Valid data conforms: a validator
/// whose report is not empty has a function that fails, and the message says which.
#[must_use]
pub fn shapes() -> String {
    shapes_for(CASES)
}

/// [`shapes`] for any cases: a test can give a wrong one, to show the constraints fire.
#[must_use]
pub fn shapes_for(cases: &[Case]) -> String {
    let mut out = String::from(
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
         <urn:semops:cases:prefixes> sh:declare\n\
           [ sh:prefix \"fn\" ; sh:namespace \"http://www.w3.org/2005/xpath-functions#\"^^xsd:anyURI ] ,\n\
           [ sh:prefix \"math\" ; sh:namespace \"http://www.w3.org/2005/xpath-functions/math#\"^^xsd:anyURI ] ,\n\
           [ sh:prefix \"afn\" ; sh:namespace \"http://jena.apache.org/ARQ/function#\"^^xsd:anyURI ] ,\n\
           [ sh:prefix \"spif\" ; sh:namespace \"http://spinrdf.org/spif#\"^^xsd:anyURI ] ,\n\
           [ sh:prefix \"xsd\" ; sh:namespace \"http://www.w3.org/2001/XMLSchema#\"^^xsd:anyURI ] .\n\
         <urn:semops:cases:shape> a sh:NodeShape ;\n\
           sh:targetNode <urn:semops:cases:focus> ",
    );
    for (i, case) in cases.iter().enumerate() {
        let condition = match case.expected {
            Some(expected) => format!("BIND({expected} AS ?e) FILTER(!COALESCE({AGREES}, false))"),
            None => "FILTER(BOUND(?r))".to_owned(),
        };
        let select = format!(
            "SELECT $this ?value WHERE {{ BIND({} AS ?r) {condition} BIND(COALESCE(STR(?r), \"unbound\") AS ?value) }}",
            case.expression
        );
        out.push_str(&format!(
            ";\n  sh:sparql [ sh:prefixes <urn:semops:cases:prefixes> ; sh:message {} ; sh:select {} ]",
            turtle_string(&format!("case {i}: {}", case.expression)),
            turtle_string(&select)
        ));
    }
    out.push_str(" .\n");
    out
}

/// The data the shapes graph targets.
pub const DATA: &str = "<urn:semops:cases:focus> <urn:semops:cases:p> \"x\" .\n";

fn turtle_string(s: &str) -> String {
    let mut out = String::from("\"\"\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            c => out.push(c),
        }
    }
    out.push_str("\"\"\"");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxrdf::Dataset;
    use spareval::{QueryEvaluator, QueryResults};
    use spargebra::SparqlParser;

    /// The table, through spareval with nothing but this library registered.
    #[test]
    fn every_case_holds() {
        let evaluator = crate::register(QueryEvaluator::new());
        let mut failures = Vec::new();
        for case in CASES {
            let text = query(case);
            let parsed = SparqlParser::new()
                .parse_query(&text)
                .unwrap_or_else(|e| panic!("{}: {e}", case.expression));
            let dataset = Dataset::new();
            let QueryResults::Solutions(mut solutions) =
                evaluator.prepare(&parsed).execute(&dataset).unwrap()
            else {
                unreachable!()
            };
            let solution = solutions.next().unwrap().unwrap();
            let holds =
                matches!(solution.get("ok"), Some(oxrdf::Term::Literal(l)) if l.value() == "true");
            if !holds {
                failures.push(format!(
                    "{} gave {:?}, wanted {:?}",
                    case.expression,
                    solution.get("r").map(ToString::to_string),
                    case.expected
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {} cases fail:\n{}",
            failures.len(),
            CASES.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn the_shapes_graph_parses_with_a_constraint_per_case() {
        let shapes = shapes();
        let quads: Vec<oxrdf::Quad> = oxrdfio::RdfParser::from_format(oxrdfio::RdfFormat::Turtle)
            .for_reader(shapes.as_bytes())
            .collect::<Result<_, _>>()
            .unwrap_or_else(|e| panic!("{e}\n{shapes}"));
        let selects = quads
            .iter()
            .filter(|q| q.predicate.as_str() == "http://www.w3.org/ns/shacl#select")
            .count();
        assert_eq!(selects, CASES.len());
    }
}
