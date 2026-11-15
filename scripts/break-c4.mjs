// Proves that the differential tests of the C4 port can fail: breaks the port on purpose, one
// way at a time, and checks that the test of the broken part goes red.
//
//   mise exec -- node scripts/break-c4.mjs [test...]
//
// With test names, such as `c4_front c4_layout`, only the breaks those tests watch are made,
// and only those tests are run clean at the end, so the run can be split.
// Each break is one edit to the port. The source is put back after each, also when the run is
// interrupted, and every test must pass again at the end. Exits 1 if a break goes unnoticed.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');
const dir = 'crates/pele-core/src/diagrams/c4';

const BREAKS = [
  {
    name: 'front end: a short boundary list takes its kind in the label slot',
    // `splice(2, 0, kind)` puts the kind last in a list of one. Inserting it at 1 instead makes
    // `Enterprise_Boundary(b, "B")` take ENTERPRISE as its label.
    test: 'c4_front',
    file: `${dir}/parser.rs`,
    from: 'attrs.insert(attrs.len().min(2), Attr::Text(kind.into()));',
    to: 'attrs.insert(attrs.len().min(1), Attr::Text(kind.into()));',
  },
  {
    name: 'front end: a title keeps the space after the keyword',
    test: 'c4_front',
    file: `${dir}/parser.rs`,
    from: 'db.set_title(self.text(self.i).get(6..).unwrap_or_default().into());',
    to: 'db.set_title(self.text(self.i).get(5..).unwrap_or_default().into());',
  },
  {
    name: 'layout: rows of shapes one pixel closer',
    test: 'c4_layout',
    file: `${dir}/layout.rs`,
    from: 'const GAP_Y: f64 = 88.0;',
    to: 'const GAP_Y: f64 = 87.0;',
  },
  {
    name: 'routing: ports on a side one pixel closer together',
    test: 'c4_layout',
    file: `${dir}/route.rs`,
    from: 'const PORT_STEP: f64 = 12.0;',
    to: 'const PORT_STEP: f64 = 11.0;',
  },
  {
    name: 'drawing: a label tries the later spot along its line first',
    // A label slides along its line to the spot that covers least, the middle first, then 0.4
    // before 0.6. Trying 0.6 first moves a label wherever both cover the same.
    test: 'c4_parity',
    file: `${dir}/render.rs`,
    from: 'const SLIDE: [f64; 7] = [0.5, 0.4, 0.6, 0.3, 0.7, 0.2, 0.8];',
    to: 'const SLIDE: [f64; 7] = [0.5, 0.6, 0.4, 0.3, 0.7, 0.2, 0.8];',
  },
  {
    name: 'drawing: a crossed boundary heading tries the right before the middle',
    // A heading goes to the first of the left, the middle and the right of its top edge that no
    // line crosses. Trying the right second moves a heading the middle would have taken.
    test: 'c4_parity',
    file: `${dir}/render.rs`,
    from: 'for x in [HEAD_LEFT, (b.w - group.head_w) / 2.0, b.w - HEAD_LEFT - group.head_w] {',
    to: 'for x in [HEAD_LEFT, b.w - HEAD_LEFT - group.head_w, (b.w - group.head_w) / 2.0] {',
  },
];

function run(test) {
  const result = spawnSync('cargo', ['test', '-q', '-p', 'pele-core', '--test', test], { cwd: root, encoding: 'utf8' });
  return { passed: result.status === 0, output: result.stdout + result.stderr };
}

const chosen = process.argv.slice(2);
const tests = chosen.length > 0 ? chosen : ['c4_front', 'c4_layout', 'c4_parity'];
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
