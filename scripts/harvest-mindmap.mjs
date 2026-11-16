// Collects every mindmap source in Pele's test suite into tests/fixtures/mindmap/harvest.json, so
// that crates/pele-core/tests/mindmap_front.rs and mindmap_parity.rs can put each one through Pele
// and through this port.
//
//   mise exec -- node scripts/harvest-mindmap.mjs
//
// `sources` are the string literals of vendor/pele/tests that have a line starting `mindmap`, each
// also with a line break after it, then the corpus of Pele's mindmap parser test, then each
// template of tests/render/mindmap-security.test.ts with each of Pele's hostile payloads.
// `mutated` are the inputs of the seeded mutation runs of those two tests. `worst` are the cases
// of tests/mindmap/stress.test.ts, which Pele renders with no limit on the source's length or on
// the output's. Rerunning at the same Pele pin writes the same file.
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
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

// The code of a test file from one line to the next, with its types stripped, run to give `names`.
function evaluate(file, from, to, names, args = {}) {
  const text = read(file);
  const start = text.indexOf(from);
  const end = text.indexOf(to, start);
  if (start < 0 || end < 0) throw new Error(`${file} no longer holds ${from} … ${to}`);
  const code = stripTypeScriptTypes(text.slice(start, end));
  return new Function(...Object.keys(args), `${code}\nreturn { ${names.join(', ')} };`)(...Object.values(args));
}

const startsMindmap = (text) => /(?:^|\n)\s*mindmap/.test(text);
const RE_LITERAL = /`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"/g;
const PAYLOADS = new Function('return ' + constant('support/inert.ts', 'PAYLOADS'))();

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
    if (text === undefined || !startsMindmap(text) || text.includes('${')) continue;
    add(text, from);
    add(text + '\n', from);
  }
}

// tests/mindmap/oracle.test.ts: the spec and documentation corpus.
process.chdir(pele);
const oracleTest = 'mindmap/oracle.test.ts';
const corpus = loadCorpus('mindmap', /mindmap/);
for (const text of corpus) add(text, `${oracleTest} corpus`);

// tests/render/mindmap-security.test.ts: each template with each payload.
const security = 'render/mindmap-security.test.ts';
const { TEMPLATES } = evaluate(security, 'const TEMPLATES', "describe('inert mindmap output'", ['TEMPLATES']);
for (const template of TEMPLATES) for (const payload of PAYLOADS) add(template(payload), `${security} templates`);

// The mutation runs of tests/mindmap/oracle.test.ts (seed 7, 5000 inputs) and of
// tests/render/mindmap-security.test.ts (seed 3, 4000 inputs), each of at most 4000 characters.
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
mutate(corpus, new Function('return ' + constant(oracleTest, 'FRAGMENTS'))(), 7, 5000);
const hostile = /const fragments = (\[\.\.\.PAYLOADS, [^\n]*\]);/.exec(read(security));
if (hostile === null) throw new Error(`${security} no longer mutates the mindmap corpus`);
mutate(corpus, new Function('PAYLOADS', 'return ' + hostile[1])(PAYLOADS), 3, 4000);

// tests/mindmap/stress.test.ts: the source each case hands the lexer, the parser or the renderer.
const given = (source) => ({ source });
const { CASES } = evaluate('mindmap/stress.test.ts', 'const N = ', "describe('mindmap worst cases'", ['CASES'], {
  metricsMeasurer: undefined,
  tokenize: given,
  parse: given,
  render: given,
});
const worst = CASES.map(([name, run]) => ({ name, ...run() })).filter((item) => item.source.isWellFormed());

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/mindmap'), { recursive: true });
const out = join(root, 'tests/fixtures/mindmap/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated], worst }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources, ${mutated.size} mutated inputs and ${worst.length} worst cases written to ${relative(root, out)}`);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
