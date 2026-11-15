// Proves that the differential tests of the gantt port can fail: breaks the port on purpose, one
// way at a time, and checks that the test of the broken part goes red.
//
//   mise exec -- node scripts/break-gantt.mjs
//
// Each break is one edit to the port. The source is put back after each, also when the run is
// interrupted, and every test must pass again at the end. Exits 1 if a break goes unnoticed.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');
const dir = 'crates/pele-core/src/diagrams/gantt';

const BREAKS = [
  {
    name: 'dates: 2000 not a leap year',
    // A year divisible by 400 is a leap year. Dropping that rule moves every day after
    // February 2000 and of every other such year by one.
    test: 'gantt_dates',
    file: `${dir}/date.rs`,
    from: 'if y % 4 != 0 || (y % 100 == 0 && y % 400 != 0) { common } else { leap }',
    to: 'if y % 4 != 0 || y % 100 == 0 { common } else { leap }',
  },
  {
    name: 'front end: a date format that keeps the space after its keyword',
    // `dateFormat X` is cut eleven units in. Cutting ten keeps the space in the model.
    test: 'gantt_front',
    filter: 'random_charts_match_pele',
    file: `${dir}/parser.rs`,
    from: 'db.model.date_format = text(&p, 11).into();',
    to: 'db.model.date_format = text(&p, 10).into();',
  },
  {
    name: 'front end: a Friday weekend that starts on Thursday',
    // `weekend friday` excludes Friday and Saturday. Starting a day early excludes Thursday and
    // Friday, which moves the end of every task that covers either.
    test: 'gantt_front',
    filter: 'random_charts_match_pele',
    file: `${dir}/db.rs`,
    from: 'Weekend::Friday => 5,',
    to: 'Weekend::Friday => 4,',
  },
  {
    name: 'drawing: a milestone a third of a bar wide on each side',
    test: 'gantt_parity',
    filter: 'random_charts_match_pele',
    file: `${dir}/render.rs`,
    from: 'x0 = mid - bar_height / 2.0;',
    to: 'x0 = mid - bar_height / 3.0;',
  },
  {
    name: 'drawing: axis labels let closer together',
    // A label is skipped when it starts within 8 pixels of the one before it.
    test: 'gantt_parity',
    filter: 'random_charts_match_pele',
    file: `${dir}/render.rs`,
    from: 'if tx - width / 2.0 < last_right + 8.0 || label == last_label {',
    to: 'if tx - width / 2.0 < last_right + 6.0 || label == last_label {',
  },
];

function run(test, filter) {
  const args = ['test', '-q', '-p', 'pele-core', '--test', test];
  if (filter) args.push(filter);
  const result = spawnSync('cargo', args, { cwd: root, encoding: 'utf8' });
  return { passed: result.status === 0, output: result.stdout + result.stderr };
}

let unnoticed = 0;
for (const { name, test, filter, file, from, to } of BREAKS) {
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

for (const test of ['gantt_dates', 'gantt_front', 'gantt_parity']) {
  const clean = run(test);
  console.log(clean.passed ? `green  ${test} passes on the port as committed` : `${test} fails on the port as committed:\n${clean.output.slice(-800)}`);
  if (!clean.passed) unnoticed++;
}
process.exit(unnoticed > 0 ? 1 : 0);
