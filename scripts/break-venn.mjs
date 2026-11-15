// Proves that the differential tests of the venn port can fail: breaks the port on purpose, one
// way at a time, and checks that the test of the broken part goes red.
//
//   mise exec -- node scripts/break-venn.mjs [test...]
//
// With test names, such as `venn_front venn_layout`, only the breaks those tests watch are made,
// and only those tests are run clean at the end, so the run can be split.
// Each break is one edit to the port. The source is put back after each, also when the run is
// interrupted, and every test must pass again at the end. Exits 1 if a break goes unnoticed.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');
const dir = 'crates/pele-core/src/diagrams/venn';

const BREAKS = [
  {
    name: 'front end: an overlap with no size gets 10 / n instead of 10 / n²',
    test: 'venn_front',
    file: `${dir}/parser.rs`,
    from: 'size: size.unwrap_or(10.0 / (count * count))',
    to: 'size: size.unwrap_or(10.0 / count)',
  },
  {
    name: 'front end: a label in brackets without quotes keeps its spaces',
    test: 'venn_front',
    file: `${dir}/lexer.rs`,
    from: 'self.text = src[i + 1..end].trim_js();',
    to: 'self.text = &src[i + 1..end];',
  },
  {
    name: 'layout: the search for a spot samples a coarser grid',
    test: 'venn_layout',
    file: `${dir}/layout.rs`,
    from: 'const GRID: i32 = 10;',
    to: 'const GRID: i32 = 9;',
  },
  {
    name: 'layout: the distance for an overlap stops one halving early',
    test: 'venn_layout',
    file: `${dir}/layout.rs`,
    from: 'for _ in 0..48 {',
    to: 'for _ in 0..47 {',
  },
  {
    name: 'drawing: the text of a region takes the ink of the first overlap painted over it',
    // Of the painted overlaps that hold all of a region, the one drawn last is under its text.
    test: 'venn_parity',
    file: `${dir}/render.rs`,
    from: 'for other in painted {',
    to: 'for other in painted.iter().rev() {',
  },
  {
    name: 'drawing: the scale grows at most by 1.5 to fit the text',
    test: 'venn_parity',
    file: `${dir}/render.rs`,
    from: 'const MAX_GROWTH: f64 = 1.6;',
    to: 'const MAX_GROWTH: f64 = 1.5;',
  },
  {
    name: 'drawing: an overlap of no style of its own is drawn as an outline',
    test: 'venn_parity',
    file: `${dir}/render.rs`,
    from: 'if region.sets.len() < 2 || region.style.shape.is_none() {',
    to: 'if region.sets.len() < 2 {',
  },
];

function run(test) {
  const result = spawnSync('cargo', ['test', '-q', '-p', 'pele-core', '--test', test], { cwd: root, encoding: 'utf8' });
  return { passed: result.status === 0, output: result.stdout + result.stderr };
}

const chosen = process.argv.slice(2);
const tests = chosen.length > 0 ? chosen : ['venn_front', 'venn_layout', 'venn_parity'];
let unnoticed = 0;
for (const { name, test, file, from, to } of BREAKS.filter((b) => tests.includes(b.test))) {
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
  const differ = [...result.output.matchAll(/^(\d+) (?:of (\d+) )?(.*) differ(?: from Pele)?:\n(.*)$/gm)];
  if (result.passed || differ.length === 0) {
    unnoticed++;
    console.log(`UNNOTICED  ${name}: ${test} ${result.passed ? 'passed' : 'failed some other way'}\n${result.output.slice(-800)}`);
    continue;
  }
  console.log(`red  ${name}`);
  for (const [, count, total, what, first] of differ) {
    console.log(`     ${test}: ${count}${total ? ` of ${total}` : ''} ${what} differ; first: ${first.trim().slice(0, 160)}`);
  }
}

for (const test of tests) {
  const clean = run(test);
  console.log(clean.passed ? `green  ${test} passes on the port as committed` : `${test} fails on the port as committed:\n${clean.output.slice(-800)}`);
  if (!clean.passed) unnoticed++;
}
process.exit(unnoticed > 0 ? 1 : 0);
