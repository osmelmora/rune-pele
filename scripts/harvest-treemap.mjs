// Collects every treemap source in Pele's test suite into tests/fixtures/treemap/harvest.json, so
// that crates/pele-core/tests/treemap_front.rs and treemap_parity.rs can put each one through Pele
// and through this port.
//
//   mise exec -- node scripts/harvest-treemap.mjs
//
// `sources` are the string literals of vendor/pele/tests that have a line starting with
// `treemap`; every template of tests/render/treemap-security.test.ts with every hostile payload;
// and the corpus and seeds of Pele's treemap parser test. `mutated` are the inputs Pele's treemap
// tests derive from those: the mutations of tests/treemap/oracle.test.ts and of
// tests/render/treemap-security.test.ts. `worst` are the sources of tests/treemap/stress.test.ts,
// which Pele reads with no limit on the source's length. Rerunning at the same Pele pin writes the
// same file.
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

const RE_LITERAL = /`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"/g;

// Pele's loadTestStrings (tests/support/langium-extra.ts), which imports Mermaid's own parser and
// so cannot load here: the literals of tests/compat/<diagram>/*.test.ts that the keyword finds.
function loadTestStrings(diagram, keyword) {
  const seeds = new Set();
  const dir = join('tests/compat', diagram);
  for (const file of readdirSync(dir).filter((f) => /\.test\.[jt]s$/.test(f))) {
    for (const m of readFileSync(join(dir, file), 'utf8').matchAll(RE_LITERAL)) {
      const raw = m[1] ?? m[2] ?? m[3];
      if (!keyword.test(raw)) continue;
      const found = value(raw);
      if (found !== undefined) seeds.add(found);
    }
  }
  return [...seeds];
}

const startsTreemap = (text) => /(?:^|\n)\s*treemap/.test(text);
const PAYLOADS = new Function('return ' + constant('support/inert.ts', 'PAYLOADS'))();

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
    if (found !== undefined && startsTreemap(found) && !found.includes('${')) add(found, from);
  }
}

// tests/render/treemap-security.test.ts: each template with each payload.
const security = 'render/treemap-security.test.ts';
{
  const { TEMPLATES } = evaluate(security, 'const TEMPLATES', "describe('inert treemap output'", ['TEMPLATES']);
  for (const payload of PAYLOADS) for (const template of TEMPLATES) add(template(payload), `${security} templates`);
}

// tests/treemap/oracle.test.ts: the spec and documentation corpus and the seeds it adds.
process.chdir(pele);
const oracleTest = 'treemap/oracle.test.ts';
const { corpus } = evaluate(oracleTest, 'const corpus = [', 'const referenceLex', ['corpus'], {
  loadCorpus,
  loadTestStrings,
});
for (const text of corpus) add(text, `${oracleTest} corpus`);

const mutated = new Set();
let illFormedMutations = 0;
function keep(text) {
  if (sources.has(text)) return;
  if (text.isWellFormed()) mutated.add(text);
  else illFormedMutations++;
}

// The mutations of tests/treemap/oracle.test.ts (seed 7, 3000 inputs of at most 2000 characters)
// and of tests/render/treemap-security.test.ts (seed 3, 4000 of at most 4000).
function mutate(seeds, fragments, seed, count, longest) {
  const next = mutator(seeds, fragments, random(seed));
  for (let i = 0; i < count; i++) {
    const text = next();
    if (text.length <= longest) keep(text);
  }
}
mutate(corpus, new Function('return ' + constant(oracleTest, 'FRAGMENTS'))(), 7, 3000, 2000);
const fragments = /const fragments = (\[\.\.\.PAYLOADS, [^\n]*\]);/.exec(read(security));
if (fragments === null || !/mutator\(corpus, fragments, random\(3\)\)/.test(read(security))) {
  throw new Error(`${security} no longer mutates the corpus`);
}
mutate(loadCorpus('treemap', /treemap/), new Function('PAYLOADS', 'return ' + fragments[1])(PAYLOADS), 3, 4000, 4000);

// tests/treemap/stress.test.ts: the source of every worst case, read by stand-ins for Pele's
// render, parse and parser that hand back what they are given.
const worst = [];
{
  const given = (text) => text;
  const { CASES } = evaluate('treemap/stress.test.ts', 'const N = ', "describe('treemap worst cases'", ['CASES'], {
    metricsMeasurer: undefined,
    render: given,
    parse: given,
    parseTreemap: given,
  });
  for (const [, run] of CASES) {
    const text = run();
    if (text.isWellFormed()) worst.push(text);
  }
}

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/treemap'), { recursive: true });
const out = join(root, 'tests/fixtures/treemap/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated], worst }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources, ${mutated.size} mutated inputs and ${worst.length} worst cases written to ${relative(root, out)}`);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
