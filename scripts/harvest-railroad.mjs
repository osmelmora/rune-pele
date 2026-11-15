// Collects every railroad diagram source in Pele's test suite into tests/fixtures/railroad/harvest.json,
// so that crates/pele-core/tests/railroad_front.rs and railroad_parity.rs can put each one through
// Pele and through this port.
//
//   mise exec -- node scripts/harvest-railroad.mjs
//
// `sources` are:
// - the string literals of vendor/pele/tests that have a line starting with a railroad keyword;
// - the inputs that tests/railroad/model.test.ts builds with its `ir`, `ebnf`, `abnf` and `peg`
//   helpers, and tests/render/railroad.test.ts with its `ebnf`, `abnf` and `withConfig` helpers;
// - each notation's corpus as tests/support/railroad-oracle.ts gives it, and the grammars
//   tests/render/railroad.test.ts generates (seed 11, 80 per notation);
// - each template of tests/render/railroad-security.test.ts filled with each hostile payload.
//
// `mutated` are the inputs Pele's railroad tests derive from those: the mutated, pieced and
// generated inputs of tests/railroad/oracle.test.ts (seed 7, 1500 per notation, at most 2000
// characters) and of tests/render/railroad-security.test.ts (seed 3, 600 per notation, at most
// 4000). Pele's generators run as Pele wrote them, bundled by the harness's esbuild with the
// reference parser stubbed out, since only their inputs are needed. Rerunning at the same Pele
// pin writes the same file.
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join, relative } from 'node:path';

const root = join(import.meta.dirname, '..');
const pele = join(root, 'vendor/pele');
const tests = join(pele, 'tests');
const { build, transform } = createRequire(join(root, 'harness/package.json'))('esbuild');

const STUBS = {
  '@mermaid-js/parser': 'export {};',
  './token-patch.js': 'export function patchTokens() {}\nexport function patchTitle() {}',
  vitest: 'export function expect() { return {}; }\nexport function describe() {}\nexport function it() {}',
};
const stubs = {
  name: 'stubs',
  setup(b) {
    b.onResolve({ filter: /^(@mermaid-js\/parser|\.\/token-patch\.js|vitest)$/ }, (args) => ({ path: args.path, namespace: 'stub' }));
    b.onLoad({ filter: /.*/, namespace: 'stub' }, (args) => ({ contents: STUBS[args.path], loader: 'js' }));
  },
};
const bundle = await build({
  stdin: {
    contents: [
      "export { NOTATIONS, corpusFor, grammar, pieces } from './tests/support/railroad-oracle.ts';",
      "export { mutator, random } from './tests/support/corpus.ts';",
      "export { PAYLOADS } from './tests/support/inert.ts';",
    ].join('\n'),
    resolveDir: pele,
    loader: 'ts',
  },
  bundle: true,
  write: false,
  format: 'esm',
  platform: 'node',
  plugins: [stubs],
});
const { NOTATIONS, corpusFor, grammar, pieces, mutator, random, PAYLOADS } = await import(
  'data:text/javascript;base64,' + Buffer.from(bundle.outputFiles[0].text).toString('base64')
);

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

const LITERAL = String.raw`\`((?:[^\`\\]|\\.)*)\`|'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"`;
const RE_LITERAL = new RegExp(LITERAL, 'g');
const startsRailroad = (text) => /(?:^|\n)\s*railroad-(?:ebnf-|abnf-|peg-)?beta/i.test(text);

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
    if (found !== undefined && startsRailroad(found)) add(found, from);
  }
}

// The helpers of the model and render tests, each called with one string literal.
function helperCalls(file, helpers) {
  const text = read(file);
  for (const [name, make] of Object.entries(helpers)) {
    for (const match of text.matchAll(new RegExp(String.raw`\b${name}\((?:${LITERAL})\)`, 'g'))) {
      const found = value(match[1] ?? match[2] ?? match[3]);
      if (found !== undefined) add(make(found), `${file} ${name}()`);
    }
  }
}
helperCalls('railroad/model.test.ts', {
  ir: (expression) => `railroad-beta\nr = ${expression} ;`,
  ebnf: (expression) => `railroad-ebnf-beta\nr = ${expression} ;`,
  abnf: (expression) => `railroad-abnf-beta\nr = ${expression} ;`,
  peg: (expression) => `railroad-peg-beta\nR <- ${expression} ;`,
});
const configured = /const src = '([^']*)';\s*const withConfig/.exec(read('render/railroad.test.ts'));
if (configured === null) throw new Error('render/railroad.test.ts no longer configures one source');
helperCalls('render/railroad.test.ts', {
  ebnf: (rules) => 'railroad-ebnf-beta\n' + rules,
  abnf: (rule) => 'railroad-abnf-beta\n' + rule,
  withConfig: (config) => `---\nconfig:\n  railroad:\n${config}\n---\n${value(configured[1])}`,
});

// Each notation's corpus, and the grammars of tests/render/railroad.test.ts.
process.chdir(pele);
for (const notation of NOTATIONS) {
  for (const text of corpusFor(notation).corpus) add(text, `railroad-oracle.ts corpus (${notation.name})`);
  const rnd = random(11);
  for (let i = 0; i < 80; i++) add(grammar(notation, rnd), `render/railroad.test.ts grammars (${notation.name})`);
}

// The templates of tests/render/railroad-security.test.ts, each with each payload.
const security = read('render/railroad-security.test.ts');
const templates = /(const quoted[\s\S]*?\nconst TEMPLATES[\s\S]*?\n\];)/.exec(security);
if (templates === null) throw new Error('render/railroad-security.test.ts no longer declares TEMPLATES');
const { code } = await transform(templates[1], { loader: 'ts' });
const TEMPLATES = new Function(code + '\nreturn TEMPLATES;')();
for (const template of TEMPLATES) {
  for (const payload of PAYLOADS) add(template(payload), 'render/railroad-security.test.ts templates');
}

const mutated = new Set();
let illFormedMutations = 0;
function keep(text) {
  if (sources.has(text)) return;
  if (text.isWellFormed()) mutated.add(text);
  else illFormedMutations++;
}

// tests/railroad/oracle.test.ts: a quarter each of mutated corpus inputs, random token pieces,
// generated grammars and mutated generated grammars.
for (const notation of NOTATIONS) {
  const rnd = random(7);
  const next = mutator(corpusFor(notation).corpus, notation.fragments, rnd);
  for (let i = 0; i < 1500; i++) {
    const kind = i % 4;
    let src = kind === 0 ? next() : kind === 1 ? pieces(notation, rnd) : grammar(notation, rnd);
    if (kind === 3) src = mutator([src], notation.fragments, rnd)();
    if (src.length <= 2000) keep(src);
  }
}
// tests/render/railroad-security.test.ts: mutations with the hostile payloads among the fragments.
for (const notation of NOTATIONS) {
  const rnd = random(3);
  const fragments = [...PAYLOADS, ...notation.fragments];
  const next = mutator(corpusFor(notation).corpus, fragments, rnd);
  for (let i = 0; i < 600; i++) {
    const src = i % 3 === 0 ? next() : mutator([grammar(notation, rnd)], fragments, rnd)();
    if (src.length <= 4000) keep(src);
  }
}

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/railroad'), { recursive: true });
const out = join(root, 'tests/fixtures/railroad/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated] }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources and ${mutated.size} mutated inputs written to ${relative(root, out)}`);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
