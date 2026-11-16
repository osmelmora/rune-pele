// Collects every kanban source in Pele's test suite into tests/fixtures/kanban/harvest.json, so
// that crates/pele-core/tests/kanban_front.rs and kanban_parity.rs can put each one through Pele
// and through this port.
//
//   mise exec -- node scripts/harvest-kanban.mjs
//
// `sources` are the string literals of vendor/pele/tests that have a line starting `kanban`, each
// also with a line break after it, then the corpus of Pele's kanban parser test, then each
// template of tests/render/kanban-security.test.ts with each of Pele's hostile payloads.
// `configured` are the ticket links of that test, each a source with the kanban config it is
// rendered with. `mutated` are the inputs of the seeded mutation runs of those two tests. `worst`
// are the cases of tests/kanban/stress.test.ts, each with the config it is rendered with, which
// Pele renders with no limit on the source's length. Rerunning at the same Pele pin writes the
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
  const found = new RegExp(`const ${name}(?::[^=]*)? = (\\[[\\s\\S]*?\\n\\s*\\]);`).exec(read(file));
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

const startsKanban = (text) => /(?:^|\n)\s*kanban/.test(text);
const RE_LITERAL = /`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"/g;
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
    const text = value(match[1] ?? match[2] ?? match[3]);
    if (text === undefined || !startsKanban(text) || text.includes('${')) continue;
    add(text, from);
    add(text + '\n', from);
  }
}

// tests/kanban/oracle.test.ts: the spec and documentation corpus.
process.chdir(pele);
const oracleTest = 'kanban/oracle.test.ts';
const corpus = loadCorpus('kanban', /kanban/);
for (const text of corpus) add(text, `${oracleTest} corpus`);

// tests/render/kanban-security.test.ts: each template with each payload, and the ticket links.
const security = 'render/kanban-security.test.ts';
const { TEMPLATES } = evaluate(security, 'const BASE = ', "describe('inert kanban output'", ['TEMPLATES']);
for (const template of TEMPLATES) for (const payload of PAYLOADS) add(template(payload), `${security} templates`);
const configured = [];
{
  const text = read(security);
  const bases = /for \(const base of (\[[^\n]*\])\)/.exec(text);
  const tickets = /for \(const ticket of (\[[^\n]*\])\)/.exec(text);
  const board = /render\((`kanban[^`]*`), \{ \.\.\.options, config \}\)/.exec(text);
  if (bases === null || tickets === null || board === null) throw new Error(`${security} no longer links tickets`);
  for (const payload of PAYLOADS) {
    for (const base of new Function('payload', 'return ' + bases[1])(payload)) {
      for (const ticket of new Function('return ' + tickets[1])()) {
        const source = new Function('ticket', 'return ' + board[1])(ticket);
        if (source.isWellFormed() && base.isWellFormed()) configured.push({ source, config: { kanban: { ticketBaseUrl: base } } });
      }
    }
  }
}

// The mutation runs of tests/kanban/oracle.test.ts (seed 7, 5000 inputs) and of
// tests/render/kanban-security.test.ts (seed 3, 4000 inputs), each of at most 4000 characters.
const mutated = new Set();
let illFormedMutations = 0;
function mutate(base, fragments, seed, count) {
  const next = mutator(base, fragments, random(seed));
  for (let i = 0; i < count; i++) {
    const text = next();
    if (text.length > 4000 || sources.has(text)) continue;
    if (text.isWellFormed()) mutated.add(text);
    else illFormedMutations++;
  }
}
mutate(corpus, new Function('return ' + constant(oracleTest, 'FRAGMENTS'))(), 7, 5000);
const hostile = /const fragments = (\[\.\.\.PAYLOADS, [^\n]*\]);/.exec(read(security));
if (hostile === null) throw new Error(`${security} no longer mutates the kanban corpus`);
mutate(corpus, new Function('PAYLOADS', 'return ' + hostile[1])(PAYLOADS), 3, 4000);

// tests/kanban/stress.test.ts: the source each case hands the lexer, the parser or the renderer,
// with the config it renders with.
const given = (source, options) => ({ source, config: options?.config ?? null });
const { CASES } = evaluate('kanban/stress.test.ts', 'const N = ', "describe('kanban worst cases'", ['CASES'], {
  metricsMeasurer: undefined,
  tokenize: given,
  parse: given,
  render: given,
});
const worst = CASES.map(([name, run]) => ({ name, ...run() })).filter((item) => item.source.isWellFormed());

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/kanban'), { recursive: true });
const out = join(root, 'tests/fixtures/kanban/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], configured, mutated: [...mutated], worst }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(
  `${sources.size} sources, ${configured.length} configured, ${mutated.size} mutated inputs and ${worst.length} worst cases written to ${relative(root, out)}`
);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
