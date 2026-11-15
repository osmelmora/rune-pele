// Collects every use case source in Pele's test suite into tests/fixtures/usecase/harvest.json,
// so that crates/pele-core/tests/usecase_front.rs and usecase_parity.rs can run each one through
// Pele and through this port.
//
//   mise exec -- node scripts/harvest-usecase.mjs
//
// `sources` are:
//   - the string literals of vendor/pele/tests that have a line starting `usecase-beta`, with
//     each `${p}` or `${payload}` filled in from Pele's hostile payload list;
//   - the literals of the use case specs and tests that hold a statement without the header,
//     with the header put back, as Pele's own `specCorpus` does;
//   - every template of tests/render/usecase-security.test.ts with every payload;
//   - the seeds and the rejected inputs of tests/usecase/oracle.test.ts, and the documentation corpus.
// `mutated` are the inputs Pele's use case tests derive from those with their seeded generators.
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

// The array a test file assigns to `name`, as source text.
function constant(file, name) {
  const text = readFileSync(join(tests, file), 'utf8');
  const found = new RegExp(`const ${name}(?:: [^\\n]*?)? = (\\[[\\s\\S]*?\\n\\s*\\]);`).exec(text);
  if (found === null) throw new Error(`${file} no longer declares ${name}`);
  return found[1];
}

const HEADER = 'usecase-beta\n';
const startsUsecase = (text) => /(?:^|\n)\s*usecase-beta/.test(text);

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

const ownFiles = /^(?:compat\/usecase(?:-parser)?\/|render\/usecase|usecase\/)/;
for (const file of files(tests)) {
  const from = relative(tests, file);
  for (const match of readFileSync(file, 'utf8').matchAll(RE_LITERAL)) {
    const raw = match[1] ?? match[2] ?? match[3];
    const text = value(raw);
    if (text === undefined) continue;
    if (startsUsecase(text)) {
      add(text, from);
      if (RE_TEMPLATE.test(text)) {
        for (const payload of PAYLOADS) add(text.replace(RE_TEMPLATE, () => payload), from);
      }
    } else if (ownFiles.test(from) && !raw.includes('${') && !raw.includes('./') && /\W/.test(raw) && text.length <= 2000) {
      // A statement the tests write without the header and add it in a helper.
      add(HEADER + text, `${from} (header added)`);
    }
  }
}

const TEMPLATES = new Function('U', 'return ' + constant('render/usecase-security.test.ts', 'TEMPLATES'))(HEADER);
for (const template of TEMPLATES) {
  for (const payload of PAYLOADS) add(template(payload), 'render/usecase-security.test.ts templates');
}

const FRAGMENTS = new Function('return ' + constant('usecase/oracle.test.ts', 'FRAGMENTS'))();
const SEEDS = new Function('return ' + constant('usecase/oracle.test.ts', 'SEEDS'))();
const REJECTED = new Function('return ' + constant('usecase/oracle.test.ts', 'REJECTED'))();
const JSON_SEEDS = new Function('return ' + constant('usecase/oracle.test.ts', 'JSON_SEEDS'))();
const JSON_FRAGMENTS = new Function('return ' + constant('usecase/oracle.test.ts', 'JSON_FRAGMENTS'))();
const OPENERS = new Function('return ' + constant('usecase/oracle.test.ts', 'openers'))();
for (const text of [...SEEDS, ...REJECTED]) add(text, 'usecase/oracle.test.ts seeds');

// Pele's use case corpus reads paths relative to its own checkout.
process.chdir(pele);
const docs = loadCorpus('usecase', /usecase-beta/);
for (const text of docs) add(text, 'loadCorpus(usecase)');

// Pele's `specCorpus`: the literals of the ported specs, with the header put back where a spec
// leaves it to a helper.
const spec = [];
for (const dir of ['tests/compat/usecase', 'tests/compat/usecase-parser']) {
  for (const file of readdirSync(dir).filter((name) => name.endsWith('.spec.ts')).sort()) {
    for (const match of readFileSync(join(dir, file), 'utf8').matchAll(/`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)'/g)) {
      const raw = match[1] ?? match[2];
      if (raw.includes('${') || raw.includes('./') || !/\W/.test(raw)) continue;
      let text;
      try {
        text = new Function('return `' + raw.replace(/`/g, '\\`') + '`')();
      } catch {
        continue;
      }
      if (text.length > 2000) continue;
      spec.push(/^\s*usecase-beta/.test(text) ? text : HEADER + text);
    }
  }
}
for (const text of spec) add(text, 'specCorpus');

const mutated = new Set();
let illFormedMutations = 0;
function derived(text) {
  if (text.length > 4000 || sources.has(text)) return;
  if (text.isWellFormed()) mutated.add(text);
  else illFormedMutations++;
}

// tests/usecase/oracle.test.ts, 'agrees on mutated inputs': seed 7, 3000 inputs, every sixth cut
// off at a random place.
const corpus = [...new Set([...docs, ...spec, ...SEEDS, ...REJECTED])];
{
  const rnd = random(7);
  const next = mutator(corpus, FRAGMENTS, rnd);
  for (let i = 0; i < 3000; i++) {
    let text = next();
    if (i % 6 === 0) text = text.slice(0, Math.floor(rnd() * (text.length + 1)));
    derived(text);
  }
}
// 'locates invalid JSON alike when the engine does not': seed 9, 1500 inputs.
{
  const next = mutator(JSON_SEEDS, JSON_FRAGMENTS, random(9));
  for (let i = 0; i < 1500; i++) derived(`usecase-beta\njson P@${next()}\n`);
}
// 'agrees on random sequences of tokens': seed 8, 3000 inputs.
{
  const rnd = random(8);
  const pieces = [...new Set([...FRAGMENTS, ...FRAGMENTS.map((f) => f.trim()).filter((f) => f !== '')])];
  for (let i = 0; i < 3000; i++) {
    let text = OPENERS[Math.floor(rnd() * OPENERS.length)];
    for (let k = 1 + Math.floor(rnd() * 10); k > 0; k--) text += pieces[Math.floor(rnd() * pieces.length)] + (rnd() < 0.3 ? ' ' : '');
    derived(text);
  }
}
// tests/render/usecase-security.test.ts, 'holds for mutated inputs': seed 3, 4000 inputs.
{
  const security = /const fragments = (\[\.\.\.PAYLOADS, [^\n]*\]);/.exec(
    readFileSync(join(tests, 'render/usecase-security.test.ts'), 'utf8')
  );
  if (security === null) throw new Error('render/usecase-security.test.ts no longer mutates the use case corpus');
  const fragments = new Function('PAYLOADS', 'return ' + security[1])(PAYLOADS);
  const next = mutator(docs, fragments, random(3));
  for (let i = 0; i < 4000; i++) derived(next());
}

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/usecase'), { recursive: true });
const out = join(root, 'tests/fixtures/usecase/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated] }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources and ${mutated.size} mutated inputs written to ${relative(root, out)}`);
console.log(
  `${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`
);
