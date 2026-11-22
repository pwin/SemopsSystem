#!/usr/bin/env node
//
// The combined module, run in Node: the store, the SHACL engine and the quality checks from
// one require().
//
//   node crates/semops-wasm/build.mjs nodejs
//   node crates/semops-wasm/tests/smoke.cjs
//
// As holos-wasm's smoke test says, the faults that matter here compile for
// wasm32-unknown-unknown and fail only when called (threads, clocks), so only running the
// module in a real host checks them.
'use strict';

const path = require('node:path');
const assert = require('node:assert');

const PKG = path.join(__dirname, '..', 'pkg-node', 'semops_wasm.js');
let semops;
try {
  semops = require(PKG);
} catch (err) {
  console.error(`could not load ${PKG}`);
  console.error('build it first: node crates/semops-wasm/build.mjs nodejs');
  console.error(String(err).split('\n')[0]);
  process.exit(2);
}

const DATA = `
@prefix ex: <https://example.org/> .
ex:inkwell a ex:Shop ; ex:staff 3 .
ex:bookwyrm a ex:Shop ; ex:staff 0 .
`;

const SHAPES = `
@prefix ex: <https://example.org/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
ex:ShopShape a sh:NodeShape ; sh:targetClass ex:Shop ;
  sh:property [ sh:path ex:staff ; sh:minInclusive 1 ] .
`;

const ONTOLOGY = `
@prefix owl:  <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <https://example.org/o#> .
<https://example.org/o> a owl:Ontology .
ex:Shop a owl:Class .
ex:Bookshop rdfs:subClassOf ex:Shop .
ex:sells a owl:ObjectProperty ; rdfs:domain ex:Shop ; rdfs:range ex:Undeclared .
`;

let passed = 0;
function test(name, fn) {
  fn();
  passed += 1;
  console.log(`ok - ${name}`);
}

test('the version is the workspace release', () => {
  assert.match(semops.version(), /^\d+\.\d+\.\d+/);
});

test('the store loads Turtle and answers SPARQL', () => {
  const store = new semops.Store();
  assert.strictEqual(store.loadTurtle(DATA, undefined), 4);
  const rows = store.query(
    'SELECT ?s WHERE { ?s <https://example.org/staff> ?n FILTER(?n > 0) }', undefined);
  assert.strictEqual(rows.length, 1);
  // NOW() reaches the JavaScript clock rather than panicking.
  assert.strictEqual(store.query('ASK { FILTER(YEAR(NOW()) > 2000) }', undefined), true);
});

test('the SHACL engine validates', () => {
  const report = semops.validateTurtle(DATA, SHAPES, undefined);
  assert.strictEqual(report.conforms, false);
  assert.strictEqual(report.length, 1);
  const validator = semops.Validator.fromTurtle(SHAPES, undefined);
  const fixed = validator.validateTurtle(DATA.replace('ex:staff 0', 'ex:staff 2'), undefined);
  assert.strictEqual(fixed.conforms, true);
  assert.ok(semops.rdfsClosure(SHAPES, undefined).length > 0);
});

test('the quality checks run', () => {
  const checks = semops.listChecks();
  assert.strictEqual(checks.length, 61);
  assert.ok(checks.every((c) => c.id && c.title && c.defaultSeverity));
  const run = semops.check(ONTOLOGY, 'turtle', 'both', undefined);
  assert.strictEqual(run.complete, true, JSON.stringify(run.failed));
  assert.ok(run.findings.length > 0, 'an undeclared range class and unlabelled terms');
  const ids = new Set(run.findings.map((f) => f.checkId));
  assert.ok([...ids].some((id) => id && id.length > 0));
  assert.ok(run.findings.every((f) => ['Violation', 'Warning', 'Info'].includes(f.severity)));
});

test('a bad name is an error, not a silent default', () => {
  assert.throws(() => semops.check(ONTOLOGY, 'yaml', undefined, undefined), /unknown RDF format/);
  assert.throws(() => semops.check(ONTOLOGY, undefined, 'owl', undefined), /unknown engine/);
});

console.log(`\n${passed} passed`);
