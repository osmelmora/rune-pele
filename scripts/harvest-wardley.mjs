// Collects every wardley map source in Pele's test suite into tests/fixtures/wardley/harvest.json,
// so that crates/pele-core/tests/wardley_front.rs and wardley_parity.rs can put each one through
// Pele and through this port.
//
//   mise exec -- node scripts/harvest-wardley.mjs
//
// `sources` are the string literals of vendor/pele/tests that have a line starting `wardley-beta`,
// each also with a line break after it, then the corpus, VALID and INVALID of Pele's wardley parser
// test (tests/wardley/oracle.test.ts), then each template of tests/render/wardley-security.test.ts
// with each of Pele's hostile payloads, and its extreme numbers. `mutated` are the inputs of the
// seeded mutation runs of those two tests. `worst` are the cases of tests/wardley/stress.test.ts,
// which Pele renders with no limit on the source's length. Rerunning at the same Pele pin writes
// the same file.
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { loadCorpus, mutator, random } from '../vendor/pele/tests/support/corpus.ts';

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

function read(file) {
  return readFileSync(join(tests, file), 'utf8');
}

function constant(file, name) {
  const found = new RegExp(`const ${name}(?::[^=]*)? = (\\[[\\s\\S]*?\\n\\]);`).exec(read(file));
  if (found === null) throw new Error(`${file} no longer declares ${name}`);
  return found[1];
}

const startsWardley = (text) => /(?:^|\n)\s*wardley-beta/.test(text);
const RE_LITERAL = /`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"/g;

const sources = new Set();
const origin = new Map();
let illFormed = 0;
function add(text, from) {
  if (typeof text !== 'string' || sources.has(text)) return;
  if (!text.isWellFormed()) {
    illFormed++;
    return;
  }
  sources.add(text);
  origin.set(from, (origin.get(from) ?? 0) + 1);
}

for (const file of files(tests)) {
  const from = relative(tests, file);
  for (const match of readFileSync(file, 'utf8').matchAll(RE_LITERAL)) {
    const text = value(match[1] ?? match[2] ?? match[3]);
    if (text === undefined || !startsWardley(text)) continue;
    add(text, from);
    add(text + '\n', from);
  }
}

// tests/wardley/oracle.test.ts: the documentation corpus, VALID and INVALID.
process.chdir(pele);
const FRAGMENTS = new Function('return ' + constant('wardley/oracle.test.ts', 'FRAGMENTS'))();
const VALID = new Function('return ' + constant('wardley/oracle.test.ts', 'VALID'))();
const INVALID = new Function('return ' + constant('wardley/oracle.test.ts', 'INVALID'))();
const docs = loadCorpus('wardley', /wardley-beta/);
const corpus = [...docs, ...VALID, ...INVALID];
for (const text of corpus) {
  add(text, 'wardley/oracle.test.ts corpus');
  add(text + '\n', 'wardley/oracle.test.ts corpus');
}

// tests/render/wardley-security.test.ts: hostile text in every place the grammar takes text, and
// extreme numbers.
const PAYLOADS = new Function('return ' + constant('support/inert.ts', 'PAYLOADS'))();
const security = read('render/wardley-security.test.ts');
const quoted = (p) => p.replace(/["\\\n]/g, '');
const single = (p) => p.replace(/['\\\n]/g, '');
const templates = /const TEMPLATES[^\n]*= (\[[\s\S]*?\n\]);/.exec(security);
if (templates === null) throw new Error('render/wardley-security.test.ts no longer lists its templates');
for (const template of new Function('quoted', 'single', 'return ' + templates[1])(quoted, single)) {
  for (const payload of PAYLOADS) add(template(payload), 'render/wardley-security.test.ts templates');
}
const extremes = /for \(const src of (\[[\s\S]*?\n\s*\])\) \{\n\s*expect\(check\(src\)/.exec(security);
if (extremes === null) throw new Error('render/wardley-security.test.ts no longer lists its extreme numbers');
for (const text of new Function('nines', 'return ' + extremes[1])('9'.repeat(400))) {
  add(text, 'render/wardley-security.test.ts extremes');
}

// The mutation runs of tests/wardley/oracle.test.ts (seed 7, 3000 inputs of at most 2000
// characters) and of tests/render/wardley-security.test.ts (seed 17, 2000 inputs).
const mutated = new Set();
let illFormedMutations = 0;
function mutate(base, fragments, seed, count, longest) {
  const next = mutator(base, fragments, random(seed));
  for (let i = 0; i < count; i++) {
    const text = next();
    if (text.length > longest || sources.has(text)) continue;
    if (text.isWellFormed()) mutated.add(text);
    else illFormedMutations++;
  }
}
mutate(corpus, FRAGMENTS, 7, 3000, 2000);
const hostile = /mutator\(corpus, (\[\.\.\.PAYLOADS, [^\n]*\]), random\(17\)\)/.exec(security);
if (hostile === null) throw new Error('render/wardley-security.test.ts no longer mutates the wardley corpus');
mutate(docs, new Function('PAYLOADS', 'return ' + hostile[1])(PAYLOADS), 17, 2000, 4000);

// tests/wardley/stress.test.ts: its cases, built as the test builds them.
const stress = read('wardley/stress.test.ts');
const cases = /const CASES: \[string, string\]\[\] = (\[[\s\S]*?\n\]);/.exec(stress);
if (cases === null) throw new Error('wardley/stress.test.ts no longer lists its cases');
const N = 50000;
const repeat = (count, line) => Array.from({ length: count }, (_, i) => line(i)).join('');
const frac = (i) => (0.001 * (i % 1000)).toFixed(3);
const worst = new Function('N', 'HEAD', 'repeat', 'frac', 'return ' + cases[1])(N, 'wardley-beta\n', repeat, frac).map(
  ([name, text]) => ({ name, source: text })
);

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/wardley'), { recursive: true });
const out = join(root, 'tests/fixtures/wardley/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated], worst }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources, ${mutated.size} mutated inputs and ${worst.length} worst cases written to ${relative(root, out)}`);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
