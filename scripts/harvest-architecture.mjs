// Collects every architecture diagram source in Pele's test suite into
// tests/fixtures/architecture/harvest.json, so that crates/pele-core/tests/architecture_front.rs,
// architecture_layout.rs and architecture_parity.rs can put each one through Pele and through this
// port.
//
//   mise exec -- node scripts/harvest-architecture.mjs
//
// `sources` are the string literals of vendor/pele/tests that have a line starting with
// `architecture`, with `${head}` as Pele's tests define it, each `${p}` or `${payload}` filled in
// from Pele's hostile payload list as written and with its quotes as `#quot;`, and each `${value}`
// from the extreme settings of tests/render/architecture-security.test.ts; then the corpus of
// Pele's architecture parser test, and the random diagrams of tests/render/architecture.test.ts.
// `mutated` are the inputs Pele's architecture tests derive from those: the mutations and the
// generated statements of tests/architecture/oracle.test.ts, and the mutations of
// tests/render/architecture-security.test.ts. `worst` are the worst cases of
// tests/architecture/stress.test.ts, which Pele renders with no limit on the source's length.
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
  const found = new RegExp(`const ${name} = (\\[[\\s\\S]*?\\n\\]);`).exec(read(file));
  if (found === null) throw new Error(`${file} no longer declares ${name}`);
  return found[1];
}

// The code of a test file from one line to the next, with its types stripped, run to give `names`.
function evaluate(file, from, to, names, args = {}) {
  const text = read(file);
  const start = text.indexOf(from);
  const end = text.indexOf(to, start);
  if (start < 0 || end < 0) throw new Error(`${file} no longer holds ${from} … ${to}`);
  const code = stripTypeScriptTypes(text.slice(start, end));
  return new Function(...Object.keys(args), `${code}\nreturn { ${names.join(', ')} };`)(...Object.values(args));
}

const startsArchitecture = (text) => /(?:^|\n)\s*architecture/.test(text);

const PAYLOADS = new Function('return ' + constant('support/inert.ts', 'PAYLOADS'))();
const SETTINGS = ['0', '-1', '1e308', '.inf', '.nan', '1e-9', '99999999', '[1]', '{a: 1}', 'null', 'true'];
const RE_LITERAL = /`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"/g;
const RE_TEMPLATE = /\$\{(?:p|payload)\}/g;
const RE_VALUE = /\$\{value\}/g;

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
    const text = found.replaceAll('${head}', 'architecture-beta\n');
    if (!startsArchitecture(text)) continue;
    if (RE_TEMPLATE.test(text)) {
      for (const payload of PAYLOADS) {
        add(text.replace(RE_TEMPLATE, () => payload), from);
        add(text.replace(RE_TEMPLATE, () => payload.replace(/"/g, '#quot;')), from);
      }
    } else if (RE_VALUE.test(text)) {
      for (const setting of SETTINGS) add(text.replace(RE_VALUE, () => setting), from);
    } else {
      add(text, from);
    }
  }
}

// tests/architecture/oracle.test.ts: the spec and documentation corpus and the seeds it adds.
process.chdir(pele);
const oracleTest = 'architecture/oracle.test.ts';
const { corpus } = evaluate(oracleTest, 'const corpus = [', 'async function compare', ['corpus'], {
  loadCorpus,
});
for (const text of corpus) add(text, 'architecture/oracle.test.ts corpus');

// tests/render/architecture.test.ts: 300 random diagrams of groups, services, junctions and edges.
{
  const rnd = random(11);
  const pick = (n) => Math.floor(rnd() * n);
  const sides = ['L', 'R', 'T', 'B'];
  for (let round = 0; round < 300; round++) {
    const groupCount = pick(6);
    const nodeCount = 1 + pick(14);
    const groupParent = [];
    let src = 'architecture-beta\n';
    for (let g = 0; g < groupCount; g++) {
      groupParent.push(g > 0 && rnd() < 0.5 ? pick(g) : -1);
      src += `group g${g}${rnd() < 0.5 ? '[Group title]' : ''}${groupParent[g] >= 0 ? ` in g${groupParent[g]}` : ''}\n`;
    }
    for (let i = 0; i < nodeCount; i++) {
      const parent = groupCount > 0 && rnd() < 0.7 ? pick(groupCount) : -1;
      const where = parent >= 0 ? ` in g${parent}` : '';
      src += rnd() < 0.15 ? `junction n${i}${where}\n` : `service n${i}${rnd() < 0.5 ? '[Name]' : ''}${where}\n`;
    }
    for (let k = pick(nodeCount * 2); k > 0; k--) {
      src += `n${pick(nodeCount)}:${sides[pick(4)]} -${rnd() < 0.2 ? '[label]-' : '-'}${rnd() < 0.5 ? '>' : ''} ${sides[pick(4)]}:n${pick(nodeCount)}\n`;
    }
    if (nodeCount > 2 && rnd() < 0.3) src += `align ${rnd() < 0.5 ? 'row' : 'column'} n0 n1 n2\n`;
    add(src, 'render/architecture.test.ts random diagrams');
  }
}

const mutated = new Set();
let illFormedMutations = 0;
function keep(text) {
  if (sources.has(text)) return;
  if (text.isWellFormed()) mutated.add(text);
  else illFormedMutations++;
}

// The mutations of tests/architecture/oracle.test.ts (seed 7, 3000 inputs of at most 2000
// characters) and of tests/render/architecture-security.test.ts (seed 3, 4000 of at most 4000).
function mutate(seeds, fragments, seed, count, longest) {
  const next = mutator(seeds, fragments, random(seed));
  for (let i = 0; i < count; i++) {
    const text = next();
    if (text.length <= longest) keep(text);
  }
}
mutate(corpus, new Function('return ' + constant(oracleTest, 'FRAGMENTS'))(), 7, 3000, 2000);
const security = /mutator\(corpus, fragments, random\(3\)\)/.test(read('render/architecture-security.test.ts'));
const fragments = /const fragments = (\[\.\.\.PAYLOADS, [^\n]*\]);/.exec(read('render/architecture-security.test.ts'));
if (!security || fragments === null) throw new Error('render/architecture-security.test.ts no longer mutates the corpus');
mutate(loadCorpus('architecture', /architecture/), new Function('PAYLOADS', 'return ' + fragments[1])(PAYLOADS), 3, 4000, 4000);

// The generated statements with token edits of tests/architecture/oracle.test.ts (seed 8, 3000).
{
  const { generator } = evaluate(oracleTest, 'const IDS = [', 'interface ReferenceToken', ['generator']);
  const next = generator(random(8));
  for (let i = 0; i < 3000; i++) keep(next());
}

// tests/architecture/stress.test.ts: every valid worst case and every one that may fail.
const worst = [];
{
  const { VALID, ANY } = evaluate('architecture/stress.test.ts', 'const N = ', "describe('architecture worst cases'", ['VALID', 'ANY'], {
    metricsMeasurer: undefined,
  });
  for (const [, text] of [...VALID, ...ANY]) if (text.isWellFormed()) worst.push(text);
}

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/architecture'), { recursive: true });
const out = join(root, 'tests/fixtures/architecture/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated], worst }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources, ${mutated.size} mutated inputs and ${worst.length} worst cases written to ${relative(root, out)}`);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
