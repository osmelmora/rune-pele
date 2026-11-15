// Collects every venn diagram source in Pele's test suite into tests/fixtures/venn/harvest.json,
// so that crates/pele-core/tests/venn_front.rs, venn_layout.rs and venn_parity.rs can put each one
// through Pele and through this port.
//
//   mise exec -- node scripts/harvest-venn.mjs
//
// `sources` are the string literals of vendor/pele/tests that have a line starting with
// `venn-beta`, with each `${V}` as tests/render/venn-security.test.ts defines it and each `${p}`
// filled in from Pele's hostile payload list; then the corpus and the seeds of Pele's venn parser
// test (tests/venn/oracle.test.ts). `mutated` are the inputs Pele's venn tests derive from those:
// the mutations, random token sequences and random diagrams of tests/venn/oracle.test.ts, and the
// mutations of tests/render/venn-security.test.ts. Rerunning at the same Pele pin writes the same
// file.
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
  const found = new RegExp(`const ${name}(?:: [^=]+)? = (\\[[\\s\\S]*?\\n\\]);`).exec(read(file));
  if (found === null) throw new Error(`${file} no longer declares ${name}`);
  return new Function('return ' + found[1])();
}

const startsVenn = (text) => /(?:^|\n)\s*venn-beta/.test(text);

const PAYLOADS = constant('support/inert.ts', 'PAYLOADS');
const RE_LITERAL = /`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"/g;
const RE_TEMPLATE = /\$\{p\}/g;

const v = /const V = '([^']*)';/.exec(read('render/venn-security.test.ts'));
if (v === null) throw new Error('render/venn-security.test.ts no longer declares V');
const V = value(v[1]);

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
    const text = found.replaceAll('${V}', V);
    if (!startsVenn(text)) continue;
    add(text, from);
    if (RE_TEMPLATE.test(text)) {
      for (const payload of PAYLOADS) add(text.replace(RE_TEMPLATE, () => payload), from);
    }
  }
}

// tests/render/venn.test.ts writes most of its diagrams without the header, which its `svgOf` adds.
const RE_SVG_OF = /svgOf\(\s*(?:`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)')/g;
for (const match of read('render/venn.test.ts').matchAll(RE_SVG_OF)) {
  const found = value(match[1] ?? match[2]);
  if (found !== undefined && !found.includes('${')) add('venn-beta\n' + found, 'render/venn.test.ts svgOf');
}

// tests/venn/oracle.test.ts: the spec and documentation corpus, and its seeds.
const oracleTest = 'venn/oracle.test.ts';
const SEEDS = constant(oracleTest, 'SEEDS');
const FRAGMENTS = constant(oracleTest, 'FRAGMENTS');
process.chdir(pele);
const corpus = [...new Set([...loadCorpus('venn', /venn-beta/), ...SEEDS])];
for (const text of corpus) add(text, 'venn/oracle.test.ts corpus');

const mutated = new Set();
let illFormedMutations = 0;
function keep(text) {
  if (sources.has(text)) return;
  if (text.isWellFormed()) mutated.add(text);
  else illFormedMutations++;
}

// The mutations of tests/venn/oracle.test.ts (seed 7, 5000 inputs of at most 4000 characters).
{
  const next = mutator(corpus, FRAGMENTS, random(7));
  for (let i = 0; i < 5000; i++) {
    const src = next();
    if (src.length <= 4000) keep(src);
  }
}

// Its random sequences of tokens (seed 8, 5000 of them).
{
  const rnd = random(8);
  const pieces = [...new Set([...FRAGMENTS, ...FRAGMENTS.map((f) => f.trim()).filter((f) => f !== '')])];
  const pick = () => pieces[Math.floor(rnd() * pieces.length)];
  const openers = [
    'venn-beta\n',
    'venn-beta\nset A\n',
    'venn-beta\nset A\nset B\nunion A,B\n  ',
    'venn-beta\nset A\n  text ',
    'venn-beta\nset A\nstyle A ',
    'venn-beta\nset A\nstyle A fill:',
    'venn-beta\nset A\ntext A ',
    'venn-beta ',
    '',
  ];
  for (let i = 0; i < 5000; i++) {
    let src = openers[Math.floor(rnd() * openers.length)];
    for (let k = 1 + Math.floor(rnd() * 8); k > 0; k--) src += pick() + (rnd() < 0.4 ? ' ' : rnd() < 0.3 ? '\n' : '');
    keep(src);
  }
}

// Its random diagrams built from statements (seed 9, 5000 of them).
{
  const rnd = random(9);
  const pick = (list) => list[Math.floor(rnd() * list.length)];
  const id = () => pick(['A', 'B', 'C', '"D e"', 'x-1', '_y']);
  const label = () => pick(['', '', '["L"]', '[L m]', '[""]']);
  const size = () => pick(['', '', ':5', ': 2.5', ':-1', ':.5']);
  const indent = () => pick(['', '', '  ', '    ', '\t', ' ']);
  const tail = () => pick(['t1', '"q r"', '7', 't2["L"]', '"q"[L]', '7["L"]', 't3 %% c', 'text', 'set']);
  const statements = [
    () => `set ${id()}${label()}${size()}`,
    () => `union ${id()},${id()}${label()}${size()}`,
    () => `union ${id()},${id()},${id()}${label()}`,
    () => `text ${tail()}`,
    () => `text ${id()} ${tail()}`,
    () => `text ${id()},${id()} ${tail()}`,
    () => `Text ${tail()}`,
    () => `style ${id()} fill:${pick(['red', '#ff6b6b', 'rgb(1, 2, 3)', '"x y"', 'a b 5'])}`,
    () => `style ${id()},${id()} color:#333, stroke-width:${pick(['2', '2px', '.5'])}`,
    () => `title ${pick(['T', 'A b c', '"quoted"', 'x # y', 'a; b'])}`,
    () => pick(['', '   ', '%% comment', 'x%% eaten', '%%{ directive }%%']),
  ];
  for (let i = 0; i < 5000; i++) {
    let src = pick(['venn-beta', '  venn-beta', '\nvenn-beta']) + pick(['\n', '\n', ' ', '\r\n']);
    for (let k = 1 + Math.floor(rnd() * 8); k > 0; k--) src += indent() + pick(statements)() + pick(['\n', '\n', '\n', ' ', '\r\n', '\n\n']);
    keep(src);
  }
}

// The mutations of tests/render/venn-security.test.ts (seed 3, 4000 of at most 4000 characters).
{
  const security = /const fragments = (\[\.\.\.PAYLOADS, [^\n]*\]);/.exec(read('render/venn-security.test.ts'));
  if (security === null) throw new Error('render/venn-security.test.ts no longer mutates the venn corpus');
  const fragments = new Function('PAYLOADS', 'return ' + security[1])(PAYLOADS);
  const next = mutator(loadCorpus('venn', /venn-beta/), fragments, random(3));
  for (let i = 0; i < 4000; i++) {
    const src = next();
    if (src.length <= 4000) keep(src);
  }
}

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/venn'), { recursive: true });
const out = join(root, 'tests/fixtures/venn/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated] }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources and ${mutated.size} mutated inputs written to ${relative(root, out)}`);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
