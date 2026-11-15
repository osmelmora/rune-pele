// Collects every sequence diagram source in Pele's test suite into
// tests/fixtures/sequence/harvest.json, so that crates/pele-core/tests/sequence_front.rs and
// sequence_parity.rs can put each one through Pele and through this port.
//
//   mise exec -- node scripts/harvest-sequence.mjs
//
// `sources` are the string literals of vendor/pele/tests that have a line starting
// `sequenceDiagram`, with each `${p}`, `${payload}` or `${json(p)}` filled in from Pele's hostile
// payload list, then the corpus of Pele's sequence parser test (tests/sequence/oracle.test.ts).
// `mutated` are the inputs Pele's sequence tests derive from that corpus: the two seeded mutation
// runs, the strings of fragments and the generated statements. `worst` are the shapes of Pele's
// worst cases (tests/sequence/stress.test.ts) at sizes the default limits allow. Rerunning at
// the same Pele pin writes the same file.
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

function declared(file, pattern, what) {
  const found = pattern.exec(read(file));
  if (found === null) throw new Error(`${file} no longer declares ${what}`);
  return found[1];
}

const list = (file, name) => declared(file, new RegExp(`const ${name} = (\\[[\\s\\S]*?\\n\\]);`), name);

const startsSequence = (text) => /(?:^|\n)\s*sequenceDiagram/.test(text);

