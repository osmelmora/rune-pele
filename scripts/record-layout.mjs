// Records the layout calls Pele makes while it renders, into tests/fixtures/layout/recorded.json,
// so that crates/pele-core/tests/layout_replay.rs can replay each one through the Rust port.
//
//   mise exec -- node scripts/record-layout.mjs
//
// The sources are every sample of vendor/pele/tests/corpus, every string literal of
// vendor/pele/tests with each `${payload}` filled in from Pele's hostile payload list, and a few
// larger graphs built here. Each is rendered by the oracle's `layout_record` op, which uses the
// parity options, once as it is and once per entry of WIDTHS, where a narrow host makes Pele
// turn a diagram and set it tighter. A call is kept once per distinct input. Rerunning at the
// same Pele pin writes the same file.
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';

const root = join(import.meta.dirname, '..');
const tests = join(root, 'vendor/pele/tests');
const WIDTHS = [320];

function files(dir, extension) {
  return readdirSync(dir, { withFileTypes: true })
    .sort((a, b) => (a.name < b.name ? -1 : 1))
    .flatMap((entry) => (entry.isDirectory() ? files(join(dir, entry.name), extension) : entry.name.endsWith(extension) ? [join(dir, entry.name)] : []));
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

const sources = new Set();
function add(text) {
  // A diagram needs a keyword and something after it, and a lone surrogate cannot cross into Rust.
  if (typeof text === 'string' && /\S\s+\S/.test(text) && text.isWellFormed()) sources.add(text);
}
for (const file of files(join(tests, 'corpus'), '.json')) {
  const samples = JSON.parse(readFileSync(file, 'utf8'));
  if (Array.isArray(samples)) samples.forEach(add);
}
for (const file of files(tests, '.ts')) {
  for (const match of readFileSync(file, 'utf8').matchAll(RE_LITERAL)) {
    const text = value(match[1] ?? match[2] ?? match[3]);
    if (text === undefined) continue;
    if (text.includes('${payload}')) for (const payload of PAYLOADS) add(text.replaceAll('${payload}', payload));
    else add(text);
  }
}

// The corpus and the tests hold no graph of more than 16 nodes. These follow the shapes of
// vendor/pele/tests/stress.test.ts, which builds its sources in code, at sizes a person might write.
const repeat = (count, item) => Array.from({ length: count }, (_, i) => item(i)).join('');
const complete = (count) => repeat(count, (i) => repeat(count - i - 1, (j) => `n${i}-->n${i + j + 1}\n`));
for (const head of ['graph TD\n', 'flowchart LR\n']) {
  add(head + repeat(60, (i) => `n${i}-->n${i + 1}\n`));
  add(head + repeat(40, (i) => `c-->n${i}\n`));
  add(head + repeat(30, (i) => `n${i}-->n${i + 1}\nn0-->n${i + 2}\n`));
  add(head + complete(10));
  add(head + repeat(8, (i) => `subgraph g${i}\n`) + 'inner\n' + 'end\n'.repeat(8) + repeat(12, (i) => `out${i % 5} --> inner\n`));
  add(head + repeat(16, (i) => `subgraph s${i} [Group ${i}]\na${i}-->|label ${i}|b${i}\nend\nb${i}-->a${(i * 7 + 3) % 16}\n`));
  add(head + 'A-->B\n'.repeat(8) + 'A-->A\n'.repeat(3) + 'B-->|back|A\n');
  add(head + repeat(12, (i) => `n${i} ------------> n${i + 1}\nn${i} --> m${i % 4}\n`));
}
add('%%{init: {"flowchart": {"curve": "step"}}}%%\ngraph TD\n' + complete(8) + repeat(20, (i) => `n${i % 8}-->|step ${i}|m${i % 7}\n`));
add('%%{init: {"flowchart": {"curve": "linear"}}}%%\ngraph LR\n' + complete(8) + repeat(20, (i) => `n${i % 8}-->|line ${i}|m${i % 7}\n`));
add('stateDiagram-v2\n[*] --> s0\n' + repeat(24, (i) => `s${i} --> s${(i * 5 + 1) % 24} : event ${i}\n`) + 'state big {\n' + repeat(8, (i) => `t${i} --> t${i + 1}\n`) + '}\ns5 --> big\nbig --> s9\n');
add('classDiagram\n' + repeat(24, (i) => `class C${i} {\n+int field${i}\n+method${i}()\n}\nC${(i * 5 + 2) % 24} <|-- C${i}\nC${i} "1" --> "*" C${(i * 11 + 7) % 24} : uses\n`));
add('erDiagram\n' + repeat(20, (i) => `E${i} ||--o{ E${(i * 7 + 1) % 20} : has\nE${i} {\nint id\nstring name${i}\n}\n`));

// One request per line, and Node ends a line at U+2028 and U+2029 as well.
const line = (text) => JSON.stringify(text).replaceAll(String.fromCharCode(0x2028), '\\u2028').replaceAll(String.fromCharCode(0x2029), '\\u2029');
const requests = [];
for (const source of sources) {
  requests.push(`layout_record\t${line(source)}\t{}`);
  for (const maxWidth of WIDTHS) requests.push(`layout_record\t${line(source)}\t${JSON.stringify({ maxWidth })}`);
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
const made = new Map();
for (const answer of answers) {
  const { type, calls } = JSON.parse(answer);
  for (const call of calls) {
    const key = `${type} ${call.fn}`;
    made.set(key, (made.get(key) ?? 0) + 1);
    const input = `${call.fn} ${JSON.stringify(call.in)}`;
    if (!kept.has(input)) kept.set(input, { type, ...call });
  }
}

const pin = execFileSync('git', ['-C', join(root, 'vendor/pele'), 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
const out = join(root, 'tests/fixtures/layout/recorded.json');
mkdirSync(dirname(out), { recursive: true });
const calls = [...kept.values()];
writeFileSync(out, `{"pele":${JSON.stringify(pin)},"calls":[\n${calls.map((call) => JSON.stringify(call)).join(',\n')}\n]}\n`);

const unique = new Map();
for (const call of calls) unique.set(`${call.type} ${call.fn}`, (unique.get(`${call.type} ${call.fn}`) ?? 0) + 1);
console.log(`${sources.size} sources, each rendered ${1 + WIDTHS.length} times`);
console.log('   made   kept  type and function');
for (const key of [...made.keys()].sort()) console.log(`${String(made.get(key)).padStart(7)}${String(unique.get(key) ?? 0).padStart(7)}  ${key}`);
console.log(`${calls.length} calls with distinct inputs written to ${relative(root, out)}`);
