// Collects every quadrant chart source in Pele's test suite into
// tests/fixtures/quadrant/harvest.json, so that crates/pele-core/tests/quadrant_front.rs and
// quadrant_parity.rs can put each one through Pele and through this port.
//
//   mise exec -- node scripts/harvest-quadrant.mjs
//
// `sources` are the string literals of vendor/pele/tests that hold `quadrantChart`, with each
// `${p}` filled in from Pele's hostile payload list; then the corpus of Pele's quadrant parser
// test (tests/quadrant/oracle.test.ts). `mutated` are the inputs Pele's quadrant tests derive from
// those: the mutations of tests/quadrant/oracle.test.ts and of
// tests/render/quadrant-security.test.ts. Rerunning at the same Pele pin writes the same file.
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
  const found = new RegExp(`const ${name}(?:: [^=]+)? = (\\[[\\s\\S]*?\\n\\]);`).exec(read(file));
  if (found === null) throw new Error(`${file} no longer declares ${name}`);
  return new Function('return ' + found[1])();
}

const PAYLOADS = constant('support/inert.ts', 'PAYLOADS');
const RE_LITERAL = /`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"/g;
const RE_TEMPLATE = /\$\{p\}/g;

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
    if (text === undefined || !text.includes('quadrantChart')) continue;
    if (RE_TEMPLATE.test(text)) {
      for (const payload of PAYLOADS) add(text.replace(RE_TEMPLATE, () => payload), from);
    } else if (!text.includes('${')) {
      add(text, from);
    }
  }
}

// tests/quadrant/oracle.test.ts: the spec and documentation corpus, with the seeds it adds.
const oracleTest = 'quadrant/oracle.test.ts';
const FRAGMENTS = constant(oracleTest, 'FRAGMENTS');
const seeds = /const corpus = \[\s*\.\.\.loadCorpus\('quadrant', \/quadrant\/i\),([\s\S]*?)\n\];/.exec(read(oracleTest));
if (seeds === null) throw new Error(`${oracleTest} no longer adds seeds to its corpus`);
process.chdir(pele);
const corpus = [...loadCorpus('quadrant', /quadrant/i), ...new Function('return [' + seeds[1] + ']')()];
for (const text of corpus) add(text, 'quadrant/oracle.test.ts corpus');

const mutated = new Set();
let illFormedMutations = 0;
function keep(text) {
  if (sources.has(text)) return;
  if (text.isWellFormed()) mutated.add(text);
  else illFormedMutations++;
}

function mutate(from, fragments, seed, count) {
  const next = mutator(from, fragments, random(seed));
  for (let i = 0; i < count; i++) {
    const src = next();
    if (src.length <= 4000) keep(src);
  }
}

// The mutations of tests/quadrant/oracle.test.ts (seed 7, 5000 inputs of at most 4000 characters).
mutate(corpus, FRAGMENTS, 7, 5000);

// The mutations of tests/render/quadrant-security.test.ts (seed 3, 4000 of at most 4000 characters).
{
  const security = /const fragments = (\[\.\.\.PAYLOADS, [^\n]*\]);/.exec(read('render/quadrant-security.test.ts'));
  if (security === null) throw new Error('render/quadrant-security.test.ts no longer mutates the quadrant corpus');
  mutate(loadCorpus('quadrant', /quadrant/i), new Function('PAYLOADS', 'return ' + security[1])(PAYLOADS), 3, 4000);
}

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/quadrant'), { recursive: true });
const out = join(root, 'tests/fixtures/quadrant/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated] }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources and ${mutated.size} mutated inputs written to ${relative(root, out)}`);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