const PAYLOADS = new Function('return ' + list('support/inert.ts', 'PAYLOADS'))();
const RE_LITERAL = /`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"/g;
const RE_TEMPLATE = /\$\{(p|payload|json\(p\))\}/g;

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
    if (text === undefined || !startsSequence(text)) continue;
    add(text, from);
    if (RE_TEMPLATE.test(text)) {
      for (const payload of PAYLOADS) {
        add(text.replace(RE_TEMPLATE, (_, slot) => (slot === 'json(p)' ? JSON.stringify(payload) : payload)), from);
      }
    }
  }
}

// Pele's sequence corpus reads paths relative to its own checkout.
process.chdir(pele);
const corpus = loadCorpus('sequence', /sequenceDiagram/i);
for (const text of corpus) add(text, 'loadCorpus(sequence)');

const mutated = new Set();
let illFormedMutations = 0;
function keep(text) {
  if (sources.has(text)) return;
  if (text.isWellFormed()) mutated.add(text);
  else illFormedMutations++;
}

// tests/sequence/oracle.test.ts: mutations of the corpus (seed 7, 5000 inputs of at most 4000
// characters), strings of fragments (seed 8, 5000) and statements built from the grammar's own
// shapes (seed 9, 5000).
const oracle = 'sequence/oracle.test.ts';
const FRAGMENTS = new Function('return ' + list(oracle, 'FRAGMENTS'))();
const SLOTS = new Function('return ' + declared(oracle, /const SLOTS: [^=]*= (\{[\s\S]*?\n\});/, 'SLOTS'))();
const TEMPLATES = new Function('return ' + declared(oracle, /const TEMPLATES = (\[[\s\S]*?\n\]\.map\([^\n]*\));/, 'TEMPLATES'))();
const SLOT_NAMES = Object.keys(SLOTS);

const next = mutator(corpus, FRAGMENTS, random(7));
for (let i = 0; i < 5000; i++) {
  const text = next();
  if (text.length <= 4000) keep(text);
}

const strung = random(8);
for (let i = 0; i < 5000; i++) {
  let text = strung() < 0.8 ? 'sequenceDiagram\n' : '';
  for (let k = 2 + Math.floor(strung() * 14); k > 0; k--) text += FRAGMENTS[Math.floor(strung() * FRAGMENTS.length)];
  keep(text);
}

const built = random(9);
const pick = (items) => items[Math.floor(built() * items.length)];
for (let i = 0; i < 5000; i++) {
  let text = 'sequenceDiagram\n';
  for (let lines = 1 + Math.floor(built() * 6); lines > 0; lines--) {
    for (const slot of pick(TEMPLATES)) {
      const r = built();
      if (r < 0.06) continue;
      text += pick(SLOTS[r < 0.18 ? pick(SLOT_NAMES) : slot]);
      if (r > 0.94) text += pick(SLOTS[pick(SLOT_NAMES)]);
      if (built() < 0.3) text += ' ';
    }
    text += built() < 0.85 ? '\n' : pick([';', ' \n', '', '\n\n', ' ']);
  }
  keep(text);
}

// tests/render/sequence-security.test.ts: mutations of the corpus with hostile fragments (seed 3,
// 4000 inputs of at most 4000 characters).
const security = declared(
  'render/sequence-security.test.ts',
  /const fragments = (\[\s*\.\.\.PAYLOADS,[\s\S]*?\]);/,
  'the fragments it mutates the sequence corpus with'
);
const hostile = mutator(corpus, new Function('PAYLOADS', 'return ' + security)(PAYLOADS), random(3));
for (let i = 0; i < 4000; i++) {
  const text = hostile();
  if (text.length <= 4000) keep(text);
}

// The shapes of tests/sequence/stress.test.ts that render, each under the 50,000 characters a
// source may hold.
const head = 'sequenceDiagram\n';
const repeat = (count, line) => Array.from({ length: count }, (_, i) => line(i)).join('');
const worst = [
  head + 'A->>B: ' + 'word '.repeat(4000),
  head + 'A->>B: wrap: ' + 'word '.repeat(4000),
  head + 'Note over A: wrap: ' + 'x'.repeat(20000),
  head + 'A->>B: ' + '#35;<br/>'.repeat(2000),
  head + 'participant ' + 'n'.repeat(20000) + '\nA->>B: x',
  head + 'participant A@{ "a": ' + '['.repeat(300) + ' }\nA->>A: x',
  head + 'box rgb(' + '1, '.repeat(30) + ') T\nparticipant A\nend',
  head + 'participant A\nlinks A: {' + repeat(1000, (i) => `"k${i}": "v", `) + '"z": 1}',
  head + 'autonumber ' + '9'.repeat(400) + ' ' + '9'.repeat(400) + '\nA->>B: x\nB->>A: y',
  head + 'A->>B: x\n'.repeat(3000),
  head + repeat(1500, (i) => `participant p${i}\n`),
  head + repeat(600, (i) => `p${i}->>p${599 - i}: m\n`),
  head + repeat(400, (i) => `p${i}->>p${(i * 7) % 400}: a long message ${i}\n`),
  head + repeat(1500, (i) => (i % 2 ? 'alt a\n' : 'rect\n')) + 'A->>B: x\n' + 'end\n'.repeat(1500),
  head + 'alt a\n' + 'else b\n'.repeat(3000) + 'end\n',
  head + 'par a\nA->>B: x\n' + 'and b\nB->>A: y\n'.repeat(2000) + 'end\n',
  head + 'A->>+B: x\n'.repeat(1500) + 'B-->>-A: y\n'.repeat(1500),
  head + 'activate A\n'.repeat(4000),
  head + 'participant A\nparticipant B\nparticipant C\n' + 'Note over A,C: n\n'.repeat(2500),
  head + 'A->>A: x\n'.repeat(4000),
  head + repeat(1000, (i) => `create participant c${i}\n`) + 'Note over c0: n\n'.repeat(500),
  head + repeat(400, (i) => `create participant c${i}\nA->>c${i}: x\ndestroy c${i}\nA-xc${i}: y\n`),
  head + repeat(1000, (i) => `box Aqua Group ${i}\nparticipant b${i}\nend\n`),
  head + 'autonumber\n' + 'A()->>()B: x\n'.repeat(3000),
  head + 'A->>B: x\n' + 'deactivate A\n'.repeat(3000),
  head + '>'.repeat(40000),
];

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/sequence'), { recursive: true });
const out = join(root, 'tests/fixtures/sequence/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources: [...sources], mutated: [...mutated], worst }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources, ${mutated.size} mutated inputs and ${worst.length} worst cases written to ${relative(root, out)}`);
console.log(`${illFormed + illFormedMutations} inputs left out: a lone surrogate cannot cross into Rust as a string`);
