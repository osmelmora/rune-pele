// Proves that the differential tests of the architecture and cynefin ports can fail: breaks a
// port on purpose, one way at a time, and checks that the test of the broken part goes red.
//
//   mise exec -- node scripts/break-archcyn.mjs [test ...]
//
// Each break is one edit to the port. The source is put back after each, also when the run is
// interrupted, and every test must pass again at the end. Test names as arguments run only the
// breaks of those tests. Exits 1 if a break goes unnoticed.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');
const arch = 'crates/pele-core/src/diagrams/architecture';
const cyn = 'crates/pele-core/src/diagrams/cynefin';

const BREAKS = [
  {
    name: 'architecture front end: a quoted title keeps the spaces inside its quotes',
    test: 'architecture_front',
    file: `${arch}/parser.rs`,
    from: '    unescaped.trim_js().to_vec()',
    to: '    unescaped',
  },
  {
    name: 'architecture front end: an alignment may list a member twice',
    test: 'architecture_front',
    file: `${arch}/db.rs`,
    from: '            if !seen.insert(node) {',
    to: '            if !seen.insert(node) && false {',
  },
  {
    name: 'architecture layout: a node slid aside goes the far way on a tie',
    test: 'architecture_layout',
    file: `${arch}/layout.rs`,
    from: 'let near = if forth <= back { forth } else { -back };',
    to: 'let near = if forth < back { forth } else { -back };',
  },
  {
    name: 'architecture drawing: route corners rounded a pixel less',
    test: 'architecture_parity',
    file: `${arch}/render.rs`,
    from: 'const CORNER: f64 = 6.0;',
    to: 'const CORNER: f64 = 5.0;',
  },
  {
    name: 'architecture drawing: a group widened for its title by one pixel more',
    test: 'architecture_parity',
    file: `${arch}/render.rs`,
    from: 'right += (head_w[g] + 20.0 - inner).ceil();',
    to: 'right += (head_w[g] + 21.0 - inner).ceil();',
  },
  {
    name: 'architecture drawing: no built-in glyph for a disk',
    test: 'architecture_parity',
    file: 'crates/pele-core/src/scene/glyph.rs',
    from: '"disk" => IconGlyph::Disk,',
    to: '"disk " => IconGlyph::Disk,',
  },
  {
    name: 'cynefin front end: an empty transition label is kept',
    test: 'cynefin_front',
    file: `${cyn}.rs`,
    from: 'label: label.filter(|text| !text.is_empty()).map(JsText::from)',
    to: 'label: label.map(JsText::from)',
  },
  {
    name: 'cynefin front end: a later block for a domain adds to the earlier one',
    test: 'cynefin_front',
    file: `${cyn}.rs`,
    from: 'Some(block) => block.1 = items,',
    to: 'Some(block) => block.1.extend(items),',
  },
  {
    name: 'cynefin drawing: the seeded wave mixes its bits one place over',
    test: 'cynefin_parity',
    file: `${cyn}/boundaries.rs`,
    from: '((t as u32) >> 15) as i32',
    to: '((t as u32) >> 16) as i32',
  },
  {
    name: 'cynefin drawing: a repeated transition bows a pixel further',
    test: 'cynefin_parity',
    file: `${cyn}/render.rs`,
    from: '+ repeat * 56.0;',
    to: '+ repeat * 57.0;',
  },
];

function run(test) {
  const result = spawnSync('cargo', ['test', '-q', '-p', 'pele-core', '--test', test], { cwd: root, encoding: 'utf8' });
  return { passed: result.status === 0, output: result.stdout + result.stderr };
}

const only = process.argv.slice(2);
const chosen = BREAKS.filter(({ test }) => only.length === 0 || only.includes(test));
let unnoticed = 0;
for (const { name, test, file, from, to } of chosen) {
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

for (const test of [...new Set(chosen.map(({ test }) => test))]) {
  const clean = run(test);
  console.log(clean.passed ? `green  ${test} passes on the port as committed` : `${test} fails on the port as committed:\n${clean.output.slice(-800)}`);
  if (!clean.passed) unnoticed++;
}
process.exit(unnoticed > 0 ? 1 : 0);
