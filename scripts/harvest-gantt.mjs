// Collects every gantt source in Pele's test suite into tests/fixtures/gantt/harvest.json, so that
// crates/pele-core/tests/gantt_front.rs and gantt_parity.rs can run each one through Pele and
// through this port.
//
//   mise exec -- node scripts/harvest-gantt.mjs
//
// `sources` are the string literals of vendor/pele/tests that hold `gantt`, with each `${p}` or
// `${payload}` filled in from Pele's hostile payload list, the corpus Pele's gantt tests load, and
// the hostile templates of tests/render/gantt-security.test.ts applied to every payload. `mutated`
// are the inputs Pele's gantt tests derive with their seeded mutators: tests/gantt/oracle.test.ts
// (seed 7, 5000) and tests/render/gantt-security.test.ts (seed 3, 4000), each of at most 4000
// characters. Rerunning at the same Pele pin writes the same file.
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

// The text of `const name = [...];` in a test file, from its opening bracket to the one that
// closes it at the start of a line.
function constant(file, name) {
  const text = readFileSync(join(tests, file), 'utf8');
  const found = new RegExp(`const ${name}[^\n]*? = (\\[[\\s\\S]*?\\n *\\]);`).exec(text);
  if (found === null) throw new Error(`${file} no longer declares ${name}`);
  return found[1];
}

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
    if (text === undefined || !/gantt/i.test(text)) continue;
    add(text, from);
    if (RE_TEMPLATE.test(text)) {
      for (const payload of PAYLOADS) add(text.replace(RE_TEMPLATE, () => payload), from);
    }
  }
}

// Pele's gantt corpus reads paths relative to its own checkout.
process.chdir(pele);
const corpus = loadCorpus('gantt', /gantt/);
for (const text of corpus) add(text, 'loadCorpus(gantt)');

// tests/render/gantt-security.test.ts writes each payload into every template.
const security = readFileSync(join(tests, 'render/gantt-security.test.ts'), 'utf8');
const HEAD = /const HEAD = '([^']*)';/.exec(security);
const TASKS = /const TASKS = '([^']*)';/.exec(security);
if (HEAD === null || TASKS === null) throw new Error('render/gantt-security.test.ts no longer declares HEAD and TASKS');
const TEMPLATES = new Function('HEAD', 'TASKS', 'return ' + constant('render/gantt-security.test.ts', 'TEMPLATES'))(
  value(HEAD[1]),
  value(TASKS[1])
);
for (const template of TEMPLATES) for (const payload of PAYLOADS) add(template(payload), 'gantt-security templates');

// The mutation runs of Pele's gantt tests.
const FRAGMENTS = new Function('return ' + constant('gantt/oracle.test.ts', 'FRAGMENTS'))();
const hostile = /const fragments = (\[[\s\S]*?\n {4}\]);/.exec(security);
if (hostile === null) throw new Error('render/gantt-security.test.ts no longer mutates the corpus');
const SECURITY = new Function('PAYLOADS', 'return ' + hostile[1])(PAYLOADS);
const mutated = new Set();
let illFormedMutations = 0;
function keep(text) {
  if (text.length > 4000 || sources.has(text)) return;
  if (text.isWellFormed()) mutated.add(text);
  else illFormedMutations++;
}
const next = mutator(corpus, FRAGMENTS, random(7));
for (let i = 0; i < 5000; i++) keep(next());
const nextHostile = mutator(corpus, SECURITY, random(3));
for (let i = 0; i < 4000; i++) keep(nextHostile());

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/gantt'), { recursive: true });
const out = join(root, 'tests/fixtures/gantt/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated] }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources and ${mutated.size} mutated inputs written to ${relative(root, out)}`);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
