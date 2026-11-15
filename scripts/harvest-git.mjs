// Collects every git graph source in Pele's test suite into tests/fixtures/git/harvest.json, so
// that crates/pele-core/tests/git_front.rs and git_parity.rs can put each one through Pele and
// through this port.
//
//   mise exec -- node scripts/harvest-git.mjs
//
// `sources` are the string literals of vendor/pele/tests that hold `gitGraph`, the corpus of
// Pele's git parser test (tests/git/oracle.test.ts), the flow of tests/render/git.test.ts under
// each header and config that file uses, and the templates of tests/render/git-security.test.ts
// with each of Pele's hostile payloads, quoted in turn as that test quotes them. `mutated` are the
// inputs of the seeded runs of those two tests: the parser test's mutations and token soup (seed
// 7, 10000 inputs of at most 2000 characters) and the security test's mutations (seed 11, 2000).
// Rerunning at the same Pele pin writes the same file.
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

// The text of `const name = [...];` in a test file, from its opening bracket to the one that
// closes it at the start of a line.
function constant(file, name) {
  const found = new RegExp(`const ${name}[^\n]*? = (\\[[\\s\\S]*?\\n *\\]);`).exec(read(file));
  if (found === null) throw new Error(`${file} no longer declares ${name}`);
  return found[1];
}

// A piece of a test file as JavaScript, its types stripped.
function code(file, pattern) {
  const found = pattern.exec(read(file));
  if (found === null) throw new Error(`${file} no longer holds ${pattern}`);
  return stripTypeScriptTypes(found[0]);
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
    if (text === undefined || !/gitGraph/.test(text)) continue;
    add(text, from);
  }
}

// tests/git/oracle.test.ts: its corpus, then its mutations and token soup.
const PARSER = 'git/oracle.test.ts';
const FRAGMENTS = new Function('return ' + constant(PARSER, 'FRAGMENTS'))();
const WORDS = new Function('return ' + constant(PARSER, 'WORDS'))();
const HEADERS = new Function('return ' + constant(PARSER, 'HEADERS'))();
const CLAUSES = new Function('return ' + constant(PARSER, 'CLAUSES'))();
const STATEMENTS = new Function('return ' + constant(PARSER, 'STATEMENTS'))();
const soup = new Function('WORDS', 'HEADERS', 'CLAUSES', 'STATEMENTS', code(PARSER, /function soup[\s\S]*?\n\}\n/) + '\nreturn soup;')(
  WORDS,
  HEADERS,
  CLAUSES,
  STATEMENTS
);
const corpusList = /const corpus = (\[[\s\S]*?\n\]);/.exec(read(PARSER));
if (corpusList === null) throw new Error(`${PARSER} no longer declares its corpus`);
process.chdir(pele);
const corpus = new Function('loadCorpus', 'return ' + corpusList[1])(loadCorpus);
for (const text of corpus) add(text, `${PARSER} corpus`);

// tests/render/git.test.ts: its flow under each header and each config the file gives.
const RENDER = 'render/git.test.ts';
const flow = /const FLOW = `([^`]*)`;/.exec(read(RENDER));
if (flow === null) throw new Error(`${RENDER} no longer declares FLOW`);
const FLOW = value(flow[1]);
const withConfig = (yaml, body) => `---\nconfig:\n  gitGraph:\n${yaml}\n---\n${body}`;
const configs = new Set(Array.from(read(RENDER).matchAll(/'( {4}[^']*)'/g), (match) => value(match[1])));
for (const header of ['gitGraph', 'gitGraph TB:', 'gitGraph BT:']) {
  add(header + '\n' + FLOW, `${RENDER} flow`);
  for (const yaml of configs) add(withConfig(yaml, header + '\n' + FLOW), `${RENDER} flow`);
}

// tests/render/git-security.test.ts: hostile text in every place the grammar takes text.
const SECURITY = 'render/git-security.test.ts';
const PAYLOADS = new Function('return ' + constant('support/inert.ts', 'PAYLOADS'))();
const quoted = new Function(code(SECURITY, /const quoted = [^\n]*\n/) + '\nreturn quoted;')();
const TEMPLATES = new Function('return ' + constant(SECURITY, 'TEMPLATES'))();
let turn = 0;
for (const payload of PAYLOADS) {
  const forms = quoted(payload);
  for (const template of TEMPLATES) add(template(forms[turn++ % forms.length], payload), `${SECURITY} templates`);
}

const mutated = new Set();
let illFormedMutations = 0;
function keep(text, longest) {
  if (text.length > longest || sources.has(text)) return;
  if (text.isWellFormed()) mutated.add(text);
  else illFormedMutations++;
}
const rnd = random(7);
const next = mutator(corpus, FRAGMENTS, rnd);
const words = soup(rnd);
for (let i = 0; i < 10000; i++) keep(i % 3 === 0 ? words() : next(), 2000);
const hostile = /mutator\(corpus, (\[\.\.\.PAYLOADS, [^\n]*\]), random\(11\)\)/.exec(read(SECURITY));
if (hostile === null) throw new Error(`${SECURITY} no longer mutates the git corpus`);
const nextHostile = mutator(loadCorpus('git', /gitGraph/), new Function('PAYLOADS', 'return ' + hostile[1])(PAYLOADS), random(11));
for (let i = 0; i < 2000; i++) keep(nextHostile(), 4000);

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/git'), { recursive: true });
const out = join(root, 'tests/fixtures/git/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated] }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources and ${mutated.size} mutated inputs written to ${relative(root, out)}`);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
