// Proves that the differential tests of the wardley port can fail: breaks the port on purpose,
// one way at a time, and checks that the test of the broken part goes red.
//
//   mise exec -- node scripts/break-wardley.mjs [test …]
//
// Each break is one edit to the port. The source is put back after each, also when the run is
// interrupted, and every test must pass again at the end. Test names as arguments, such as
// `wardley_front` and then `wardley_parity wardley_walk`, split the run. Exits 1 if a break goes
// unnoticed.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');
const dir = 'crates/pele-core/src/diagrams/wardley';

const BREAKS = [
  {
    name: 'front end: a 0 that starts a run of digits read as the whole run',
    // INT's first alternative is a lone 0, so `05` is two tokens in Pele.
    test: 'wardley_front',
    file: `${dir}/parser.rs`,
    from: '48 => Some(at + 1),',
    to: '48 => Some(src.run(at, is_digit)),',
  },
  {
    name: 'front end: 1 taken as one percent instead of a whole',
    test: 'wardley_front',
    file: `${dir}/model.rs`,
    from: 'let normalized = if value <= 1.0 { value * 100.0 } else { value };',
    to: 'let normalized = if value < 1.0 { value * 100.0 } else { value };',
  },
  {
    name: 'front end: a link to a label goes to the last node with it',
    test: 'wardley_front',
    file: `${dir}/model.rs`,
    from: 'labels.entry(node.label.clone()).or_insert_with(|| id.clone());',
    to: 'labels.insert(node.label.clone(), id.clone());',
  },
  {
    name: 'drawing: an evolve arrow that stops a pixel short',
    test: 'wardley_parity',
    file: `${dir}/render.rs`,
    from: 'let ex = tx - ux * (R + 2.0);',
    to: 'let ex = tx - ux * (R + 3.0);',
  },
  {
    name: 'drawing: a blocked label tries up and left before down and right',
    test: 'wardley_parity',
    file: `${dir}/render.rs`,
    from: '[1, 2, 4, 8]',
    to: '[1, 4, 2, 8]',
  },
  {
    name: 'drawing: text from its start grows the bounds a pixel more',
    test: 'wardley_parity',
    file: `${dir}/render.rs`,
    from: 'label.height + 4.0);',
    to: 'label.height + 5.0);',
  },
  {
    name: 'walk: a turned text element drawn level',
    test: 'wardley_walk',
    file: 'crates/pele-core/src/scene/walk.rs',
    from: 'let rotate = Some(rotate);',
    to: 'let rotate = Some(rotate).filter(|_| false);',
  },
];

function run(test) {
  const result = spawnSync('cargo', ['test', '-q', '-p', 'pele-core', '--test', test], { cwd: root, encoding: 'utf8' });
  return { passed: result.status === 0, output: result.stdout + result.stderr };
}

const chosen = process.argv.slice(2);
const tests = chosen.length > 0 ? chosen : ['wardley_front', 'wardley_parity', 'wardley_walk'];
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
