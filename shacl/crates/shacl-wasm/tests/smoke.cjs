#!/usr/bin/env node
//
// The shacl-wasm Node build, loaded and run: what has to work before it is published.
//
//   node shacl/crates/shacl-wasm/build.mjs
//   node shacl/crates/shacl-wasm/tests/smoke.cjs
//
// As holos-wasm's smoke test says, the faults that matter here compile for
// wasm32-unknown-unknown and fail only when called, so only running the module in a real
// host checks them.
'use strict';

const path = require('node:path');
const assert = require('node:assert');

const PKG = path.join(__dirname, '..', 'pkg-node', 'shacl_wasm.js');
let shacl;
try {
  shacl = require(PKG);
} catch (err) {
  console.error(`could not load ${PKG}`);
  console.error('build it first: node shacl/crates/shacl-wasm/build.mjs');
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

let passed = 0;
function test(name, fn) {
  fn();
  passed += 1;
  console.log(`ok - ${name}`);
}

test('one call validates data against shapes', () => {
  const report = shacl.validateTurtle(DATA, SHAPES, undefined);
  assert.strictEqual(report.conforms, false);
  assert.strictEqual(report.length, 1);
  assert.match(report.toTurtle(), /ValidationReport/);
});

test('a compiled validator is reused', () => {
  const validator = shacl.Validator.fromTurtle(SHAPES, undefined);
  assert.strictEqual(validator.shapeCount, 2); // the node shape and its property shape
  const fixed = validator.validateTurtle(DATA.replace('ex:staff 0', 'ex:staff 2'), undefined, undefined);
  assert.strictEqual(fixed.conforms, true);
});

test('the RDFS closure is computed', () => {
  assert.ok(shacl.rdfsClosure(SHAPES, undefined).length > 0);
});

test('shapes that declare nothing are an error, not a pass', () => {
  assert.throws(() => shacl.validateTurtle(SHAPES, DATA, undefined), /declares no shapes/);
});

console.log(`${passed} passed`);
