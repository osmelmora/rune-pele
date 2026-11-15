// Collects every agentflow source in Pele's test suite into tests/fixtures/agentflow/harvest.json,
// so that crates/pele-core/tests/agentflow_front.rs and agentflow_parity.rs can run each one
// through Pele and through this port.
//
//   mise exec -- node scripts/harvest-agentflow.mjs
//
// `sources` are the string literals of vendor/pele/tests (render tests, oracle tests, the compat
// specs) that have a line starting `agentflow-beta`, with each `${p}` or `${payload}` filled in
// from Pele's hostile payload list and each `${H}` with the header the tests use, plus Pele's own
// agentflow corpus, the upstream fixtures, and the flowchart corpus with its keywords swapped as
// Pele's oracle test swaps them. `reference` are the sources of tests/corpus/agentflow-reference.json,
// which Pele compares with Mermaid's own model. `mutated` are the inputs Pele's agentflow tests
// derive from their corpora with their seeded mutator and their seeded token sequences.
// Rerunning at the same Pele pin writes the same file.
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
  const found = new RegExp(`const ${name}(?:: [^=]+)? = (\\[[\\s\\S]*?\\n\\]);`).exec(text);
  if (found === null) throw new Error(`${file} no longer declares ${name}`);
  return found[1];
}

const HEADER = 'agentflow-beta TB\n';
const startsAgentflow = (text) => /(?:^|\n)\s*agentflow-beta/.test(text);

const PAYLOADS = new Function('return ' + constant('support/inert.ts', 'PAYLOADS'))();
const RE_LITERAL = /`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"/g;
const RE_TEMPLATE = /\$\{(?:p|payload)\}/g;

const sources = new Set();
const origin = new Map();
let illFormed = 0;
function add(text, from, into = sources) {
  if (typeof text !== 'string' || into.has(text)) return;
  if (!text.isWellFormed()) {
    illFormed++;
    return;
  }
  into.add(text);
  origin.set(from, (origin.get(from) ?? 0) + 1);
}

for (const file of files(tests)) {
  const from = relative(tests, file);
  for (const match of readFileSync(file, 'utf8').matchAll(RE_LITERAL)) {
    const literal = value(match[1] ?? match[2] ?? match[3]);
    if (literal === undefined) continue;
    const text = literal.replaceAll('${H}', () => HEADER);
    if (!startsAgentflow(text)) continue;
    add(text, from);
    if (RE_TEMPLATE.test(text)) {
      for (const payload of PAYLOADS) add(text.replace(RE_TEMPLATE, () => payload), from);
    }
  }
}

// Pele's corpora read paths relative to its own checkout.
process.chdir(pele);
const agentflow = loadCorpus('agentflow', /agentflow-beta/);
for (const text of agentflow) add(text, 'loadCorpus(agentflow)');

const fixtures = readdirSync('tests/compat/agentflow/upstream')
  .filter((name) => name.endsWith('.mmd'))
  .sort()
  .map((name) => readFileSync(`tests/compat/agentflow/upstream/${name}`, 'utf8'));
for (const text of fixtures) add(text, 'compat/agentflow/upstream');

// The agentflow specs are thin on the syntax it shares with flowcharts, so Pele's oracle test
// reuses the flowchart corpus with its keywords swapped.
const fromFlowchart = loadCorpus('flowchart', /graph|flowchart/).map((src) =>
  src.replace(/\b(?:flowchart-elk|flowchart|graph)\b/g, 'agentflow-beta').replace(/\bsubgraph\b/g, 'flow')
);
for (const text of fromFlowchart) add(text, 'loadCorpus(flowchart), keywords swapped');

const reference = new Set();
for (const { src } of JSON.parse(readFileSync('tests/corpus/agentflow-reference.json', 'utf8'))) {
  add(src, 'corpus/agentflow-reference.json', reference);
}

// The runs of tests/agentflow/oracle.test.ts: mutation (seed 7, 5000 inputs of at most 4000
// characters) and random sequences of tokens (seed 8, 5000 inputs), and the mutation run of
// tests/render/agentflow-security.test.ts (seed 3, 4000 inputs).
const FRAGMENTS = new Function('return ' + constant('agentflow/oracle.test.ts', 'FRAGMENTS'))();
const SEEDS = new Function('return ' + constant('agentflow/oracle.test.ts', 'SEEDS'))();
const OPENERS = new Function(
  'return ' + /const openers = (\[[\s\S]*?\n {4}\]);/.exec(readFileSync(join(tests, 'agentflow/oracle.test.ts'), 'utf8'))[1]
)();
const oracleCorpus = [...new Set([...agentflow, ...fixtures, ...SEEDS, ...fromFlowchart])];
const security = /const fragments = (\[\s*\.\.\.PAYLOADS,[\s\S]*?\]);/.exec(
  readFileSync(join(tests, 'render/agentflow-security.test.ts'), 'utf8')
);
if (security === null) throw new Error('render/agentflow-security.test.ts no longer mutates the agentflow corpus');
const SECURITY = new Function('PAYLOADS', 'return ' + security[1])(PAYLOADS);

const mutated = new Set();
function keep(text) {
  if (text.length > 4000 || sources.has(text) || reference.has(text)) return;
  if (text.isWellFormed()) mutated.add(text);
  else illFormed++;
}
function mutate(corpus, fragments, seed, count) {
  const next = mutator(corpus, fragments, random(seed));
  for (let i = 0; i < count; i++) keep(next());
}
mutate(oracleCorpus, FRAGMENTS, 7, 5000);
mutate(agentflow, SECURITY, 3, 4000);
{
  const rnd = random(8);
  const pieces = [...new Set([...FRAGMENTS, ...FRAGMENTS.map((f) => f.trim()).filter((f) => f !== '')])];
  const pick = () => pieces[Math.floor(rnd() * pieces.length)];
  for (let i = 0; i < 5000; i++) {
    let src = OPENERS[Math.floor(rnd() * OPENERS.length)];
    for (let k = 1 + Math.floor(rnd() * 10); k > 0; k--) src += pick() + (rnd() < 0.3 ? ' ' : '');
    keep(src);
  }
}

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/agentflow'), { recursive: true });
const out = join(root, 'tests/fixtures/agentflow/harvest.json');
writeFileSync(
  out,
  JSON.stringify({ pele: pin, sources: [...sources], reference: [...reference], mutated: [...mutated] }, null, 1) + '\n'
);

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(
  `${sources.size} sources, ${reference.size} reference sources and ${mutated.size} mutated inputs written to ${relative(root, out)}`
);
console.log(`${illFormed} inputs left out: a lone surrogate cannot cross into Rust as a string`);
