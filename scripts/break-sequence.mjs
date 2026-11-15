// Proves that the differential tests of the sequence port can fail: breaks the port on purpose,
// one way at a time, and checks that the test of the broken part goes red.
//
//   mise exec -- node scripts/break-sequence.mjs
//
// Each break is one edit to the port. The source is put back after each, also when the run is
// interrupted, and both tests must pass again at the end. Exits 1 if a break goes unnoticed.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');
const dir = 'crates/pele-core/src/diagrams/sequence';

const BREAKS = [
  {
    name: 'front end: a dotted arrow lexed as a solid one',
    test: 'sequence_front',
    file: `${dir}/lexer.rs`,
    from: '(62, 62) => self.emit(T::DottedArrow, p, p + 4),',
    to: '(62, 62) => self.emit(T::SolidArrow, p, p + 4),',
  },
  {
    name: 'front end: numbers that step by two where no step is given',
    test: 'sequence_front',
    file: `${dir}/parser.rs`,
    from: 'step = Some(1.0);',
    to: 'step = Some(2.0);',
  },
  {
    name: 'front end: a deactivation that closes no activation',
    // A participant can be deactivated only as often as it was activated.
    test: 'sequence_front',
    file: `${dir}/db.rs`,
    from: 'self.active.insert(from.clone().expect("an active participant"), count - 1);',
    to: 'self.active.insert(from.clone().expect("an active participant"), count);',
  },
  {
    name: 'drawing: an activation one pixel taller at the least',
    test: 'sequence_parity',
    file: `${dir}/render.rs`,
    from: 'const ACT_MIN: f64 = 14.0;',
    to: 'const ACT_MIN: f64 = 15.0;',
  },
  {
    name: "drawing: a figure's label one pixel lower",
    test: 'sequence_parity',
    file: `${dir}/actors.rs`,
    from: 'const GLYPH_GAP: f64 = 4.0;',
    to: 'const GLYPH_GAP: f64 = 5.0;',
  },
  {
    name: 'drawing: a half head that is not turned over on the way back of a loop',
    test: 'sequence_parity',
    file: `${dir}/render.rs`,
    from: 'end = st.at_end.then(|| head(x1, bottom, -1.0, -up));',
    to: 'end = st.at_end.then(|| head(x1, bottom, -1.0, up));',
  },
  {
    name: 'drawing: gaps widened by what each is short of, with no allowance for rounding',
    test: 'sequence_parity',
    file: `${dir}/render.rs`,
    from: '(short * (spare - 0.01)) / missing',
    to: '(short * spare) / missing',
  },
  {
    name: 'drawing: an uneven widening kept when it wraps no less',
    test: 'sequence_parity',
    file: `${dir}/render.rs`,
    from: 'uneven = sum(&counts) < sum(&first)',
    to: 'uneven = sum(&counts) <= sum(&first)',
  },
  {
    name: 'drawing: a second draw that holds back all the spare width at once',
    // The first retry holds back only what the widened diagram is over by.
    test: 'sequence_parity',
    file: `${dir}/render.rs`,
    from: 'hold = if hold > 0.0 { f64::INFINITY } else { attempt.width - max_width.floor() };',
    to: 'hold = f64::INFINITY;',
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

for (const test of ['sequence_front', 'sequence_parity']) {
  const clean = run(test);
  console.log(clean.passed ? `green  ${test} passes on the port as committed` : `${test} fails on the port as committed:\n${clean.output.slice(-800)}`);
  if (!clean.passed) unnoticed++;
}
process.exit(unnoticed > 0 ? 1 : 0);
