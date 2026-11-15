// Collects every cynefin diagram source in Pele's test suite into
// tests/fixtures/cynefin/harvest.json, so that crates/pele-core/tests/cynefin_front.rs and
// cynefin_parity.rs can put each one through Pele and through this port.
//
//   mise exec -- node scripts/harvest-cynefin.mjs
//
// `sources` are the string literals of vendor/pele/tests that have a line starting with
// `cynefin`; every template of tests/render/cynefin-security.test.ts with every hostile payload,
// and its extreme settings; and the corpus and seeds of Pele's cynefin parser test. `mutated` are
// the inputs Pele's cynefin tests derive from those: the mutations of
// tests/cynefin/oracle.test.ts and of tests/render/cynefin-security.test.ts. `worst` are the worst
// cases of tests/cynefin/stress.test.ts, which Pele renders with no limit on the source's length.
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

const startsCynefin = (text) => /(?:^|\n)\s*cynefin/.test(text);
const PAYLOADS = new Function('return ' + constant('support/inert.ts', 'PAYLOADS'))();
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
    const found = value(match[1] ?? match[2] ?? match[3]);
    if (found !== undefined && startsCynefin(found) && !found.includes('${')) add(found, from);
  }
}

// tests/render/cynefin-security.test.ts: each template with each payload, and the extreme options.
const security = 'render/cynefin-security.test.ts';
{
  const { TEMPLATES } = evaluate(security, 'const quoted = ', "describe('cynefin output is inert'", ['TEMPLATES']);
  for (const payload of PAYLOADS) for (const template of TEMPLATES) add(template(payload), `${security} templates`);
  const bodies = /for \(const body of (\[[^\n]*\])\)/.exec(read(security));
  if (bodies === null) throw new Error(`${security} no longer tries extreme options`);
  for (const body of new Function('return ' + bodies[1])()) {
    add(`---\nconfig:\n  cynefin:\n    ${body}\n---\ncynefin-beta\ncomplex\n"a"\ncomplex --> clear\n`, `${security} options`);
  }
}

// tests/cynefin/oracle.test.ts: the spec and documentation corpus and the seeds it adds.
process.chdir(pele);
const oracleTest = 'cynefin/oracle.test.ts';
const { corpus } = evaluate(oracleTest, 'const VALID = [', 'async function compare', ['corpus'], { loadCorpus });
for (const text of corpus) add(text, `${oracleTest} corpus`);

const mutated = new Set();
let illFormedMutations = 0;
function keep(text) {
  if (sources.has(text)) return;
  if (text.isWellFormed()) mutated.add(text);
  else illFormedMutations++;
}

// The mutations of tests/cynefin/oracle.test.ts (seed 7, 3000 inputs of at most 2000 characters)
// and of tests/render/cynefin-security.test.ts (seed 13, 2000).
function mutate(seeds, fragments, seed, count, longest) {
  const next = mutator(seeds, fragments, random(seed));
  for (let i = 0; i < count; i++) {
    const text = next();
    if (text.length <= longest) keep(text);
  }
}
mutate(corpus, new Function('return ' + constant(oracleTest, 'FRAGMENTS'))(), 7, 3000, 2000);
const fragments = /mutator\(corpus, (\[\.\.\.PAYLOADS, [^\n]*\]), random\(13\)\)/.exec(read(security));
if (fragments === null) throw new Error(`${security} no longer mutates the corpus`);
mutate(loadCorpus('cynefin', /cynefin-beta/), new Function('PAYLOADS', 'return ' + fragments[1])(PAYLOADS), 13, 2000, Infinity);

// tests/cynefin/stress.test.ts: every worst case.
const worst = [];
{
  const { CASES } = evaluate('cynefin/stress.test.ts', 'const N = ', "describe('cynefin worst cases'", ['CASES'], {
    metricsMeasurer: undefined,
  });
  for (const [, text] of CASES) if (text.isWellFormed()) worst.push(text);
}

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/cynefin'), { recursive: true });
const out = join(root, 'tests/fixtures/cynefin/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated], worst }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources, ${mutated.size} mutated inputs and ${worst.length} worst cases written to ${relative(root, out)}`);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
