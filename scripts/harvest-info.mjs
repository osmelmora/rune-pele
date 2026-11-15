// Collects every info diagram source in Pele's test suite into tests/fixtures/info/harvest.json, so
// that crates/pele-core/tests/info_front.rs and info_parity.rs can put each one through Pele and
// through this port.
//
//   mise exec -- node scripts/harvest-info.mjs
//
// `sources` are the corpus of tests/info/oracle.test.ts: the documentation and spec inputs, the
// string literals of Pele's ported info parser tests, and the test's own list. `mutated` are that
// test's mutations of them (seed 7, 3000 inputs of at most 2000 characters). Rerunning at the same
// Pele pin writes the same file.
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { loadCorpus, mutator, random } from '../vendor/pele/tests/support/corpus.ts';

const root = join(import.meta.dirname, '..');
const pele = join(root, 'vendor/pele');
const test = readFileSync(join(pele, 'tests/info/oracle.test.ts'), 'utf8');

// String literals in the ported Langium parser tests, as tests/support/langium-extra.ts reads them.
function loadTestStrings(diagram, keyword) {
  const seeds = new Set();
  const dir = join('tests/compat', diagram);
  for (const file of readdirSync(dir).filter((f) => /\.test\.[jt]s$/.test(f))) {
    const src = readFileSync(join(dir, file), 'utf8');
    for (const m of src.matchAll(/`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"/g)) {
      const raw = m[1] ?? m[2] ?? m[3];
      if (!keyword.test(raw)) continue;
      try {
        seeds.add(new Function('return `' + raw.replace(/`/g, '\\`').replace(/\$\{/g, '\\${') + '`')());
      } catch {
        continue;
      }
    }
  }
  return [...seeds];
}

function constant(name) {
  const found = new RegExp(`const ${name} = (\\[[\\s\\S]*?\\n\\]);`).exec(test);
  if (found === null) throw new Error(`tests/info/oracle.test.ts no longer declares ${name}`);
  return found[1];
}

process.chdir(pele);
const corpus = new Function('loadCorpus', 'loadTestStrings', 'return ' + constant('corpus'))(loadCorpus, loadTestStrings);
const FRAGMENTS = new Function('return ' + constant('FRAGMENTS'))();

const sources = [...new Set(corpus)];
const known = new Set(sources);
const mutated = new Set();
const next = mutator(corpus, FRAGMENTS, random(7));
for (let i = 0; i < 3000; i++) {
  const text = next();
  if (text.length <= 2000 && !known.has(text) && text.isWellFormed()) mutated.add(text);
}

const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
mkdirSync(join(root, 'tests/fixtures/info'), { recursive: true });
const out = join(root, 'tests/fixtures/info/harvest.json');
writeFileSync(out, JSON.stringify({ pele: pin, sources, mutated: [...mutated] }, null, 1) + '\n');
console.log(`${sources.length} sources and ${mutated.size} mutated inputs written to ${relative(root, out)}`);
