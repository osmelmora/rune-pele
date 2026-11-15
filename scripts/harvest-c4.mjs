// Collects every C4 diagram source in Pele's test suite into tests/fixtures/c4/harvest.json, so
// that crates/pele-core/tests/c4_front.rs and c4_parity.rs can put each one through Pele and
// through this port.
//
//   mise exec -- node scripts/harvest-c4.mjs
//
// `sources` are the string literals of vendor/pele/tests that have a line starting with a C4
// header, with each `${p}` or `${payload}` filled in from Pele's hostile payload list as written
// and with its quotes as `#quot;`, and each `${TWO}` as tests/render/c4-security.test.ts defines
// it; then the corpus of Pele's C4 parser test (tests/c4/oracle.test.ts). `mutated` are the inputs
// Pele's C4 tests derive from those: the generated documents and the mutations of
// tests/c4/oracle.test.ts, and the mutations of tests/render/c4-security.test.ts. Rerunning at the
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
  const found = new RegExp(`const ${name} = (\\[[\\s\\S]*?\\n\\]);`).exec(read(file));
  if (found === null) throw new Error(`${file} no longer declares ${name}`);
  return found[1];
}

const HEADER = /C4(?:Context|Container|Component|Dynamic|Deployment)/;
const startsC4 = (text) => /(?:^|\n)\s*C4(?:Context|Container|Component|Dynamic|Deployment)/.test(text);

const PAYLOADS = new Function('return ' + constant('support/inert.ts', 'PAYLOADS'))();
const RE_LITERAL = /`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"/g;
const RE_TEMPLATE = /\$\{(?:p|payload)\}/g;

const two = /const TWO = '([^']*)';/.exec(read('render/c4-security.test.ts'));
if (two === null) throw new Error('render/c4-security.test.ts no longer declares TWO');
const TWO = value(two[1]);

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
    const found = value(match[1] ?? match[2] ?? match[3]);
    if (found === undefined) continue;
    const text = found.replaceAll('${TWO}', TWO);
    if (!startsC4(text)) continue;
    add(text, from);
    if (RE_TEMPLATE.test(text)) {
      for (const payload of PAYLOADS) {
        add(text.replace(RE_TEMPLATE, () => payload), from);
        add(text.replace(RE_TEMPLATE, () => payload.replace(/"/g, '#quot;')), from);
      }
    }
  }
}

// tests/c4/oracle.test.ts: the spec and documentation corpus, each also with a line break after it.
process.chdir(pele);
const corpus = loadCorpus('c4', HEADER);
for (const text of corpus) {
  add(text, 'c4/oracle.test.ts corpus');
  add(text + '\n', 'c4/oracle.test.ts corpus');
}

const mutated = new Set();
let illFormedMutations = 0;
function keep(text) {
  if (sources.has(text)) return;
  if (text.isWellFormed()) mutated.add(text);
  else illFormedMutations++;
}

// The generated documents of tests/c4/oracle.test.ts (seed 7, 5000 of them).
const oracleTest = 'c4/oracle.test.ts';
const STATEMENTS = new Function('return ' + constant(oracleTest, 'STATEMENTS'))();
const separators = /const SEPARATORS = (\[[^\n]*\]);/.exec(read(oracleTest));
const headers = /const HEADERS = (\[[^\n]*\]);/.exec(read(oracleTest));
if (separators === null || headers === null) throw new Error(`${oracleTest} no longer builds documents from statements`);
const SEPARATORS = new Function('return ' + separators[1])();
const HEADERS = new Function('return ' + headers[1])();
{
  const rnd = random(7);
  const pick = (list) => list[Math.floor(rnd() * list.length)];
  for (let i = 0; i < 5000; i++) {
    let src = pick(HEADERS) + pick(SEPARATORS);
    const count = 1 + Math.floor(rnd() * 8);
    for (let k = 0; k < count; k++) src += pick(STATEMENTS) + pick(SEPARATORS);
    keep(src);
  }
}

// The mutations of tests/c4/oracle.test.ts (seed 7, 5000 inputs of at most 4000 characters) and of
// tests/render/c4-security.test.ts (seed 3, 4000 of at most 4000).
function mutate(fragments, seed, count, longest) {
  const next = mutator(corpus, fragments, random(seed));
  for (let i = 0; i < count; i++) {
    const text = next();
    if (text.length <= longest) keep(text);
  }
}
mutate(new Function('return ' + constant(oracleTest, 'FRAGMENTS'))(), 7, 5000, 4000);
const security = /mutator\(\s*corpus,\s*(\[\.\.\.PAYLOADS, [^\n]*\]),\s*random\(3\)/.exec(read('render/c4-security.test.ts'));
if (security === null) throw new Error('render/c4-security.test.ts no longer mutates the C4 corpus');
mutate(new Function('PAYLOADS', 'return ' + security[1])(PAYLOADS), 3, 4000, 4000);

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/c4'), { recursive: true });
const out = join(root, 'tests/fixtures/c4/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated] }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources and ${mutated.size} mutated inputs written to ${relative(root, out)}`);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
