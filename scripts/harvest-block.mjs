// Collects every block diagram source in Pele's test suite into tests/fixtures/block/harvest.json,
// so that crates/pele-core/tests/block_front.rs and block_parity.rs can put each one through Pele
// and through this port.
//
//   mise exec -- node scripts/harvest-block.mjs
//
// `sources` are the string literals of vendor/pele/tests that have a line starting `block`, each
// also with a line break after it, then the corpus and the link statements of Pele's block parser
// test (tests/block/oracle.test.ts), then each template of tests/render/block-security.test.ts
// with each of Pele's hostile payloads. `mutated` are the inputs of the seeded mutation runs of
// those two tests. Rerunning at the same Pele pin writes the same file.
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

const startsBlock = (text) => /(?:^|\n)\s*block/.test(text);
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
    if (text === undefined || !startsBlock(text)) continue;
    add(text, from);
    add(text + '\n', from);
  }
}

// tests/block/oracle.test.ts: the spec and documentation corpus, each also with a line break
// after it, and the statements with several links.
process.chdir(pele);
const oracleTest = read('block/oracle.test.ts');
const FRAGMENTS = new Function('return ' + constant('block/oracle.test.ts', 'FRAGMENTS'))();
const corpus = loadCorpus('block', /block/);
for (const text of corpus) {
  add(text, 'block/oracle.test.ts corpus');
  add(text + '\n', 'block/oracle.test.ts corpus');
}
const links = /for \(const src of (\[[\s\S]*?\])\) \{\n\s*compare\(src\);/.exec(oracleTest);
if (links === null) throw new Error('block/oracle.test.ts no longer lists its link statements');
for (const text of new Function('return ' + links[1])()) add(text, 'block/oracle.test.ts links');

// tests/render/block-security.test.ts: hostile text in every place the grammar takes text.
const PAYLOADS = new Function('return ' + constant('support/inert.ts', 'PAYLOADS'))();
const security = read('render/block-security.test.ts');
const templates = /const TEMPLATES[^\n]*= (\[[\s\S]*?\n\]);/.exec(security);
if (templates === null) throw new Error('render/block-security.test.ts no longer lists its templates');
for (const template of new Function('return ' + templates[1])()) {
  for (const payload of PAYLOADS) add(template(payload), 'render/block-security.test.ts templates');
}

// The mutation runs of tests/block/oracle.test.ts (seed 7, 5000 inputs) and of
// tests/render/block-security.test.ts (seed 3, 4000 inputs), each of at most 4000 characters.
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
const hostile = /const fragments = (\[\.\.\.PAYLOADS, [^\n]*\]);/.exec(security);
if (hostile === null) throw new Error('render/block-security.test.ts no longer mutates the block corpus');
mutate(new Function('PAYLOADS', 'return ' + hostile[1])(PAYLOADS), 3, 4000);

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/block'), { recursive: true });
const out = join(root, 'tests/fixtures/block/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated] }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources and ${mutated.size} mutated inputs written to ${relative(root, out)}`);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
