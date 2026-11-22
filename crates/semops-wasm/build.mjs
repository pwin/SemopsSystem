#!/usr/bin/env node
// Builds the npm packages for semops-wasm: `semops-wasm` (ESM, for bundlers) and
// `semops-wasm-node` (CommonJS, for Node and require()).
//
// Wraps `wasm-pack build` as holos-wasm's build.mjs does, to set what wasm-pack cannot:
//
//   * the package name, since wasm-pack names both builds after the crate;
//   * the licence files, which npm's own rule does not pick up under the
//     LICENSE-APACHE / LICENSE-MIT names a dual-licensed package uses;
//   * `repository` and `homepage`.
//
// Usage:
//   node crates/semops-wasm/build.mjs                    # both
//   node crates/semops-wasm/build.mjs nodejs             # one
//   node crates/semops-wasm/build.mjs nodejs --no-opt    # without wasm-opt
//
// --no-opt skips binaryen's optimiser, for a build that only needs to run. The wasm-opt that
// wasm-pack downloads (version 117) crashes on Windows on a module this size; CI builds the
// published packages on Linux, where it does not.
//
// Then: node crates/semops-wasm/tests/smoke.cjs
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const REPOSITORY = 'https://github.com/pwin/SemopsSystem';
const LICENSES = ['LICENSE-APACHE', 'LICENSE-MIT'];

const TARGETS = {
  bundler: { outDir: 'pkg-bundler', name: 'semops-wasm', blurb: 'ESM, for bundlers' },
  nodejs: { outDir: 'pkg-node', name: 'semops-wasm-node', blurb: 'CommonJS, for Node and require()' },
};

const args = process.argv.slice(2);
const noOpt = args.includes('--no-opt');
const requested = args.filter((a) => !a.startsWith('--'));
const targets = requested.length ? requested : Object.keys(TARGETS);

for (const target of targets) {
  const spec = TARGETS[target];
  if (!spec) {
    console.error(`unknown target ${target}: expected one of ${Object.keys(TARGETS).join(', ')}`);
    process.exit(1);
  }
  const { outDir, name, blurb } = spec;

  console.log(`\n=== wasm-pack build --target ${target} --out-dir ${outDir} ===`);
  const flags = ['build', '--target', target, '--out-dir', outDir, '--release'];
  if (noOpt) flags.push('--no-opt');
  execFileSync('wasm-pack', flags, {
    cwd: here,
    stdio: 'inherit',
  });

  const pkgPath = join(here, outDir, 'package.json');
  const pkg = JSON.parse(readFileSync(pkgPath, 'utf8'));

  const missing = LICENSES.filter((f) => !existsSync(join(here, outDir, f)));
  if (missing.length) {
    console.error(`  licence file(s) missing from ${outDir}: ${missing.join(', ')}`);
    process.exit(1);
  }
  pkg.files = [...new Set([...(pkg.files ?? []), ...LICENSES])];
  pkg.repository = { type: 'git', url: `git+${REPOSITORY}.git` };
  pkg.homepage = REPOSITORY;
  pkg.name = name;
  pkg.description = `${pkg.description} (${blurb})`;
  pkg.keywords = ['rdf', 'sparql', 'shacl', 'ontology', 'wasm'];

  writeFileSync(pkgPath, `${JSON.stringify(pkg, null, 2)}\n`, 'utf8');
  console.log(`  patched ${outDir}/package.json: name=${name}, +licences, +repository`);
}
