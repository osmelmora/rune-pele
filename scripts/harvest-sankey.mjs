// Collects every sankey source in Pele's test suite into tests/fixtures/sankey/harvest.json, so
// that crates/pele-core/tests/sankey_front.rs, sankey_layout.rs and sankey_parity.rs can put each
// one through Pele and through this port.
//
//   mise exec -- node scripts/harvest-sankey.mjs
//
// `sources` are the string literals of vendor/pele/tests that hold `sankey` in any case, the
// corpus of Pele's sankey parser test (tests/sankey/oracle.test.ts) with each also in the form
// Mermaid hands its parser, the configs of tests/render/sankey.test.ts on its basic chart, and the
// templates of tests/render/sankey-security.test.ts with each of Pele's hostile payloads.
// `mutated` are the inputs of the seeded runs of the parser test: its mutations (seed 7, 5000, of
// at most 4000 characters, every other one also in Mermaid's form) and its inputs assembled from
// fragments (seed 8, 2500), then the security test's mutations of the corpus (seed 3, 2000).
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
    if (text === undefined || !/sankey/i.test(text)) continue;
    add(text, from);
  }
}

// tests/sankey/oracle.test.ts: its corpus, each as written and in Mermaid's form.
const PARSER = 'sankey/oracle.test.ts';
const FRAGMENTS = new Function('return ' + constant(PARSER, 'FRAGMENTS'))();
const ROW_EDGES = new Function(code(PARSER, /const ROW_EDGES = [^\n]*\n/) + '\nreturn ROW_EDGES;')();
const reference = new Function('ROW_EDGES', code(PARSER, /const reference = [\s\S]*?\.trim\(\);\n/) + '\nreturn reference;')(ROW_EDGES);
// Pele's `cleanupComments` (src/preprocess.ts), which Node cannot load from Pele's source: the
// comment lines go, then the leading white space.
const preprocess = readFileSync(join(pele, 'src/preprocess.ts'), 'utf8');
const comment = /const RE_COMMENT = (\/.*\/gm);/.exec(preprocess);
if (comment === null || !preprocess.includes("if (text.includes('%%')) text = removeAll(text, RE_COMMENT, steps);")) {
  throw new Error('src/preprocess.ts no longer strips comments as this harvest does');
}
const RE_COMMENT = new Function('return ' + comment[1])();
const cleanupComments = (text) => (text.includes('%%') ? text.replace(RE_COMMENT, '') : text).trimStart();
process.chdir(pele);
const energy = readFileSync('tests/compat/sankey/upstream/energy.csv', 'utf8');
const rawList = /const raw = (\[[\s\S]*?\n\]);/.exec(read(PARSER));
if (rawList === null) throw new Error(`${PARSER} no longer declares raw`);
const raw = new Function('loadCorpus', 'cleanupComments', 'energy', 'return ' + rawList[1])(loadCorpus, cleanupComments, energy);
const corpus = [...new Set([...raw, ...raw.map(reference)])];
for (const text of corpus) add(text, `${PARSER} corpus`);

// tests/render/sankey.test.ts: each config it gives, on its basic chart.
const RENDER = 'render/sankey.test.ts';
const BASIC = value(/const BASIC = '([^']*)';/.exec(read(RENDER))[1]);
const configured = (sankey, body = BASIC) =>
  `---\nconfig:\n  sankey:\n${sankey
    .split('\n')
    .map((line) => '    ' + line)
    .join('\n')}\n---\n${body}`;
for (const match of read(RENDER).matchAll(/configured\((`[^`]*`|'(?:[^'\\]|\\.)*'|"(?:[^"\\]|\\.)*")/g)) {
  const yaml = value(match[1].slice(1, -1));
  if (yaml !== undefined && !yaml.includes('${')) add(configured(yaml), `${RENDER} configs`);
}

// tests/render/sankey-security.test.ts: hostile text in every place the grammar takes text.
const SECURITY = 'render/sankey-security.test.ts';
const PAYLOADS = new Function('return ' + constant('support/inert.ts', 'PAYLOADS'))();
const BODY = value(/const BODY = '([^']*)';/.exec(read(SECURITY))[1]);
const quoted = new Function(code(SECURITY, /const quoted = [^\n]*\n/) + '\nreturn quoted;')();
const json = new Function(code(SECURITY, /const json = [^\n]*\n/) + '\nreturn json;')();
const TEMPLATES = new Function('BODY', 'quoted', 'json', 'return ' + constant(SECURITY, 'TEMPLATES'))(BODY, quoted, json);
for (const template of TEMPLATES) for (const payload of PAYLOADS) add(template(payload), `${SECURITY} templates`);

const mutated = new Set();
let illFormedMutations = 0;
function keep(text) {
  if (text.length > 4000 || sources.has(text)) return;
  if (text.isWellFormed()) mutated.add(text);
  else illFormedMutations++;
}
const next = mutator(corpus, FRAGMENTS, random(7));
for (let i = 0; i < 5000; i++) {
  const text = next();
  if (text.length > 4000) continue;
  keep(text);
  if (i % 2 === 0) keep(reference(text));
}
const rnd = random(8);
for (let i = 0; i < 2500; i++) {
  let text = rnd() < 0.7 ? 'sankey\n' : '';
  for (let k = 1 + Math.floor(rnd() * 14); k > 0; k--) text += FRAGMENTS[Math.floor(rnd() * FRAGMENTS.length)];
  keep(text);
}
const hostile = /mutator\(corpus, (\[\.\.\.PAYLOADS, [^\n]*\]), random\((\d+)\)\)/.exec(read(SECURITY));
if (hostile !== null) {
  const nextHostile = mutator(loadCorpus('sankey', /sankey/i), new Function('PAYLOADS', 'return ' + hostile[1])(PAYLOADS), random(Number(hostile[2])));
  for (let i = 0; i < 2000; i++) keep(nextHostile());
}

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/sankey'), { recursive: true });
const out = join(root, 'tests/fixtures/sankey/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated] }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources and ${mutated.size} mutated inputs written to ${relative(root, out)}`);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
