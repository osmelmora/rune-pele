// Collects every flowchart source in Pele's test suite into tests/fixtures/flowchart/harvest.json,
// so that crates/pele-core/tests/flowchart_front.rs can parse each one through Pele and through
// this port.
//
//   mise exec -- node scripts/harvest-flowchart.mjs
//
// `sources` are the string literals of vendor/pele/tests (render tests, oracle tests, the compat
// specs) that have a line starting `graph` or `flowchart`, with each `${p}` or `${payload}` filled
// in from Pele's hostile payload list, plus Pele's own flowchart corpus and the documentation
// corpus. `mutated` are the inputs Pele's flowchart tests derive from that corpus with their
// seeded mutator. Rerunning at the same Pele pin writes the same file.
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

function constant(file, name) {
  const text = readFileSync(join(tests, file), 'utf8');
  const found = new RegExp(`const ${name} = (\\[[\\s\\S]*?\\n\\]);`).exec(text);
  if (found === null) throw new Error(`${file} no longer declares ${name}`);
  return found[1];
}

const startsFlow = (text) => /(?:^|\n)\s*(?:graph|flowchart)/.test(text);

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
    if (text === undefined || !startsFlow(text)) continue;
    add(text, from);
    if (RE_TEMPLATE.test(text)) {
      for (const payload of PAYLOADS) add(text.replace(RE_TEMPLATE, () => payload), from);
    }
  }
}

// Pele's flowchart corpus reads paths relative to its own checkout.
process.chdir(pele);
const corpus = loadCorpus('flowchart', /graph|flowchart/);
for (const text of corpus) add(text, 'loadCorpus(flowchart)');

// The two mutation runs of Pele's flowchart tests: tests/flowchart/oracle.test.ts (seed 7, 5000
// inputs) and tests/render/security.test.ts (seed 3, 4000 inputs), each of at most 4000 characters.
const FRAGMENTS = new Function('return ' + constant('flowchart/oracle.test.ts', 'FRAGMENTS'))();
const security = /mutator\(corpus, (\[\.\.\.PAYLOADS, [^\n]*\]), random\(3\)\)/.exec(
  readFileSync(join(tests, 'render/security.test.ts'), 'utf8')
);
if (security === null) throw new Error('render/security.test.ts no longer mutates the flowchart corpus');
const SECURITY = new Function('PAYLOADS', 'return ' + security[1])(PAYLOADS);
const mutated = new Set();
let illFormedMutations = 0;
function mutate(fragments, seed, count) {
  const next = mutator(corpus, fragments, random(seed));
  for (let i = 0; i < count; i++) {
    const text = next();
    if (text.length > 4000 || sources.has(text)) continue;
    if (text.isWellFormed()) mutated.add(text);
    else illFormedMutations++;
  }
}
mutate(FRAGMENTS, 7, 5000);
mutate(SECURITY, 3, 4000);

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/flowchart'), { recursive: true });
const out = join(root, 'tests/fixtures/flowchart/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated] }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources and ${mutated.size} mutated inputs written to ${relative(root, out)}`);
console.log(
  `${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`
);
