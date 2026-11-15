// Collects every XY chart source in Pele's test suite into tests/fixtures/xychart/harvest.json, so
// that crates/pele-core/tests/xychart_front.rs and xychart_parity.rs can put each one through Pele
// and through this port.
//
//   mise exec -- node scripts/harvest-xychart.mjs
//
// `sources` are the string literals of vendor/pele/tests that have a line starting `xychart`, each
// also with a line break after it, then the corpus of Pele's XY chart parser test
// (tests/xychart/oracle.test.ts), then each template of tests/render/xychart-security.test.ts with
// each of Pele's hostile payloads, and its extreme numbers. `mutated` are the inputs of the seeded
// mutation runs of those two tests. `worst` are the sources of tests/xychart/stress.test.ts, which
// Pele parses or renders with no limit on the source's length.
//
// The parser test reads the text that the grammar's broken markdown rule matches back from the
// lexer Jison generates, and holds Pele's lexer to that lexer. Jison is not installed here, so the
// text is taken from Pele's lexer, `BROKEN_RULE` in src/diagrams/xychart/lexer.ts. Rerunning at the
// same Pele pin writes the same file.
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
  const found = new RegExp(`const ${name}(?::[^=]*)? = (\\[[\\s\\S]*?\\n\\s*\\]);`).exec(read(file));
  if (found === null) throw new Error(`${file} no longer declares ${name}`);
  return found[1];
}

const startsXychart = (text) => /(?:^|\n)\s*xychart/i.test(text);
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
    if (text === undefined || !startsXychart(text)) continue;
    add(text, from);
    add(text + '\n', from);
  }
}

const lexer = readFileSync(join(pele, 'src/diagrams/xychart/lexer.ts'), 'utf8');
const broken = /const BROKEN_RULE =\n([\s\S]*?);\n/.exec(lexer);
if (broken === null) throw new Error('src/diagrams/xychart/lexer.ts no longer declares BROKEN_RULE');
const BROKEN = new Function('return ' + broken[1])();

// tests/xychart/oracle.test.ts: the documentation corpus and the charts written for the test.
process.chdir(pele);
const FRAGMENTS = new Function('BROKEN', 'return ' + constant('xychart/oracle.test.ts', 'FRAGMENTS'))(BROKEN);
const corpus = new Function('loadCorpus', 'BROKEN', 'return ' + constant('xychart/oracle.test.ts', 'corpus'))(loadCorpus, BROKEN);
const docs = loadCorpus('xychart', /xychart/i);
for (const text of corpus) {
  add(text, 'xychart/oracle.test.ts corpus');
  add(text + '\n', 'xychart/oracle.test.ts corpus');
}

// tests/render/xychart-security.test.ts: hostile text in every place the grammar takes text, and
// extreme numbers.
const PAYLOADS = new Function('return ' + constant('support/inert.ts', 'PAYLOADS'))();
const security = read('render/xychart-security.test.ts');
const templates = /const TEMPLATES[^\n]*= (\[[\s\S]*?\n\]);/.exec(security);
if (templates === null) throw new Error('render/xychart-security.test.ts no longer lists its templates');
for (const template of new Function('return ' + templates[1])()) {
  for (const payload of PAYLOADS) add(template(payload), 'render/xychart-security.test.ts templates');
}
const charts = /const charts = (\[[\s\S]*?\n\s*\]);/.exec(security);
if (charts === null) throw new Error('render/xychart-security.test.ts no longer lists its extreme charts');
for (const text of new Function('huge', 'return ' + charts[1])('9'.repeat(400))) add(text, 'render/xychart-security.test.ts extremes');
const sizes = /for \(const value of (\[[^\n]*\])\) \{\n\s*const src = (`[^`]*`);/.exec(security);
if (sizes === null) throw new Error('render/xychart-security.test.ts no longer lists its extreme sizes');
for (const size of new Function('return ' + sizes[1])()) {
  add(new Function('value', 'return ' + sizes[2])(size), 'render/xychart-security.test.ts extremes');
}

// The mutation runs of tests/xychart/oracle.test.ts (seed 7, 5000 inputs) and of
// tests/render/xychart-security.test.ts (seed 3, 4000 inputs), each of at most 4000 characters.
const mutated = new Set();
let illFormedMutations = 0;
function mutate(base, fragments, seed, count) {
  const next = mutator(base, fragments, random(seed));
  for (let i = 0; i < count; i++) {
    const text = next();
    if (text.length > 4000 || sources.has(text)) continue;
    if (text.isWellFormed()) mutated.add(text);
    else illFormedMutations++;
  }
}
mutate(corpus, FRAGMENTS, 7, 5000);
const hostile = /const fragments = (\[\.\.\.PAYLOADS, [^\n]*\]);/.exec(security);
if (hostile === null) throw new Error('render/xychart-security.test.ts no longer mutates the XY chart corpus');
mutate(docs, new Function('PAYLOADS', 'return ' + hostile[1])(PAYLOADS), 3, 4000);

// tests/xychart/stress.test.ts: the source each case hands the lexer, the parser or the renderer.
const stress = read('xychart/stress.test.ts');
const cases = /const CASES: \[string, \(\) => unknown\]\[\] = (\[[\s\S]*?\n\]);/.exec(stress);
if (cases === null) throw new Error('xychart/stress.test.ts no longer lists its cases');
const repeat = (count, item, join = '') => Array.from({ length: count }, (_, i) => item(i)).join(join);
const withLabels = '---\nconfig:\n  xyChart:\n    showDataLabel: true\n---\n';
const given = (text) => text;
const worst = new Function('N', 'big', 'repeat', 'withLabels', 'tokenize', 'parse', 'render', 'return ' + cases[1])(
  50000, {}, repeat, withLabels, given, given, given
).map(([name, run]) => ({ name, source: run() }));

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/xychart'), { recursive: true });
const out = join(root, 'tests/fixtures/xychart/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated], worst }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources, ${mutated.size} mutated inputs and ${worst.length} worst cases written to ${relative(root, out)}`);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
