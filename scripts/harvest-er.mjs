// Collects every ER diagram source in Pele's test suite into tests/fixtures/er/harvest.json, so
// that crates/pele-core/tests/er_front.rs and er_parity.rs can put each one through Pele and
// through this port.
//
//   mise exec -- node scripts/harvest-er.mjs
//
// `sources` are the string literals of vendor/pele/tests that have a line starting `erDiagram`,
// with each `${p}` or `${payload}` filled in from Pele's hostile payload list as written and with
// its quotes as `#quot;`, then the corpus and seeds of Pele's ER parser test
// (tests/er/oracle.test.ts). `mutated` are the inputs Pele's ER tests derive from those: the two
// seeded mutation runs and the random token sequences. Rerunning at the same Pele pin writes the
// same file.
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
  const found = new RegExp(`const ${name} = (\\[[\\s\\S]*?\\n\\]);`).exec(read(file));
  if (found === null) throw new Error(`${file} no longer declares ${name}`);
  return found[1];
}

const startsEr = (text) => /(?:^|\n)\s*erDiagram/.test(text);

const PAYLOADS = new Function('return ' + constant('support/inert.ts', 'PAYLOADS'))();
const RE_LITERAL = /`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"/g;
const RE_TEMPLATE = /\$\{(?:p|payload)\}/g;

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
    if (text === undefined || !startsEr(text)) continue;
    add(text, from);
    if (RE_TEMPLATE.test(text)) {
      for (const payload of PAYLOADS) {
        add(text.replace(RE_TEMPLATE, () => payload), from);
        add(text.replace(RE_TEMPLATE, () => payload.replace(/"/g, '#quot;')), from);
      }
    }
  }
}

// tests/er/oracle.test.ts: the spec and documentation corpus, and the seeds that cover the
// constructs directly.
process.chdir(pele);
const oracleTest = read('er/oracle.test.ts');
const FRAGMENTS = new Function('return ' + constant('er/oracle.test.ts', 'FRAGMENTS'))();
const SEEDS = new Function('return ' + constant('er/oracle.test.ts', 'SEEDS'))();
const corpus = [...new Set([...loadCorpus('er', /erDiagram/), ...SEEDS])];
for (const text of corpus) add(text, 'er/oracle.test.ts corpus');

// The three generated runs of Pele's ER tests: the mutations of tests/er/oracle.test.ts (seed 7,
// 5000 inputs of at most 4000 characters), its random token sequences (seed 8, 5000), and the
// mutations of tests/render/er-security.test.ts (seed 3, 4000 inputs of at most 4000).
const mutated = new Set();
let illFormedMutations = 0;
function keep(text) {
  if (sources.has(text)) return;
  if (text.isWellFormed()) mutated.add(text);
  else illFormedMutations++;
}
function mutate(from, fragments, seed, count, longest) {
  const next = mutator(from, fragments, random(seed));
  for (let i = 0; i < count; i++) {
    const text = next();
    if (text.length <= longest) keep(text);
  }
}
mutate(corpus, FRAGMENTS, 7, 5000, 4000);

const sequences = /const openers = (\[[^\n]*\]);/.exec(oracleTest);
if (sequences === null) throw new Error('er/oracle.test.ts no longer strings random pieces together');
const openers = new Function('return ' + sequences[1])();
const rnd = random(7 + 1);
const pieces = [...new Set([...FRAGMENTS, ...FRAGMENTS.map((f) => f.trim()).filter((f) => f !== '')])];
const pick = () => pieces[Math.floor(rnd() * pieces.length)];
for (let i = 0; i < 5000; i++) {
  let text = openers[Math.floor(rnd() * openers.length)];
  for (let k = 1 + Math.floor(rnd() * 10); k > 0; k--) text += pick() + (rnd() < 0.3 ? ' ' : '');
  keep(text);
}

const security = /const fragments = (\[\.\.\.PAYLOADS, [^\n]*\]);/.exec(read('render/er-security.test.ts'));
if (security === null) throw new Error('render/er-security.test.ts no longer mutates the ER corpus');
const SECURITY = new Function('PAYLOADS', 'return ' + security[1])(PAYLOADS);
mutate(loadCorpus('er', /erDiagram/), SECURITY, 3, 4000, 4000);

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/er'), { recursive: true });
const out = join(root, 'tests/fixtures/er/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated] }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources and ${mutated.size} mutated inputs written to ${relative(root, out)}`);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
