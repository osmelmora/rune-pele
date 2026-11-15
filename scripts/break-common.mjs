// Proves that the differential tests of the shared helpers can fail: breaks a helper on purpose,
// one way at a time, and checks that its test in tests/common.rs goes red.
//
//   mise exec -- node scripts/break-common.mjs
//
// Each break is one edit to the port. The source is put back after each, also when the run is
// interrupted, and the tests must pass again at the end. Exits 1 if a break goes unnoticed.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');
const src = 'crates/pele-core/src';

const BREAKS = [
  {
    name: 'scan: keywordAt matches one case only',
    // Pele folds each code unit by setting bit 5, so `Journey` is the keyword `journey`.
    test: 'scan_helpers_match_pele',
    file: `${src}/diagrams/scan.rs`,
    from: 'src.at(at + i).map_or(32, |c| c | 32) == w',
    to: 'src.at(at + i).map_or(32, |c| c) == w',
  },
  {
    name: 'tokens: a longer alternative wins a tie',
    // Chevrotain takes the alternative only when it matches more text, so `axis` stays a keyword.
    test: 'longer_alternatives_lex_as_pele_does',
    file: `${src}/diagrams/tokens.rs`,
    from: '.filter(|&longer| longer > end)',
    to: '.filter(|&longer| longer >= end)',
  },
  {
    name: 'labels: literalLabel keeps the spaces a line breaks at',
    test: 'literal_labels_match_pele',
    file: `${src}/measure/fitting.rs`,
    from: 'lines.push(if wraps && !trimmed.is_empty() { trimmed.to_vec() } else { line });',
    to: 'lines.push(line);',
  },
  {
    name: 'labels: fitText shrinks two pixels at a time',
    test: 'fit_text_matches_pele',
    file: `${src}/measure/fitting.rs`,
    from: 'fitted -= 1.0;',
    to: 'fitted -= 2.0;',
  },
  {
    name: 'drawing: leftLabelSvg wraps a run its element already styles in a tspan',
    test: 'left_labels_write_as_pele_writes_them',
    file: `${src}/svg.rs`,
    from: 'if own_emphasis(run.style, *shared).is_plain() {',
    to: 'if run.style.is_plain() {',
  },
  {
    name: 'palette: venn repeats an empty colour',
    // Pele leaves out undefined, null and the empty string before it repeats the list.
    test: 'palette_matches_pele',
    file: `${src}/scene.rs`,
    from: 'Some(JsValue::String(text)) => !text.is_empty(),',
    to: 'Some(JsValue::String(_)) => true,',
  },
  {
    name: 'outline: `(` with any closer is a rounded rectangle',
    // getType makes `(` a rounded rectangle only before `)`, and a cloud before anything else.
    test: 'outlines_tokenize_and_parse_as_pele_does',
    file: `${src}/diagrams/outline.rs`,
    from: '"(" if end.is_str(")") => NodeShape::RoundedRect,',
    to: '"(" => NodeShape::RoundedRect,',
  },
];

function run(filter) {
  const args = ['test', '-q', '-p', 'pele-core', '--test', 'common', '--', '--nocapture', ...(filter ? ['--exact', filter] : [])];
  const result = spawnSync('cargo', args, { cwd: root, encoding: 'utf8' });
  return { passed: result.status === 0, output: result.stdout + result.stderr };
}

let unnoticed = 0;
for (const { name, test, file, from, to } of BREAKS) {
  const path = join(root, file);
  const source = readFileSync(path, 'utf8');
  if (source.split(from).length !== 2) throw new Error(`${name}: ${file} no longer holds the text this break edits exactly once`);
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
  const differ = /^(.*): (\d+) of (\d+) differ:$/m.exec(result.output);
  if (result.passed || differ === null) {
    unnoticed++;
    console.log(`UNNOTICED  ${name}: ${test} ${result.passed ? 'passed' : 'failed some other way'}\n${result.output.slice(-800)}`);
    continue;
  }
  console.log(`red  ${name}: ${differ[2]} of ${differ[3]} ${differ[1].replace(/^.*panicked at [^\n]*\n/, '')} differ`);
}

const clean = run();
console.log(clean.passed ? 'green  tests/common.rs passes on the port as committed' : `tests/common.rs fails on the port as committed:\n${clean.output.slice(-800)}`);
if (!clean.passed) unnoticed++;
process.exit(unnoticed > 0 ? 1 : 0);
