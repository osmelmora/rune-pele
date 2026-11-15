// Collects every class diagram source in Pele's test suite into tests/fixtures/class/harvest.json,
// so that crates/pele-core/tests/class_front.rs and class_parity.rs can put each one through Pele
// and through this port.
//
//   mise exec -- node scripts/harvest-class.mjs
//
// `sources` are the string literals of vendor/pele/tests that have a line starting
// `classDiagram`, with each `${p}` or `${payload}` filled in from Pele's hostile payload list as
// written and with its quotes as `#quot;`, then the corpus and the grammar tour of Pele's class
// parser test (tests/class/oracle.test.ts). `mutated` are the inputs Pele's class tests derive
// from those with their seeded mutator. Rerunning at the same Pele pin writes the same file.
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

const startsClass = (text) => /(?:^|\n)\s*classDiagram/.test(text);

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
    if (text === undefined || !startsClass(text)) continue;
    add(text, from);
    if (RE_TEMPLATE.test(text)) {
      for (const payload of PAYLOADS) {
        add(text.replace(RE_TEMPLATE, () => payload), from);
        add(text.replace(RE_TEMPLATE, () => payload.replace(/"/g, '#quot;')), from);
      }
    }
  }
}

// tests/class/oracle.test.ts: its corpus, with a header put in front of the spec pieces that have
// none, and its tour of the grammar, whose TypeScript annotations are dropped to run it here.
process.chdir(pele);
const literals = loadCorpus('class', /[^]/);
const pieces = literals.filter((s) => !s.includes('classDiagram'));
const corpus = [
  ...literals.filter((s) => s.includes('classDiagram')),
  ...pieces.filter((s) => !/^(?:should|when|given|does|gives|emits|moves|defaults) /.test(s)).map((s) => 'classDiagram\n' + s),
];
for (const text of corpus) add(text, 'class/oracle.test.ts corpus');
const oracleTest = read('class/oracle.test.ts');
const tourSource = /function tour\(\): string\[\] \{[\s\S]*?\n\}\n/.exec(oracleTest);
if (tourSource === null) throw new Error('class/oracle.test.ts no longer declares tour()');
const tour = new Function(tourSource[0].replace(/: string\[\]/g, '') + 'return tour();')();
for (const text of tour) add(text, 'class/oracle.test.ts tour');

// The two mutation runs of Pele's class tests: tests/class/oracle.test.ts (seed 7, 5000 inputs
// of at most 4000 characters) and tests/render/class-security.test.ts (seed 3, 3000 inputs).
const FRAGMENTS = new Function('return ' + constant('class/oracle.test.ts', 'FRAGMENTS'))();
const security = /const fragments = (\[\.\.\.PAYLOADS, [^\n]*\]);/.exec(read('render/class-security.test.ts'));
if (security === null) throw new Error('render/class-security.test.ts no longer mutates the class corpus');
const SECURITY = new Function('PAYLOADS', 'return ' + security[1])(PAYLOADS);
const mutated = new Set();
let illFormedMutations = 0;
function mutate(from, fragments, seed, count, longest) {
  const next = mutator(from, fragments, random(seed));
  for (let i = 0; i < count; i++) {
    const text = next();
    if (text.length > longest || sources.has(text)) continue;
    if (text.isWellFormed()) mutated.add(text);
    else illFormedMutations++;
  }
}
mutate([...corpus, ...tour], [...FRAGMENTS, ...pieces], 7, 5000, 4000);
mutate(loadCorpus('class', /classDiagram/), SECURITY, 3, 3000, Infinity);

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/class'), { recursive: true });
const out = join(root, 'tests/fixtures/class/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated] }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources and ${mutated.size} mutated inputs written to ${relative(root, out)}`);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
