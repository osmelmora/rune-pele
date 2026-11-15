// Collects every packet source in Pele's test suite into tests/fixtures/packet/harvest.json, so
// that crates/pele-core/tests/packet_front.rs and packet_parity.rs can put each one through Pele
// and through this port.
//
//   mise exec -- node scripts/harvest-packet.mjs
//
// `sources` are the string literals of vendor/pele/tests that hold `packet`, the corpus of Pele's
// packet parser test (tests/packet/oracle.test.ts), each config of tests/render/packet.test.ts on
// each chart that file draws, and the templates of tests/render/packet-security.test.ts with each
// of Pele's hostile payloads. `mutated` are the inputs of the seeded runs of those two tests: the
// parser test's mutations (seed 7, 3000 of at most 2000 characters) and the security test's
// (seed 3, 4000 of at most 4000). Rerunning at the same Pele pin writes the same file.
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

// The text of `const name = [...];` in a test file, from its opening bracket to the one that
// closes it at the start of a line.
function constant(file, name) {
  const found = new RegExp(`const ${name}[^\n]*? = (\\[[\\s\\S]*?\\n *\\]);`).exec(read(file));
  if (found === null) throw new Error(`${file} no longer declares ${name}`);
  return found[1];
}

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
    if (text === undefined || !/packet/.test(text)) continue;
    add(text, from);
  }
}

// Pele's `loadTestStrings` (tests/support/langium-extra.ts, which Node cannot load here): the
// string literals of the compat test files that hold the keyword, as its loadCorpus reads them.
function loadTestStrings(diagram, keyword) {
  const seeds = new Set();
  const dir = join('tests/compat', diagram);
  for (const file of readdirSync(dir).filter((f) => /\.test\.[jt]s$/.test(f))) {
    for (const match of readFileSync(join(dir, file), 'utf8').matchAll(RE_LITERAL)) {
      const raw = match[1] ?? match[2] ?? match[3];
      if (!keyword.test(raw)) continue;
      const text = value(raw);
      if (text !== undefined) seeds.add(text);
    }
  }
  return [...seeds];
}

// tests/packet/oracle.test.ts: its corpus, then its mutations.
const PARSER = 'packet/oracle.test.ts';
const FRAGMENTS = new Function('return ' + constant(PARSER, 'FRAGMENTS'))();
const corpusList = /const corpus = (\[[\s\S]*?\n\]);/.exec(read(PARSER));
if (corpusList === null) throw new Error(`${PARSER} no longer declares its corpus`);
process.chdir(pele);
const corpus = new Function('loadCorpus', 'loadTestStrings', 'return ' + corpusList[1])(loadCorpus, loadTestStrings);
for (const text of corpus) add(text, `${PARSER} corpus`);

// tests/render/packet.test.ts: each config it gives, on each chart it draws.
const RENDER = 'render/packet.test.ts';
const TCP = value(/const TCP = '([^']*)';/.exec(read(RENDER))[1]);
const config = (packet, body) => `---\nconfig:\n  packet:\n${packet}\n---\n${body}`;
const literals = Array.from(read(RENDER).matchAll(RE_LITERAL), (match) => value(match[1] ?? match[2] ?? match[3]));
const yamls = literals.filter((text) => typeof text === 'string' && /^ {4}\w+:/.test(text));
const bodies = [TCP, ...literals.filter((text) => typeof text === 'string' && /^packet(-beta)?\n/.test(text))];
for (const yaml of yamls) for (const body of bodies) add(config(yaml, body), `${RENDER} configs`);

// tests/render/packet-security.test.ts: hostile text in every place the grammar takes text.
const SECURITY = 'render/packet-security.test.ts';
const PAYLOADS = new Function('return ' + constant('support/inert.ts', 'PAYLOADS'))();
const TEMPLATES = new Function('return ' + constant(SECURITY, 'TEMPLATES'))();
for (const template of TEMPLATES) for (const payload of PAYLOADS) add(template(payload), `${SECURITY} templates`);

const mutated = new Set();
let illFormedMutations = 0;
function keep(text, longest) {
  if (text.length > longest || sources.has(text)) return;
  if (text.isWellFormed()) mutated.add(text);
  else illFormedMutations++;
}
const next = mutator(corpus, FRAGMENTS, random(7));
for (let i = 0; i < 3000; i++) keep(next(), 2000);
const hostile = /mutator\(corpus, (\[\.\.\.PAYLOADS, [^\n]*\]), random\(3\)\)/.exec(read(SECURITY));
if (hostile === null) throw new Error(`${SECURITY} no longer mutates the packet corpus`);
const nextHostile = mutator(loadCorpus('packet', /packet/), new Function('PAYLOADS', 'return ' + hostile[1])(PAYLOADS), random(3));
for (let i = 0; i < 4000; i++) keep(nextHostile(), 4000);

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/packet'), { recursive: true });
const out = join(root, 'tests/fixtures/packet/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated] }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources and ${mutated.size} mutated inputs written to ${relative(root, out)}`);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
