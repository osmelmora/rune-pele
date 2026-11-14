// Collects every pie source in Pele's test suite into tests/fixtures/pie/harvest.json, so that
// crates/pele-core/tests/pie_parity.rs can render each one through Pele and through this port.
//
//   mise exec -- node scripts/harvest-pie.mjs
//
// `sources` are the string literals of vendor/pele/tests that start a pie diagram, with each
// `${payload}` filled in from Pele's hostile payload list, plus the documentation corpus.
// `mutated` are the inputs Pele's own pie tests derive from those with their seeded mutator.
// Rerunning at the same Pele pin writes the same file.
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { mutator, random } from '../vendor/pele/tests/support/corpus.ts';

const root = join(import.meta.dirname, '..');
const tests = join(root, 'vendor/pele/tests');

function files(dir) {
  return readdirSync(dir, { withFileTypes: true })
    .sort((a, b) => (a.name < b.name ? -1 : 1))
    .flatMap((entry) => (entry.isDirectory() ? files(join(dir, entry.name)) : entry.name.endsWith('.ts') ? [join(dir, entry.name)] : []));
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
  const found = new RegExp(`const ${name} = (\\[[\\s\\S]*?\\n\\]);`).exec(text);
  if (found === null) throw new Error(`${file} no longer declares ${name}`);
  return found[1];
}

// Text that Pele would detect as a pie, or that fails before detection with `pie` leading a line.
const startsPie = (text) => /^\s*pie(?![\w.\/-])/.test(text) || /\n[\t ]*pie(?=[\t ]|%%|\n|$)/.test(text);

const PAYLOADS = new Function('return ' + constant('support/inert.ts', 'PAYLOADS'))();
const RE_LITERAL = /`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"/g;

const sources = new Set();
const origin = new Map();
function add(text, from) {
  if (typeof text !== 'string' || !startsPie(text) || !text.isWellFormed() || sources.has(text)) return;
  sources.add(text);
  origin.set(from, (origin.get(from) ?? 0) + 1);
}

for (const file of files(tests)) {
  const from = relative(tests, file);
  for (const match of readFileSync(file, 'utf8').matchAll(RE_LITERAL)) {
    const text = value(match[1] ?? match[2] ?? match[3]);
    if (text === undefined) continue;
    if (text.includes('${payload}')) {
      for (const payload of PAYLOADS) add(text.replaceAll('${payload}', payload), from);
    } else {
      add(text, from);
    }
  }
}
for (const sample of JSON.parse(readFileSync(join(tests, 'corpus/pie-docs.json'), 'utf8'))) add(sample, 'corpus/pie-docs.json');

// The two mutation runs of Pele's pie tests: tests/render/pie.test.ts (seed 5, 2000 inputs) and
// tests/pie/oracle.test.ts (seed 7, 3000 inputs of at most 2000 characters).
const seeds = [...sources];
const FRAGMENTS = new Function('return ' + constant('pie/oracle.test.ts', 'FRAGMENTS'))();
const mutated = new Set();
let illFormed = 0;
function mutate(fragments, seed, count) {
  const next = mutator(seeds, fragments, random(seed));
  for (let i = 0; i < count; i++) {
    const text = next();
    if (text.length > 2000 || sources.has(text)) continue;
    if (text.isWellFormed()) mutated.add(text);
    else illFormed++;
  }
}
mutate([...PAYLOADS, '"', ':', '\n', 'title ', 'showData'], 5, 2000);
mutate(FRAGMENTS, 7, 3000);

const pin = execFileSync('git', ['-C', join(root, 'vendor/pele'), 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
const out = join(root, 'tests/fixtures/pie/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated] }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources and ${mutated.size} mutated inputs written to ${relative(root, out)}`);
console.log(`${illFormed} mutated inputs left out: a lone surrogate cannot cross into Rust as a string`);
