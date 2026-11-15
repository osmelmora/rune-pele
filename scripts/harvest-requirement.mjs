// Collects every requirement source in Pele's test suite into tests/fixtures/requirement/harvest.json,
// so that crates/pele-core/tests/requirement_front.rs and requirement_parity.rs can run each one
// through Pele and through this port.
//
//   mise exec -- node scripts/harvest-requirement.mjs
//
// `sources` are the string literals of vendor/pele/tests that hold `requirementDiagram`, with each
// `${p}` or `${payload}` filled in from Pele's hostile payload list, the corpus of
// tests/requirement/oracle.test.ts, and the hostile templates of
// tests/render/requirement-security.test.ts applied to every payload. `mutated` are the inputs
// Pele's requirement tests derive with their seeded mutators and fragment assembly. Rerunning at
// the same Pele pin writes the same file.
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
    if (text === undefined || !/requirementDiagram/i.test(text)) continue;
    add(text, from);
    if (RE_TEMPLATE.test(text)) {
      for (const payload of PAYLOADS) add(text.replace(RE_TEMPLATE, () => payload), from);
    }
  }
}

// Pele's requirement corpus reads paths relative to its own checkout.
process.chdir(pele);
const docs = loadCorpus('requirement', /requirementDiagram/i);
for (const text of docs) add(text, 'loadCorpus(requirement)');

// tests/requirement/oracle.test.ts builds its corpus with code, so it is run here as written.
const corpus = new Function('loadCorpus', 'return ' + constant('requirement/oracle.test.ts', 'corpus'))(loadCorpus);
for (const text of corpus) add(text, 'requirement/oracle.test.ts corpus');

// tests/render/requirement-security.test.ts writes each payload into every template.
const BODY = 'requirement a {\nid: 1\n}\nelement b {\ntype: t\n}\n';
const TEMPLATES = new Function('BODY', 'return ' + constant('render/requirement-security.test.ts', 'TEMPLATES'))(BODY);
for (const template of TEMPLATES) for (const payload of PAYLOADS) add(template(payload), 'requirement-security templates');

// The mutation runs of Pele's requirement tests: tests/requirement/oracle.test.ts (seed 7, 5000
// mutated inputs, and 2500 assembled from fragments with seed 8) and
// tests/render/requirement-security.test.ts (seed 3, 4000), each of at most 4000 characters.
const FRAGMENTS = new Function('return ' + constant('requirement/oracle.test.ts', 'FRAGMENTS'))();
const security = /mutator\(corpus, (\[\.\.\.PAYLOADS, [^\n]*\]), random\(3\)\)/.exec(
  readFileSync(join(tests, 'render/requirement-security.test.ts'), 'utf8')
);
if (security === null) throw new Error('render/requirement-security.test.ts no longer mutates the corpus');
const SECURITY = new Function('PAYLOADS', 'return ' + security[1])(PAYLOADS);
const mutated = new Set();
let illFormedMutations = 0;
function keep(text) {
  if (text.length > 4000 || sources.has(text)) return;
  if (text.isWellFormed()) mutated.add(text);
  else illFormedMutations++;
}
const next = mutator(corpus, FRAGMENTS, random(7));
for (let i = 0; i < 5000; i++) keep(next());
const rnd = random(8);
for (let i = 0; i < 2500; i++) {
  let src = rnd() < 0.7 ? 'requirementDiagram\n' : '';
  for (let k = 1 + Math.floor(rnd() * 14); k > 0; k--) src += FRAGMENTS[Math.floor(rnd() * FRAGMENTS.length)];
  keep(src);
}
const hostile = mutator(docs, SECURITY, random(3));
for (let i = 0; i < 4000; i++) keep(hostile());

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/requirement'), { recursive: true });
const out = join(root, 'tests/fixtures/requirement/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated] }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources and ${mutated.size} mutated inputs written to ${relative(root, out)}`);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
