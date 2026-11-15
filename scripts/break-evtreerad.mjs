// Proves that the differential tests of the eventmodeling, treeview and radar ports can fail:
// breaks a port on purpose, one way at a time, and checks that the test of the broken part goes
// red.
//
//   mise exec -- node scripts/break-evtreerad.mjs [test...]
//
// With test names, such as `radar_front radar_parity`, only the breaks those tests watch are
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
    name: 'front end: an eventmodeling data block closes at the first brace line, not the last that can',
    // The optional group of EM_DATA_BLOCK is greedy, so a brace right after the opening line
    // closes the block only when no later line does.
    test: 'eventmodeling_front',
    file: `${dir}/eventmodeling/parser.rs`,
    from: '    let mut from = body;\n',
    to: '    if let Some(end) = closes(body) {\n        return Some(end);\n    }\n    let mut from = body;\n',
  },
  {
    name: 'drawing: down, a flow between neighbours runs 4 pixels nearer the lane heading',
    test: 'eventmodeling_parity',
    file: `${dir}/eventmodeling/render.rs`,
    from: 'BOX_PAD_X + lower.lane.width + 16.0',
    to: 'BOX_PAD_X + lower.lane.width + 12.0',
  },
  {
    name: 'front end: a box-drawing branch takes its depth rounded down',
    test: 'treeview_front',
    file: `${dir}/treeview/box_drawing.rs`,
    from: 'let depth = js::round(branch as f64 / segment as f64) as usize + 1;',
    to: 'let depth = (branch as f64 / segment as f64) as usize + 1;',
  },
  {
    name: 'drawing: a tight tree leaves 100 pixels for its names, not 120',
    test: 'treeview_parity',
    file: `${dir}/treeview/render.rs`,
    from: '(host - 120.0) / js::max(1.0, max_depth as f64)',
    to: '(host - 100.0) / js::max(1.0, max_depth as f64)',
  },
  {
    name: 'front end: a radar option value gives way to a longer name even right after its option',
    test: 'radar_front',
    file: `${dir}/radar/parser.rs`,
    from: 'if previous == Some(option) || !dashed_name(src, end) {',
    to: 'if !dashed_name(src, end) {',
  },
  {
    name: 'drawing: the second handle of a radar curve leans on the point before, not the one it leaves',
    test: 'radar_parity',
    file: `${dir}/radar/render.rs`,
    from: 'let second = [p2[0] - (p3[0] - p1[0]) * tension, p2[1] - (p3[1] - p1[1]) * tension];',
    to: 'let second = [p2[0] - (p3[0] - p0[0]) * tension, p2[1] - (p3[1] - p0[1]) * tension];',
  },
];

function run(test) {
  const result = spawnSync('cargo', ['test', '-q', '-p', 'pele-core', '--test', test], { cwd: root, encoding: 'utf8' });
  return { passed: result.status === 0, output: result.stdout + result.stderr };
}

const chosen = process.argv.slice(2);
const tests =
  chosen.length > 0
    ? chosen
    : ['eventmodeling_front', 'eventmodeling_parity', 'treeview_front', 'treeview_parity', 'radar_front', 'radar_parity'];
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
