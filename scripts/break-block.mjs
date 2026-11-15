// Proves that the differential tests of the block diagram port can fail: breaks the port on
// purpose, one way at a time, and checks that the test of the broken part goes red.
//
//   mise exec -- node scripts/break-block.mjs
//
// Each break is one edit to the port. The source is put back after each, also when the run is
// interrupted, and every test must pass again at the end. Exits 1 if a break goes unnoticed.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');
const dir = 'crates/pele-core/src/diagrams/block';

const BREAKS = [
  {
    name: 'front end: a text style for a class without a colour',
    // Only a class declaration that names a colour gives its block's label a style. Testing for
    // a fill instead gives one to every filled class and loses it on a coloured one.
    test: 'block_front',
    file: `${dir}/db.rs`,
    from: 'if attribute.has("color") {',
    to: 'if attribute.has("fill") {',
  },
  {
    name: 'front end: a link of two dashes and no head',
    // A link without a head needs three dashes or equals signs. Taking two makes `--` a link
    // where Pele reads the start of a link label.
    test: 'block_front',
    file: `${dir}/lexer.rs`,
    from: '(k - q >= 3).then_some(k)',
    to: '(k - q >= 2).then_some(k)',
  },
  {
    name: 'layout: a span that does not count the gaps it covers',
    test: 'block_layout',
    file: `${dir}/layout.rs`,
    from: 'let width = (child.nw - gap * (span - 1.0)) / span;',
    to: 'let width = (child.nw - gap * span) / span;',
  },
  {
    name: 'drawing: block arrows with a shaft half a pixel wider',
    test: 'block_parity',
    file: `${dir}/arrows.rs`,
    from: 'let s = y - OVERHANG;',
    to: 'let s = y - OVERHANG + 0.5;',
  },
  {
    name: 'drawing: each further self-loop a pixel wider',
    test: 'block_parity',
    file: `${dir}/render.rs`,
    from: 'js::min(js::min(10.0 + 5.0 * (k - 1.0), hw), hh)',
    to: 'js::min(js::min(10.0 + 6.0 * (k - 1.0), hw), hh)',
  },
  {
    name: 'drawing: stacked edge labels a pixel further apart',
    test: 'block_parity',
    file: `${dir}/render.rs`,
    from: 'const LABEL_GAP: f64 = 4.0;',
    to: 'const LABEL_GAP: f64 = 5.0;',
  },
  {
    name: 'drawing: a self-loop sampled short of its end when it is tested against fills',
    test: 'block_parity',
    file: 'crates/pele-core/src/graph/edges.rs',
    from: 'let angle = start + (turn * f64::from(i)) / 8.0;',
    to: 'let angle = start + (turn * f64::from(i)) / 9.0;',
  },
];

function run(test) {
  const result = spawnSync('cargo', ['test', '-q', '-p', 'pele-core', '--test', test], { cwd: root, encoding: 'utf8' });
  return { passed: result.status === 0, output: result.stdout + result.stderr };
}

let unnoticed = 0;
for (const { name, test, file, from, to } of BREAKS) {
  const path = join(root, file);
  const source = readFileSync(path, 'utf8');
  if (source.split(from).length !== 2) throw new Error(`${name}: ${file} no longer holds the line this break edits exactly once`);
  const restore = () => writeFileSync(path, source);
  process.on('SIGINT', restore);
  process.on('SIGTERM', restore);
  let result;
  try {
    writeFileSync(path, source.replace(from, to));
    result = run(test);
  } finally {
    restore();
    process.off('SIGINT', restore);
    process.off('SIGTERM', restore);
  }
  const differ = [...result.output.matchAll(/^(\d+) of (\d+) (.*) differ(?: from Pele)?:\n(.*)$/gm)];
  if (result.passed || differ.length === 0) {
    unnoticed++;
    console.log(`UNNOTICED  ${name}: ${test} ${result.passed ? 'passed' : 'failed some other way'}\n${result.output.slice(-800)}`);
    continue;
  }
  console.log(`red  ${name}`);
  for (const [, count, total, what, first] of differ) {
    console.log(`     ${test}: ${count} of ${total} ${what} differ; first: ${first.trim().slice(0, 160)}`);
  }
}

for (const test of ['block_front', 'block_layout', 'block_parity']) {
  const clean = run(test);
  console.log(clean.passed ? `green  ${test} passes on the port as committed` : `${test} fails on the port as committed:\n${clean.output.slice(-800)}`);
  if (!clean.passed) unnoticed++;
}
process.exit(unnoticed > 0 ? 1 : 0);
