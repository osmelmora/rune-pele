// Proves that the differential tests of the quadrant chart port can fail: breaks the port on
// purpose, one way at a time, and checks that the test of the broken part goes red.
//
//   mise exec -- node scripts/break-quadrant.mjs [test...]
//
// With test names, such as `quadrant_front`, only the breaks those tests watch are made,
// and only those tests are run clean at the end, so the run can be split.
// Each break is one edit to the port. The source is put back after each, also when the run is
// interrupted, and every test must pass again at the end. Exits 1 if a break goes unnoticed.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');
const dir = 'crates/pele-core/src/diagrams/quadrant';

const BREAKS = [
  {
    name: 'front end: a coordinate takes the character after its 0 only when it is a dot',
    // Mermaid's `0(.\d+)?` has a dot that is any character, so `0x5` is one coordinate.
    test: 'quadrant_front',
    file: `${dir}/lexer.rs`,
    from: 'if e < self.src.len() && !self.is(e, is_line_end) && self.is(e + 1, is_digit) {',
    to: 'if e < self.src.len() && self.c(e) == 46 && self.is(e + 1, is_digit) {',
  },
  {
    name: 'front end: an axis arrow with nothing after it adds an ASCII arrow',
    test: 'quadrant_front',
    file: `${dir}/parser.rs`,
    from: 'from.text.push_str(" \\u{27f6} ");',
    to: 'from.text.push_str(" -> ");',
  },
  {
    name: 'drawing: the x axis goes below the plot only when the config says so',
    // Mermaid moves the x axis below the plot as soon as there are points.
    test: 'quadrant_parity',
    file: `${dir}/render.rs`,
    from: 'let x_below = has_points || settings.is("xAxisPosition", "bottom");',
    to: 'let x_below = settings.is("xAxisPosition", "bottom");',
  },
  {
    name: 'drawing: a point label tries above its point before below',
    test: 'quadrant_parity',
    file: `${dir}/render.rs`,
    from: `                [cx, cy + away + half_h],
                [cx, cy - away - half_h],`,
    to: `                [cx, cy - away - half_h],
                [cx, cy + away + half_h],`,
  },
  {
    name: 'drawing: a chart of exactly 300 points skips the search for free places',
    test: 'quadrant_parity',
    file: `${dir}/render.rs`,
    from: 'const CAREFUL: usize = 300;',
    to: 'const CAREFUL: usize = 299;',
  },
  {
    name: 'drawing: each new point is drawn over the ones before it',
    test: 'quadrant_parity',
    file: `${dir}/render.rs`,
    from: 'in circles.iter().enumerate().rev() {',
    to: 'in circles.iter().enumerate() {',
  },
];

function run(test) {
  const result = spawnSync('cargo', ['test', '-q', '-p', 'pele-core', '--test', test], { cwd: root, encoding: 'utf8' });
  return { passed: result.status === 0, output: result.stdout + result.stderr };
}

const chosen = process.argv.slice(2);
const tests = chosen.length > 0 ? chosen : ['quadrant_front', 'quadrant_parity'];
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
