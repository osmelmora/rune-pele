// Proves that the differential tests of the use case port can fail: breaks the port on purpose,
// one way at a time, and checks that the test of the broken part goes red.
//
//   mise exec -- node scripts/break-usecase.mjs
//
// Each break is one edit to the port. The source is put back after each, also when the run is
// interrupted, and both tests must pass again at the end. Exits 1 if a break goes unnoticed.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');
const dir = 'crates/pele-core/src/diagrams/usecase';

const BREAKS = [
  {
    name: 'front end: a relationship one rank too long',
    // Each dash beyond the second asks for one more rank. Counting from the first dash makes
    // `--->` two ranks longer than `-->` instead of one.
    test: 'usecase_front',
    file: `${dir}/parser.rs`,
    from: 'dashes.saturating_sub(1).max(1)',
    to: 'dashes.max(1)',
  },
  {
    name: 'front end: a JSON fault one column to the left',
    // V8 counts columns from 1. Counting from 0 moves the column in its message, which is part
    // of Pele's message.
    test: 'usecase_front',
    file: `${dir}/json.rs`,
    from: 'let column = 1 + end - last_break;',
    to: 'let column = end - last_break;',
  },
  {
    name: 'front end: an element moved into a later boundary',
    // The first boundary an element is declared in keeps it. Letting a later one take it changes
    // the model's parent and the boundaries' members.
    test: 'usecase_front',
    file: `${dir}/db.rs`,
    from: 'existing.parent = existing.parent.or(draft.parent);',
    to: 'existing.parent = draft.parent.or(existing.parent);',
  },
  {
    name: 'drawing: an edge that meets an oval at its box',
    // An edge off the middle of an oval's side stops at the oval. Without the square root it
    // stops at the box around the oval.
    test: 'usecase_parity',
    file: `${dir}/render.rs`,
    from: 'reach *= js::max(1.0 - off * off, 0.0).sqrt();',
    to: 'reach *= js::max(1.0 - off * off, 0.0);',
  },
  {
    name: 'drawing: an actor one pixel taller',
    test: 'usecase_parity',
    file: `${dir}/render.rs`,
    from: 'const FIGURE_GAP: f64 = 6.0;',
    to: 'const FIGURE_GAP: f64 = 7.0;',
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
  const differ = [...result.output.matchAll(/^(\d+) of (\d+) (.*) differ from Pele:\n(.*)$/gm)];
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

for (const test of ['usecase_front', 'usecase_parity']) {
  const clean = run(test);
  console.log(clean.passed ? `green  ${test} passes on the port as committed` : `${test} fails on the port as committed:\n${clean.output.slice(-800)}`);
  if (!clean.passed) unnoticed++;
}
process.exit(unnoticed > 0 ? 1 : 0);
