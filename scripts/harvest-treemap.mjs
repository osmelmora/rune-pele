// Collects every treemap source in Pele's test suite into tests/fixtures/treemap/harvest.json, so
// that crates/pele-core/tests/treemap_front.rs and treemap_parity.rs can put each one through Pele
// and through this port.
//
//   mise exec -- node scripts/harvest-treemap.mjs
//
// `sources` are the string literals of vendor/pele/tests that have a line starting with
// `treemap`; every template of tests/render/treemap-security.test.ts with every hostile payload;
// and the corpus and seeds of Pele's treemap parser test. `mutated` are the inputs Pele's treemap
// tests derive from those: the mutations of tests/treemap/oracle.test.ts and of
// tests/render/treemap-security.test.ts. `worst` are the sources of tests/treemap/stress.test.ts,
// which Pele reads with no limit on the source's length. Rerunning at the same Pele pin writes the
// same file.
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { loadCorpus } from '../vendor/pele/tests/support/corpus.ts';
import { PAYLOADS, RE_LITERAL, constant, evaluate, harvest, pele, read, value } from './harvest-lib.mjs';

// Pele's loadTestStrings (tests/support/langium-extra.ts), which imports Mermaid's own parser and
// so cannot load here: the literals of tests/compat/<diagram>/*.test.ts that the keyword finds.
function loadTestStrings(diagram, keyword) {
  const seeds = new Set();
  const dir = join('tests/compat', diagram);
  for (const file of readdirSync(dir).filter((f) => /\.test\.[jt]s$/.test(f))) {
    for (const m of readFileSync(join(dir, file), 'utf8').matchAll(RE_LITERAL)) {
      const raw = m[1] ?? m[2] ?? m[3];
      if (!keyword.test(raw)) continue;
      const found = value(raw);
      if (found !== undefined) seeds.add(found);
    }
  }
  return [...seeds];
}

const out = harvest('treemap');
out.literals((text) => /(?:^|\n)\s*treemap/.test(text));

// tests/render/treemap-security.test.ts: each template with each payload.
const security = 'render/treemap-security.test.ts';
{
  const { TEMPLATES } = evaluate(security, 'const TEMPLATES', "describe('inert treemap output'", ['TEMPLATES']);
  for (const payload of PAYLOADS) for (const template of TEMPLATES) out.add(template(payload), `${security} templates`);
}

// tests/treemap/oracle.test.ts: the spec and documentation corpus and the seeds it adds.
process.chdir(pele);
const oracleTest = 'treemap/oracle.test.ts';
const { corpus } = evaluate(oracleTest, 'const corpus = [', 'const referenceLex', ['corpus'], {
  loadCorpus,
  loadTestStrings,
});
for (const text of corpus) out.add(text, `${oracleTest} corpus`);

// The mutations of tests/treemap/oracle.test.ts (seed 7, 3000 inputs of at most 2000 characters)
// and of tests/render/treemap-security.test.ts (seed 3, 4000 of at most 4000).
out.mutate(corpus, new Function('return ' + constant(oracleTest, 'FRAGMENTS'))(), 7, 3000, 2000);
const fragments = /const fragments = (\[\.\.\.PAYLOADS, [^\n]*\]);/.exec(read(security));
if (fragments === null || !/mutator\(corpus, fragments, random\(3\)\)/.test(read(security))) {
  throw new Error(`${security} no longer mutates the corpus`);
}
out.mutate(loadCorpus('treemap', /treemap/), new Function('PAYLOADS', 'return ' + fragments[1])(PAYLOADS), 3, 4000, 4000);

// tests/treemap/stress.test.ts: the source of every worst case, read by stand-ins for Pele's
// render, parse and parser that hand back what they are given.
{
  const given = (text) => text;
  const { CASES } = evaluate('treemap/stress.test.ts', 'const N = ', "describe('treemap worst cases'", ['CASES'], {
    metricsMeasurer: undefined,
    render: given,
    parse: given,
    parseTreemap: given,
  });
  for (const [, run] of CASES) out.worst(run());
}

out.write();
