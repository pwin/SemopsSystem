# Provenance

This is **spareval 0.2.9** from the [Oxigraph](https://github.com/oxigraph/oxigraph) project,
as published on [crates.io](https://crates.io/crates/spareval/0.2.9), with the one change below.
The workspace `Cargo.toml` puts it in place of the registry crate with `[patch.crates-io]`, so
every crate here resolves this copy: HOLOS, its adaptation of the SHACL engine, SHACL_Engine
itself and tarka. Before SemOps System, HOLOS and SHACL_Engine each vendored the same `src/`
and tarka took HOLOS's by git tag; this is that one copy.

| | |
|---|---|
| Upstream | https://github.com/oxigraph/oxigraph/tree/main/lib/spareval |
| Taken from | the crates.io package `spareval-0.2.9`, unpacked |
| Licence | MIT OR Apache-2.0, the same terms as this workspace |
| Copyright | the Oxigraph developers |

The package on crates.io carries no licence files, so `LICENSE-MIT` and `LICENSE-APACHE` here
were taken from the root of the Oxigraph repository. `Cargo.toml` is the normalised manifest
cargo publishes, unchanged but for one line: its `resolver = "2"` is removed, because cargo
ignores a resolver set by a package that is not the workspace root and warns about it on every
command. `Cargo.toml.orig` is the one it was normalised from and is not read
by cargo.

## The change

`src/expression.rs`: the evaluator for `Expression::Add`, and one function,
`temporal_operand_first`, placed before `enum NumericBinaryOperands`.

With the `sep-0002` feature, upstream adds a duration to an `xsd:date`, `xsd:time` or
`xsd:dateTime` only when the temporal value comes first: `NumericBinaryOperands::new` has a case
for (dateTime, duration) and none for (duration, dateTime), so the second order is an error and
leaves its variable unbound. XPath's operator mapping defines `A + B` for both orders and computes
both as the temporal value plus the duration (`op:add-yearMonthDuration-to-dateTime(B, A)` when
`A` is the duration, and so on).

That gap is wider than a query written with the duration first. `sparopt`'s optimizer, which
`spareval` runs before evaluating, rebuilds every `+` through `order_pair`
(`sparopt/src/algebra.rs`), and `order_pair` orders the two operands by their `DefaultHasher`
hash, as if `+` commuted for every type. So `?d + "P2Y"^^xsd:yearMonthDuration`, written the
supported way round, reached the evaluator reversed whenever the duration hashed lower, and its
result was unbound. A hash of the same value differs between 64-bit and 32-bit builds: of thirty
pairs of six dateTimes and five durations, HOLOS 0.26.3 bound 7 natively and 24 in WebAssembly.

The change puts the temporal operand first before `NumericBinaryOperands::new` is consulted, for
`+` only. Both orders, and so both outcomes of the hash, now give the sum. Subtraction does not
commute and is untouched: `duration - dateTime` is still an error. Without `sep-0002` the
function returns its arguments as they came.

## Tests

`crates/holos-engine/tests/date_arithmetic.rs` adds the thirty pairs in both orders through the
whole engine, optimizer included, and compares each sum with `oxsdatatypes` computing it directly;
dates and times in both orders; and checks that `duration - dateTime` stays unbound. Against the
registry crate the pair matrix and the known sums fail. The package's own tests, 14 unit tests
and 18 doctests in `src/`, run in this workspace, where the copy is a member, and pass with the
change.

## Updating

Dropping this copy is right once an upstream release accepts `+` with the duration on either side,
or once `sparopt` stops reordering `+` across types and HOLOS can leave the written order to
users. Before that, moving to a later 0.2.x means taking its package and reapplying the change,
which is self-contained in `src/expression.rs`: the `Expression::Add` evaluator's first line and
`temporal_operand_first`.

`spareval` must stay at the version the other Oxigraph crates in the lock expect, and pins
`oxsdatatypes` to an exact version, which the copy in `vendor/oxsdatatypes` has to match.

## In SemOps System

HOLOS (pwin/triplestore 0.27.0) and SHACL_Engine (0.3.9) each carried this package with the
same `src/`; the only difference was that SHACL_Engine's copy of the new function had been
through rustfmt. This copy takes that formatting, so `cargo fmt --all --check` passes over it.
It is a workspace member, as it was in HOLOS, so its 32 tests run here. CI runs clippy with
`--exclude spareval`: under clippy's defaults Oxigraph's code draws `type_complexity` and
`too_many_arguments`, which are its design choices, not defects. That is why SHACL_Engine had
excluded it from its workspace altogether.

Clippy still reaches it, though: it lints every workspace member a build compiles, selected
or not, so `cargo clippy -p shacl -- -D warnings` failed here on 17 of these (16
`type_complexity`, 1 `too_many_arguments`). The manifest's `[lints.clippy]` table therefore
allows those two, under a comment saying so. That is the only change to `Cargo.toml` besides
the `resolver` line; upstream's other lint settings are kept.
