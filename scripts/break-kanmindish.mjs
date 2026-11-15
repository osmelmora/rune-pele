// Proves that the differential tests of the kanban, mindmap and ishikawa ports can fail: breaks a
// port on purpose, one way at a time, and checks that the test of the broken part goes red.
//
//   mise exec -- node scripts/break-kanmindish.mjs [test ...]
//
// Each break is one edit to the port. The source is put back after each, also when the run is
// interrupted, and every test must pass again at the end. Test names as arguments run only the
// breaks of those tests. Exits 1 if a break goes unnoticed.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');
const kanban = 'crates/pele-core/src/diagrams/kanban';
const mindmap = 'crates/pele-core/src/diagrams/mindmap';
const ishikawa = 'crates/pele-core/src/diagrams/ishikawa';

const BREAKS = [
  {
    name: 'kanban front end: a shape name with an underscore is let through',
    test: 'kanban_front',
    file: `${kanban}.rs`,
    from: "|| shape.contains(&u16::from(b'_'))",
    to: "|| shape.contains(&u16::from(b'#'))",
  },
  {
    name: 'kanban front end: metadata that is falsy still sets its field',
    test: 'kanban_front',
    file: `${kanban}.rs`,
    from: 'doc.get(key).filter(|value| value.truthy())',
    to: 'doc.get(key)',
  },
  {
    name: 'kanban drawing: cards a pixel further apart',
    test: 'kanban_parity',
    file: `${kanban}/render.rs`,
    from: 'const CARD_GAP: f64 = 8.0;',
    to: 'const CARD_GAP: f64 = 9.0;',
  },
  {
    name: 'kanban drawing: an underlined line written as plain text',
    test: 'kanban_parity',
    file: `${kanban}/render.rs`,
    from: '[run] if !run.style.underline && run.style.script == Script::Normal =>',
    to: '[run] if run.style.script == Script::Normal =>',
  },
  {
    name: 'kanban drawing: a word breaks when it only just fits',
    test: 'kanban_parity',
    file: `${kanban}/render.rs`,
    from: 'if width > 0.0 && width + w > max {',
    to: 'if width > 0.0 && width + w >= max {',
  },
  {
    name: 'mindmap front end: branch sections wrap after twelve',
    test: 'mindmap_front',
    file: `${mindmap}.rs`,
    from: 'const SECTIONS: usize = 11;',
    to: 'const SECTIONS: usize = 12;',
  },
  {
    name: 'mindmap front end: a node at its sibling\'s level becomes its child',
    test: 'mindmap_front',
    file: `${mindmap}.rs`,
    from: 'nodes[self.open[depth - 1]].level >= level',
    to: 'nodes[self.open[depth - 1]].level > level',
  },
  {
    name: 'mindmap drawing: subtrees with children a pixel closer',
    test: 'mindmap_parity',
    file: `${mindmap}/render.rs`,
    from: 'const GROUP_GAP: f64 = 18.0;',
    to: 'const GROUP_GAP: f64 = 17.0;',
  },
  {
    name: 'mindmap drawing: the trunk under the root starts a pixel further in',
    test: 'mindmap_parity',
    file: `${mindmap}/render.rs`,
    from: 'js::min(js::max(STEM, p.h[0] / 2.0), INDENT - BEND)',
    to: 'js::min(js::max(STEM, p.h[0] / 2.0), INDENT - BEND + 1.0)',
  },
  {
    name: 'ishikawa front end: a cause at the first cause\'s level nests under the one before',
    test: 'ishikawa_front',
    file: `${ishikawa}.rs`,
    from: '(raw_level as i64 - base as i64 + 1).max(1)',
    to: '(raw_level as i64 - base as i64 + 1).max(0)',
  },
  {
    name: 'ishikawa front end: indentation before a keyword names the wrong tokens',
    test: 'ishikawa_front',
    file: `${ishikawa}/parser.rs`,
    from: `if here == T::Spacelist { "'SPACELINE', 'NL', 'EOF', 'TEXT'" } else { STOP }`,
    to: 'STOP',
  },
  {
    name: 'ishikawa drawing: twigs a pixel further apart',
    test: 'ishikawa_parity',
    file: `${ishikawa}/render.rs`,
    from: 'const TWIG_STEP: f64 = 22.0;',
    to: 'const TWIG_STEP: f64 = 23.0;',
  },
  {
    name: 'ishikawa drawing: the indented list sets each level in a pixel more',
    test: 'ishikawa_parity',
    file: `${ishikawa}/render.rs`,
    from: 'js::min(20.0, js::max(2.0, (room - 120.0)',
    to: 'js::min(21.0, js::max(2.0, (room - 120.0)',
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
