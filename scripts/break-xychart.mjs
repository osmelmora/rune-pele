// Proves that the differential tests of the XY chart port can fail: breaks the port on purpose,
// one way at a time, and checks that the test of the broken part goes red.
//
//   mise exec -- node scripts/break-xychart.mjs [test …]
//
// Each break is one edit to the port. The source is put back after each, also when the run is
// interrupted, and every test must pass again at the end. Test names as arguments, such as
// `xychart_front xychart_ticks` and then `xychart_parity xychart_walk`, split the run. Exits 1 if a break goes
// unnoticed.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');
const dir = 'crates/pele-core/src/diagrams/xychart';

const BREAKS = [
  {
    name: 'front end: a keyword that runs on into a word',
    // `lines` is no LINE token in Pele: a keyword ends where a word does.
    test: 'xychart_front',
    file: `${dir}/parser.rs`,
    from: '108 if keyword(p, "line") => {',
    to: '108 if word_at(src, p, "line") => {',
  },
  {
    name: 'front end: an orientation read without regard to case',
    test: 'xychart_front',
    file: `${dir}/db.rs`,
    from: 'let horizontal = text.is_str("horizontal");',
    to: 'let horizontal = !text.is_str("vertical");',
  },
  {
    name: 'ticks: a step of ten only above 7.07, not from it',
    test: 'xychart_ticks',
    file: `${dir}/ticks.rs`,
    from: 'let factor = if unit >= 7.07 {',
    to: 'let factor = if unit > 7.07 {',
  },
  {
    name: 'drawing: wider gaps between grouped bars',
    test: 'xychart_parity',
    file: `${dir}/render.rs`,
    from: 'let gap = if bars > 1 { js::min(2.0, thickness * 0.2) } else { 0.0 };',
    to: 'let gap = if bars > 1 { js::min(2.0, thickness * 0.25) } else { 0.0 };',
  },
  {
    name: 'drawing: a line that comes back into range moves where it already is',
    test: 'xychart_parity',
    file: `${dir}/render.rs`,
    from: 'if !pen || t0 > 0.0 {',
    to: 'if !pen || t0 >= 0.0 {',
  },
  {
    name: 'drawing: a pixel more room above the plot for written values',
    test: 'xychart_parity',
    file: `${dir}/render.rs`,
    from: 'tiny_height + 4.0',
    to: 'tiny_height + 5.0',
  },
  {
    name: 'walk: a moved and turned group turns about the page origin',
    test: 'xychart_walk',
    file: 'crates/pele-core/src/scene/walk.rs',
    from: '(to, Some(Rotation { degrees: degrees.effective(), centre: to }))',
    to: '(to, Some(Rotation { degrees: degrees.effective(), centre: Point { x: 0.0, y: 0.0 } }))',
  },
];

function run(test) {
  const result = spawnSync('cargo', ['test', '-q', '-p', 'pele-core', '--test', test], { cwd: root, encoding: 'utf8' });
  return { passed: result.status === 0, output: result.stdout + result.stderr };
}

const chosen = process.argv.slice(2);
const tests = chosen.length > 0 ? chosen : ['xychart_front', 'xychart_ticks', 'xychart_parity', 'xychart_walk'];
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
  const differ = [...result.output.matchAll(/^(\d+) of (\d+) (.*) differ(?: from Pele)?:\n(.*)$/gm)];
  const asserted = /panicked at [^\n]*\n([^\n]*)/.exec(result.output);
  if (result.passed || result.output.includes('error[E') || (differ.length === 0 && asserted === null)) {
    unnoticed++;
    console.log(`UNNOTICED  ${name}: ${test} ${result.passed ? 'passed' : 'failed some other way'}\n${result.output.slice(-800)}`);
    continue;
  }
  console.log(`red  ${name}`);
  for (const [, count, total, what, first] of differ) {
    console.log(`     ${test}: ${count} of ${total} ${what} differ; first: ${first.trim().slice(0, 160)}`);
  }
  if (differ.length === 0) console.log(`     ${test}: ${asserted[1].trim().slice(0, 160)}`);
}

for (const test of tests) {
  const clean = run(test);
  console.log(clean.passed ? `green  ${test} passes on the port as committed` : `${test} fails on the port as committed:\n${clean.output.slice(-800)}`);
  if (!clean.passed) unnoticed++;
}
process.exit(unnoticed > 0 ? 1 : 0);
