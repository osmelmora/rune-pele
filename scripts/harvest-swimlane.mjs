// Collects every swimlane source in Pele's test suite into tests/fixtures/swimlane/harvest.json, and
// records every lane layout call Pele makes while it renders them into
// tests/fixtures/swimlane/lanes.json, so that crates/pele-core/tests/swimlane_front.rs,
// swimlane_parity.rs and lanes_replay.rs can run each one through Pele and through this port.
//
//   mise exec -- node scripts/harvest-swimlane.mjs
//
// `sources` are the string literals of vendor/pele/tests that start with `swimlane-beta`, with each
// `${p}` or `${payload}` filled in from Pele's hostile payload list, and Pele's documentation corpus.
// `converted` are the sources of tests/fixtures/flowchart/harvest.json with their `graph` or
// `flowchart` header made `swimlane-beta`: a swimlane is a flowchart with lanes, and Pele's tests
// hold many more flowcharts. `mutated` are made from `sources` by Pele's seeded mutator with the
// fragments of its flowchart mutation run, since Pele runs none of its own for swimlanes.
// Each source is rendered once as written and once at maxWidth 320, where a narrow host makes Pele
// set the lanes tighter, and each lane layout call is kept once per distinct input. Rerunning at
// the same Pele pin writes the same files.
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { mutator, random } from '../vendor/pele/tests/support/corpus.ts';

const root = join(import.meta.dirname, '..');
const pele = join(root, 'vendor/pele');
const tests = join(pele, 'tests');

function files(dir) {
  return readdirSync(dir, { withFileTypes: true })
    .sort((a, b) => (a.name < b.name ? -1 : 1))
    .flatMap((entry) =>
      entry.isDirectory() ? files(join(dir, entry.name)) : /\.[jt]s$/.test(entry.name) ? [join(dir, entry.name)] : []
    );
}

// A string literal's value, as Pele's loadCorpus reads it: a `${...}` stays as written.
function value(raw) {
  try {
    return new Function('return `' + raw.replace(/`/g, '\\`').replace(/\$\{/g, '\\${') + '`')();
  } catch {
    return undefined;
  }
}

function constant(file, name) {
  const text = readFileSync(join(tests, file), 'utf8');
  const found = new RegExp(`const ${name} = (\\[[\\s\\S]*?\\n\\]);`).exec(text);
  if (found === null) throw new Error(`${file} no longer declares ${name}`);
  return found[1];
}

const PAYLOADS = new Function('return ' + constant('support/inert.ts', 'PAYLOADS'))();
const RE_LITERAL = /`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"/g;
const RE_TEMPLATE = /\$\{(?:p|payload)\}/g;
const startsSwimlane = (text) => /(?:^|\n)\s*swimlane-beta/.test(text);

const sources = new Set();
const origin = new Map();
let illFormed = 0;
function add(set, text, from) {
  if (typeof text !== 'string' || sources.has(text) || set.has(text)) return;
  if (!text.isWellFormed()) {
    illFormed++;
    return;
  }
  set.add(text);
  origin.set(from, (origin.get(from) ?? 0) + 1);
}

for (const file of files(tests)) {
  const from = relative(tests, file);
  for (const match of readFileSync(file, 'utf8').matchAll(RE_LITERAL)) {
    const text = value(match[1] ?? match[2] ?? match[3]);
    if (text === undefined || !startsSwimlane(text)) continue;
    add(sources, text, from);
    if (RE_TEMPLATE.test(text)) {
      for (const payload of PAYLOADS) add(sources, text.replace(RE_TEMPLATE, () => payload), from);
    }
  }
}
for (const text of JSON.parse(readFileSync(join(tests, 'corpus/swimlane-docs.json'), 'utf8'))) {
  add(sources, text, 'corpus/swimlane-docs.json');
}

const flowchart = JSON.parse(readFileSync(join(root, 'tests/fixtures/flowchart/harvest.json'), 'utf8'));
const converted = new Set();
for (const text of flowchart.sources) {
  add(converted, text.replace(/(^|\n)(\s*)(?:graph|flowchart)(?=\s|$)/, '$1$2swimlane-beta'), 'flowchart harvest');
}

const FRAGMENTS = new Function('return ' + constant('flowchart/oracle.test.ts', 'FRAGMENTS'))();
const mutated = new Set();
const next = mutator([...sources], [...FRAGMENTS, 'subgraph L\n', 'end\n', 'swimlane-beta '], random(7));
for (let i = 0; i < 4000; i++) {
  const text = next();
  if (text.length <= 4000) add(mutated, text, 'mutated');
}

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
const dir = join(root, 'tests/fixtures/swimlane');
mkdirSync(dir, { recursive: true });
const harvest = { pele: pin, sources: [...sources], converted: [...converted], mutated: [...mutated] };
writeFileSync(join(dir, 'harvest.json'), JSON.stringify(harvest, null, 1) + '\n');

// One request per line, and Node ends a line at U+2028 and U+2029 as well.
const line = (text) => JSON.stringify(text).replaceAll('\u2028', '\\u2028').replaceAll('\u2029', '\\u2029');
const requests = [];
for (const text of [...sources, ...converted, ...mutated]) {
  requests.push(`lane_record\t${line(text)}\t{}`);
  requests.push(`lane_record\t${line(text)}\t${JSON.stringify({ maxWidth: 320 })}`);
}
const oracle = spawnSync('node', [join(root, 'harness/dist/oracle.mjs')], {
  input: requests.join('\n') + '\n',
  encoding: 'utf8',
  env: { ...process.env, TZ: 'UTC' },
  maxBuffer: 1 << 30,
});
if (oracle.status !== 0) throw new Error(`the oracle failed: ${oracle.stderr}`);
const answers = oracle.stdout.split('\n').filter((answer) => answer !== '');
if (answers.length !== requests.length) throw new Error(`${answers.length} answers to ${requests.length} requests`);
const kept = new Map();
let made = 0;
for (const answer of answers) {
  for (const call of JSON.parse(answer).calls) {
    made++;
    const input = JSON.stringify(call.in);
    if (!kept.has(input)) kept.set(input, call);
  }
}
const calls = [...kept.values()];
writeFileSync(join(dir, 'lanes.json'), `{"pele":${JSON.stringify(pin)},"calls":[\n${calls.map((call) => JSON.stringify(call)).join(',\n')}\n]}\n`);

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources, ${converted.size} converted and ${mutated.size} mutated inputs written to ${relative(root, dir)}/harvest.json`);
console.log(`${illFormed} inputs left out: a lone surrogate cannot cross into Rust as a string`);
console.log(`${made} lane layout calls made, ${calls.length} with distinct inputs written to ${relative(root, dir)}/lanes.json`);
