// Collects the Mermaid sources of the diagram types that import Pele's shared helpers into
// tests/fixtures/common/harvest.json, so that crates/pele-core/tests/common.rs can render each
// through Pele, see every input those types give the helpers, and put the same inputs through
// this port.
//
//   mise exec -- node scripts/harvest-common.mjs
//
// `sources` are each importing type's documentation corpus, then the string literals that hold a
// line break in that type's folders of vendor/pele/tests, Pele's own and the Mermaid specs under
// compat/, and in its tests/support/<type>-oracle.ts.
// Rerunning at the same Pele pin writes the same file.
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join, relative } from 'node:path';

const root = join(import.meta.dirname, '..');
const pele = join(root, 'vendor/pele');
const tests = join(pele, 'tests');

// The types that import literal-label, outline, icon, scan, fit, fit-text, fit-width's capWidth,
// accDescr, caseless or the palette.
const TYPES = [
  'c4', 'eventmodeling', 'gantt', 'git', 'ishikawa', 'journey', 'kanban', 'mindmap', 'packet', 'quadrant',
  'radar', 'sequence', 'timeline', 'treemap', 'treeview', 'venn', 'wardley', 'xychart',
];

function files(dir) {
  return readdirSync(dir, { withFileTypes: true })
    .sort((a, b) => (a.name < b.name ? -1 : 1))
    .flatMap((entry) =>
      entry.isDirectory() ? files(join(dir, entry.name)) : /\.[jt]s$/.test(entry.name) ? [join(dir, entry.name)] : []
    );
}

// A string literal's value: a `${...}` stays as written.
function value(raw) {
  try {
    return new Function('return `' + raw.replace(/`/g, '\\`').replace(/\$\{/g, '\\${') + '`')();
  } catch {
    return undefined;
  }
}

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

for (const type of TYPES) {
  for (const text of JSON.parse(readFileSync(join(tests, `corpus/${type}-docs.json`), 'utf8'))) add(text, `corpus/${type}-docs.json`);
  const support = join(tests, `support/${type}-oracle.ts`);
  const folders = [join(tests, type), join(tests, 'compat', type)].filter((folder) => existsSync(folder));
  for (const file of [...folders.flatMap(files), ...(existsSync(support) ? [support] : [])]) {
    const from = relative(tests, file);
    for (const match of readFileSync(file, 'utf8').matchAll(RE_LITERAL)) {
      const text = value(match[1] ?? match[2] ?? match[3]);
      if (text !== undefined && text.includes('\n')) add(text, from);
    }
  }
}

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/common'), { recursive: true });
const out = join(root, 'tests/fixtures/common/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, types: TYPES, sources: [...sources] }, null, 1) + '\n');

for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
console.log(`${sources.size} sources written to ${relative(root, out)}`);
console.log(`${illFormed} left out: a lone surrogate cannot cross into Rust as a string`);
