// What the harvests of eventmodeling, treeview and radar share: every string literal of
// vendor/pele/tests that has a line starting with the type's keyword, each also with a line break
// after it, then the corpus, valid and invalid seeds of the type's parser oracle test, then each
// template of its render security test with each of Pele's hostile payloads, then the type's own
// extra sources. `mutated` are the inputs of the seeded mutation runs of those two tests. Each
// input has at most 4000 characters. Rerunning at the same Pele pin writes the same file.
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

export function read(file) {
  return readFileSync(join(tests, file), 'utf8');
}

export function constant(file, name) {
  const found = new RegExp(`const ${name}[^=\\n]* = (\\[[\\s\\S]*?\\n\\]);`).exec(read(file));
  if (found === null) throw new Error(`${file} no longer declares ${name}`);
  return found[1];
}

const RE_LITERAL = /`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"/g;

// String literals of the ported Langium tests, as tests/support/langium-extra.ts reads them.
export function loadTestStrings(diagram, keyword) {
  const seeds = new Set();
  const dir = join('tests/compat', diagram);
  for (const file of readdirSync(dir).filter((f) => /\.test\.[jt]s$/.test(f))) {
    for (const m of readFileSync(join(dir, file), 'utf8').matchAll(RE_LITERAL)) {
      const raw = m[1] ?? m[2] ?? m[3];
      if (!keyword.test(raw)) continue;
      const text = value(raw);
      if (text !== undefined) seeds.add(text);
    }
  }
  return [...seeds];
}

// `type` names the fixture folder, `keyword` the line a source starts with, `compat` and
// `pattern` Pele's loadCorpus arguments, `oracle` and `security` the two tests, `seeds` the
// constants of the oracle test besides FRAGMENTS, `oracleRun` and `securityRun` the seed and count
// of each mutation run, and `extra` a list of [source, origin] pairs. A test that builds its
// corpus or its hostile fragments its own way gives `corpus()` or `hostile(PAYLOADS)` instead,
// each called from within vendor/pele.
export function harvest({ type, keyword, compat, pattern, oracle, security, seeds = [], oracleRun, securityRun, extra = [], corpus: ownCorpus, hostile: ownHostile }) {
  const sources = new Set();
  const origin = new Map();
  let illFormed = 0;
  const add = (text, from) => {
    if (typeof text !== 'string' || sources.has(text)) return;
    if (!text.isWellFormed()) {
      illFormed++;
      return;
    }
    sources.add(text);
    origin.set(from, (origin.get(from) ?? 0) + 1);
  };
  const startsType = (text) => new RegExp(`(?:^|\\n)\\s*${keyword}`).test(text);

  for (const file of files(tests)) {
    const from = relative(tests, file);
    for (const match of readFileSync(file, 'utf8').matchAll(RE_LITERAL)) {
      const text = value(match[1] ?? match[2] ?? match[3]);
      if (text === undefined || !startsType(text)) continue;
      add(text, from);
      add(text + '\n', from);
    }
  }

  process.chdir(pele);
  const FRAGMENTS = new Function('return ' + constant(oracle, 'FRAGMENTS'))();
  const corpus = ownCorpus
    ? ownCorpus()
    : [...loadCorpus(compat, pattern), ...seeds.flatMap((name) => new Function('return ' + constant(oracle, name))())];
  for (const text of corpus) add(text, `${oracle} corpus`);

  const PAYLOADS = new Function('return ' + constant('support/inert.ts', 'PAYLOADS'))();
  const securityTest = read(security);
  const line = (p) => p.replace(/[\n\r]/g, ' ');
  const templates = /const TEMPLATES[^\n]*= (\[[\s\S]*?\n\]);/.exec(securityTest);
  if (templates === null) throw new Error(`${security} no longer lists its templates`);
  for (const template of new Function('line', 'return ' + templates[1])(line)) {
    for (const payload of PAYLOADS) add(template(payload), `${security} templates`);
  }
  for (const [text, from] of extra) add(text, from);

  const mutated = new Set();
  let illFormedMutations = 0;
  const mutate = (seedsOf, fragments, seed, count) => {
    const next = mutator(seedsOf, fragments, random(seed));
    for (let i = 0; i < count; i++) {
      const text = next();
      if (text.length > 4000 || sources.has(text)) continue;
      if (text.isWellFormed()) mutated.add(text);
      else illFormedMutations++;
    }
  };
  mutate(corpus, FRAGMENTS, oracleRun.seed, oracleRun.count);
  let fragments = ownHostile?.(PAYLOADS);
  if (fragments === undefined) {
    const hostile = new RegExp(`mutator\\(corpus, (\\[\\.\\.\\.PAYLOADS, [^\\n]*\\]), random\\(${securityRun.seed}\\)\\)`).exec(securityTest);
    if (hostile === null) throw new Error(`${security} no longer mutates the corpus`);
    fragments = new Function('PAYLOADS', 'return ' + hostile[1])(PAYLOADS);
  }
  mutate(loadCorpus(compat, pattern), fragments, securityRun.seed, securityRun.count);

  const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
  mkdirSync(join(root, `tests/fixtures/${type}`), { recursive: true });
  const out = join(root, `tests/fixtures/${type}/harvest.json`);
  writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated] }, null, 1) + '\n');

  for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
  console.log(`${sources.size} sources and ${mutated.size} mutated inputs written to ${relative(root, out)}`);
  console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
}
