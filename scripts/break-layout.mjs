// Proves that crates/pele-core/tests/layout_replay.rs can fail: breaks the layout port on
// purpose, one way at a time, and checks that the replay of the recorded calls goes red.
//
//   mise exec -- node scripts/break-layout.mjs
//
// Each break is one edit to the port. The source is put back after each, also when the run is
// interrupted, and the replay must pass again at the end. Exits 1 if a break goes unnoticed.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');
const layered = 'crates/pele-core/src/layout/layered.rs';

const BREAKS = [
  {
    name: 'an unstable sort',
    // A sort that is not stable may return equal keys in any order. Rust's own `sort_unstable`
    // keeps short slices in order, so it would not show on graphs this small. Reversing the
    // rank first gives the other order an unstable sort is free to return.
    file: layered,
    from: '    order[s..e].sort_by(|&a, &b| by_difference(key(a) - key(b)));',
    to: '    order[s..e].reverse();\n    order[s..e].sort_by(|&a, &b| by_difference(key(a) - key(b)));',
  },
  {
    name: 'a reordered sum',
    // Pele adds a rank's height to the gap under it, and that to the running y. Adding the
    // height to y first rounds differently.
    file: layered,
    from: '        y += tallest + gap_under[r];',
    to: '        y = y + tallest + gap_under[r];',
  },
];

function replay() {
  const run = spawnSync('cargo', ['test', '-q', '-p', 'pele-core', '--test', 'layout_replay', 'recorded_calls_replay_bit_identically'], {
    cwd: root,
    encoding: 'utf8',
  });
  return { passed: run.status === 0, output: run.stdout + run.stderr };
}

let unnoticed = 0;
for (const { name, file, from, to } of BREAKS) {
  const path = join(root, file);
  const source = readFileSync(path, 'utf8');
  if (source.split(from).length !== 2) throw new Error(`${name}: ${file} no longer holds the line this break edits exactly once`);
  const restore = () => writeFileSync(path, source);
  process.on('SIGINT', restore);
  process.on('SIGTERM', restore);
  let result;
  try {
    writeFileSync(path, source.replace(from, to));
    result = replay();
  } finally {
    restore();
    process.off('SIGINT', restore);
    process.off('SIGTERM', restore);
  }
  const differ = /^.*calls differ from Pele:\n(.*)$/m.exec(result.output);
  const total = /^ *(\d+) +(\d+) +total$/m.exec(result.output);
  if (result.passed || !differ || !total) {
    unnoticed++;
    console.log(`UNNOTICED  ${name}: the replay ${result.passed ? 'passed' : 'failed some other way'}\n${result.output.slice(-800)}`);
    continue;
  }
  console.log(`red  ${name}: ${total[2]} of ${total[1]} recorded calls identical`);
  console.log(`     first finding: ${differ[1].trim()}`);
}

const clean = replay();
console.log(clean.passed ? 'green  the port as committed replays every recorded call' : `the port as committed fails:\n${clean.output.slice(-800)}`);
process.exit(unnoticed > 0 || !clean.passed ? 1 : 0);
