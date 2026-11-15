// Proves that the differential tests of the railroad and info ports can fail: breaks a port on
// purpose, one way at a time, and checks that the test of the broken part goes red.
//
//   mise exec -- node scripts/break-railroad.mjs [test...]
//
// With test names, such as `railroad_front info_front`, only the breaks those tests watch are
// made, and only those tests are run clean at the end, so the run can be split.
// Each break is one edit to a port. The source is put back after each, also when the run is
// interrupted, and every test must pass again at the end. Exits 1 if a break goes unnoticed.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');
const dir = 'crates/pele-core/src/diagrams';

const BREAKS = [
  {
    name: 'front end: an EBNF exception puts what is excepted first',
    test: 'railroad_front',
    file: `${dir}/railroad/ebnf.rs`,
    from: 'grammar.add(super::Node::Sequence(vec![result, minus, except]))',
    to: 'grammar.add(super::Node::Sequence(vec![except, minus, result]))',
  },
  {
    name: 'front end: a string reads `\\r` as a line feed',
    // Mermaid's railroad converters know three escapes. Reading `\r` as `\n` changes only
    // terminals and titles that hold a `\r`.
    test: 'railroad_front',
    file: `${dir}/railroad/lexer.rs`,
    from: "Ok(b'r') => 13,",
    to: "Ok(b'r') => 10,",
  },
  {
    name: 'drawing: the widest branch of a choice runs in from its rail too',
    // A branch is inset, with a straight run from its rail, only when it is narrower than the
    // widest. Comparing with `>=` gives the widest branch a run of no length.
    test: 'railroad_parity',
    file: `${dir}/railroad/render.rs`,
    from: 'let inset = kx > rail_in + arc;',
    to: 'let inset = kx >= rail_in + arc;',
  },
  {
    name: 'drawing: a folded sequence turns back a third of the way down its gap',
    test: 'railroad_parity',
    file: `${dir}/railroad/render.rs`,
    from: 'let mid = y + offset + kid.down + js::max(sep, 2.0 * arc) / 2.0;',
    to: 'let mid = y + offset + kid.down + js::max(sep, 2.0 * arc) / 3.0;',
  },
  {
    name: 'front end: the front matter title wins over the info diagram\'s own',
    test: 'info_front',
    file: `${dir}/info.rs`,
    from: 'title: filled(ast.title).map(JsText::from).or_else(|| front_title.filter(|title| !title.is_empty()).cloned()),',
    to: 'title: front_title.filter(|title| !title.is_empty()).cloned().or_else(|| filled(ast.title).map(JsText::from)),',
  },
  {
    name: 'drawing: the info version sits a pixel lower',
    test: 'info_parity',
    file: `${dir}/info.rs`,
    from: 'let at = Point { x: width / 2.0, y: pad + title_height + text.height / 2.0 };',
    to: 'let at = Point { x: width / 2.0, y: pad + title_height + text.height / 2.0 + 1.0 };',
  },
];

function run(test) {
  const result = spawnSync('cargo', ['test', '-q', '-p', 'pele-core', '--test', test], { cwd: root, encoding: 'utf8' });
  return { passed: result.status === 0, output: result.stdout + result.stderr };
}

const chosen = process.argv.slice(2);
const tests = chosen.length > 0 ? chosen : ['railroad_front', 'railroad_parity', 'info_front', 'info_parity'];
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
