// Proves that the differential tests of the git, sankey and packet ports can fail: breaks a port
// on purpose, one way at a time, and checks that the test of the broken part goes red.
//
//   mise exec -- node scripts/break-gitsankpack.mjs [test ...]
//
// Each break is one edit to a port. With test names, only the breaks those tests catch run, and
// only those tests are run clean at the end. The source is put back after each break, also when
// the run is interrupted. Exits 1 if a break goes unnoticed or a clean run fails.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');
const src = 'crates/pele-core/src';

const BREAKS = [
  {
    name: 'git front end: made-up ids from another multiplier',
    test: 'git_front',
    filter: 'random_graphs_match_pele',
    file: `${src}/diagrams/git/db.rs`,
    from: 'let hash = (salt as u64 as u32).wrapping_mul(0x9e37_79b1);',
    to: 'let hash = (salt as u64 as u32).wrapping_mul(0x9e37_79b3);',
  },
  {
    name: 'git front end: lanes sorted by order the wrong way round',
    test: 'git_front',
    filter: 'random_graphs_match_pele',
    file: `${src}/diagrams/git/db.rs`,
    from: 'lanes.sort_by(|a, b| by_difference(a.1 - b.1));',
    to: 'lanes.sort_by(|a, b| by_difference(b.1 - a.1));',
  },
  {
    name: 'git drawing: curves between lanes shorter',
    test: 'git_parity',
    filter: 'random_graphs_match_pele',
    file: `${src}/diagrams/git/render.rs`,
    from: 'const CURVE: f64 = 36.0;',
    to: 'const CURVE: f64 = 32.0;',
  },
  {
    name: 'git drawing: tags stacked a pixel closer',
    test: 'git_parity',
    filter: 'random_graphs_match_pele',
    file: `${src}/diagrams/git/render.rs`,
    from: 'tags_cross += tag.height + 3.0;',
    to: 'tags_cross += tag.height + 2.0;',
  },
  {
    name: 'sankey front end: values read without their exponent',
    test: 'sankey_front',
    filter: 'random_diagrams_match_pele',
    file: `${src}/diagrams/sankey/parser.rs`,
    from: 'if exponent > sign {\n            end = exponent;\n        }',
    to: 'if exponent > sign {\n            end = end.min(exponent);\n        }',
  },
  {
    name: 'sankey layout: one settling pass fewer',
    test: 'sankey_layout',
    filter: 'random_flows_are_laid_out_as_pele_lays_them_out',
    file: `${src}/diagrams/sankey/layout.rs`,
    from: 'const PASSES: u32 = 6;',
    to: 'const PASSES: u32 = 5;',
  },
  {
    name: 'sankey drawing: labels a pixel nearer their bars',
    test: 'sankey_parity',
    filter: 'random_diagrams_match_pele',
    file: `${src}/diagrams/sankey/render.rs`,
    from: 'const LABEL_GAP: f64 = 6.0;',
    to: 'const LABEL_GAP: f64 = 5.0;',
  },
  {
    name: 'sankey drawing: gradient ids from another hash',
    test: 'sankey_parity',
    filter: 'cases_match_pele',
    file: `${src}/svg.rs`,
    from: 'let (mut a, mut b) = (0x811c_9dc5_u32, 5381_u32);',
    to: 'let (mut a, mut b) = (0x811c_9dc5_u32, 5380_u32);',
  },
  {
    name: 'packet front end: the pieces of a split field a bit wider',
    test: 'packet_front',
    filter: 'random_diagrams_match_pele',
    file: `${src}/diagrams/packet.rs`,
    from: 'word.push(Block { start, end: row_end, bits: row_end - start, label: label.clone() });',
    to: 'word.push(Block { start, end: row_end, bits: row_end - start + 1.0, label: label.clone() });',
  },
  {
    name: 'packet drawing: bit numbers a pixel lower',
    test: 'packet_parity',
    filter: 'random_diagrams_match_pele',
    file: `${src}/diagrams/packet/render.rs`,
    from: 'let y = Number::num(field.y - 4.0);',
    to: 'let y = Number::num(field.y - 3.0);',
  },
];

function run(test, filter) {
  const args = ['test', '-q', '-p', 'pele-core', '--test', test];
  if (filter) args.push(filter);
  const result = spawnSync('cargo', args, { cwd: root, encoding: 'utf8' });
  return { passed: result.status === 0, output: result.stdout + result.stderr };
}

const chosen = process.argv.slice(2);
const breaks = chosen.length > 0 ? BREAKS.filter((item) => chosen.includes(item.test)) : BREAKS;
let unnoticed = 0;
for (const { name, test, filter, file, from, to } of breaks) {
  const path = join(root, file);
  const source = readFileSync(path, 'utf8');
  if (source.split(from).length !== 2) throw new Error(`${name}: ${file} no longer holds the line this break edits exactly once`);
  const restore = () => writeFileSync(path, source);
  process.on('SIGINT', restore);
  process.on('SIGTERM', restore);
  let result;
  try {
    writeFileSync(path, source.replace(from, to));
    result = run(test, filter);
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

for (const test of [...new Set(breaks.map((item) => item.test))]) {
  const clean = run(test);
  console.log(clean.passed ? `green  ${test} passes on the port as committed` : `${test} fails on the port as committed:\n${clean.output.slice(-800)}`);
  if (!clean.passed) unnoticed++;
}
process.exit(unnoticed > 0 ? 1 : 0);
