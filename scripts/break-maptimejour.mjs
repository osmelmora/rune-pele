// Proves that the differential tests of the treemap, timeline and journey ports can fail: breaks
// a port on purpose, one way at a time, and checks that the test of the broken part goes red.
//
//   mise exec -- node scripts/break-maptimejour.mjs [test ...]
//
// Each break is one edit to the port or to a Scene form it added. The source is put back after
// each, also when the run is interrupted, and every test must pass again at the end. Test names
// as arguments run only the breaks of those tests. Exits 1 if a break goes unnoticed.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');
const src = 'crates/pele-core/src';
const treemap = `${src}/diagrams/treemap`;
const timeline = `${src}/diagrams/timeline`;
const journey = `${src}/diagrams/journey`;

const BREAKS = [
  {
    name: 'treemap front end: an escaped comma in a class splits its declaration',
    test: 'treemap_front',
    file: `${treemap}.rs`,
    from: 'if text[at] == 92 && text.at(at + 1) == Some(44) {',
    to: 'if false {',
  },
  {
    name: 'treemap front end: a value of a lone point reads as zero',
    test: 'treemap_front',
    file: `${treemap}/parser.rs`,
    from: 'if whole == 0 && fraction <= 1 {',
    to: 'if whole == 0 && fraction < 1 {',
  },
  {
    name: 'treemap layout: a row of boxes grows while it gets no squarer',
    test: 'treemap_layout',
    file: `${treemap}/layout.rs`,
    from: 'if ratio > worst {',
    to: 'if ratio >= worst {',
  },
  {
    name: 'treemap format: a tie rounds toward zero',
    test: 'treemap_layout',
    file: `${treemap}/format.rs`,
    from: "if digits[first + count] >= b'5' {",
    to: "if digits[first + count] > b'5' {",
  },
  {
    name: 'treemap drawing: a tooltip only where the path names a section',
    test: 'treemap_parity',
    file: `${treemap}/render.rs`,
    from: "drawn.as_deref() == Some(name.as_slice()) && full == own",
    to: 'full == own',
  },
  {
    name: 'treemap drawing: corners capped only for boxes under twelve pixels',
    test: 'treemap_parity',
    file: `${treemap}/render.rs`,
    from: 'if cap < 4.0 {',
    to: 'if cap < 3.0 {',
  },
  {
    name: 'Scene: a decoded tooltip has its entity codes read again',
    test: 'treemap_parity',
    file: `${src}/scene.rs`,
    from: 'Some(if self.tooltip_decoded { tooltip.to_vec() } else { decode_entities(tooltip) })',
    to: 'Some(decode_entities(tooltip))',
  },
  {
    name: 'timeline front end: an event ends at any colon',
    test: 'timeline_front',
    file: `${timeline}/parser.rs`,
    from: 'while e < n && !(src[e] == 10 || (src[e] == 58 && space_at(e + 1))) {',
    to: 'while e < n && !(src[e] == 10 || src[e] == 58) {',
  },
  {
    name: 'timeline front end: a colon and a space with nothing after them is an event',
    test: 'timeline_front',
    file: `${timeline}/parser.rs`,
    from: 'if e > p + 2 {',
    to: 'if e >= p + 2 {',
  },
  {
    name: 'timeline drawing: a section header sits a pixel further from its periods',
    test: 'timeline_parity',
    file: `${timeline}/render.rs`,
    from: 'const SECTION_GAP: f64 = 8.0;',
    to: 'const SECTION_GAP: f64 = 9.0;',
  },
  {
    name: "timeline drawing: text on a chip ignores the palette's text colour",
    test: 'timeline_parity',
    file: `${timeline}/render.rs`,
    from: 'Some(slot) if cx.palette.ink(slot).is_some() => Paint::OnSeries(slot),',
    to: 'Some(slot) if false => Paint::OnSeries(slot),',
  },
  {
    name: "journey front end: an actor's name keeps its spaces",
    test: 'journey_front',
    file: `${journey}.rs`,
    from: 'names.split(|&c| c == 44).map(|name| name.trim_js().into()).collect()',
    to: 'names.split(|&c| c == 44).map(|name| name.into()).collect()',
  },
  {
    name: 'journey front end: a task name may stand without its data',
    test: 'journey_front',
    file: `${journey}/parser.rs`,
    from: 'if tokens[i].kind != Kind::TaskData {',
    to: 'if tokens[i].kind == Kind::Colon {',
  },
  {
    name: "journey drawing: actors' dots a pixel closer",
    test: 'journey_parity',
    file: `${journey}/render.rs`,
    from: 'const DOT_STEP: f64 = 13.0;',
    to: 'const DOT_STEP: f64 = 12.0;',
  },
  {
    name: 'journey drawing: a score of three is happy',
    test: 'journey_parity',
    file: `${journey}/render.rs`,
    from: 'if score > 3.0 {',
    to: 'if score >= 3.0 {',
  },
  {
    name: "Scene: a shape's tooltip written with a space before it",
    test: 'journey_parity',
    file: `${src}/svg.rs`,
    from: 'out.push_str("><title>");',
    to: 'out.push_str("><title> ");',
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
