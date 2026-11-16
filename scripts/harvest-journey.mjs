// Collects every journey source in Pele's test suite into tests/fixtures/journey/harvest.json,
// so that crates/pele-core/tests/journey_front.rs and journey_parity.rs can put each one through
// Pele and through this port.
//
//   mise exec -- node scripts/harvest-journey.mjs
//
// `sources` are the string literals of vendor/pele/tests that have a line starting with
// `journey`; every template of tests/render/journey-security.test.ts with every hostile payload
// and its variant without semicolons; the corpus of that test; and the corpus and seeds of Pele's
// journey parser test. `mutated` are the inputs Pele's journey tests derive from those: the
// mutations of tests/journey/oracle.test.ts and of tests/render/journey-security.test.ts. `worst`
// are the sources of tests/journey/stress.test.ts, which Pele renders with no limit on the
// source's length. Rerunning at the same Pele pin writes the same file.
import { loadCorpus } from '../vendor/pele/tests/support/corpus.ts';
import { PAYLOADS, constant, evaluate, harvest, pele, read } from './harvest-lib.mjs';

const out = harvest('journey');
out.literals((text) => /(?:^|\n)\s*journey/.test(text));

// tests/render/journey-security.test.ts: each template with each payload and its variants, and
// the corpus the test adds.
process.chdir(pele);
const security = 'render/journey-security.test.ts';
const { variants, TEMPLATES } = evaluate(security, 'const variants', "describe('inert journey output'", [
  'variants',
  'TEMPLATES',
]);
for (const payload of PAYLOADS) {
  for (const text of variants(payload)) for (const template of TEMPLATES) out.add(template(text), `${security} templates`);
}
const { corpus: inert } = evaluate(security, '  const corpus = [', "  it('holds for every corpus input'", ['corpus'], {
  loadCorpus,
});
for (const text of inert) out.add(text, `${security} corpus`);

// tests/journey/oracle.test.ts: the spec and documentation corpus and the seeds it adds.
const oracleTest = 'journey/oracle.test.ts';
const { corpus } = evaluate(oracleTest, 'const SEEDS = [', 'const expecting', ['corpus'], { loadCorpus });
for (const text of corpus) out.add(text, `${oracleTest} corpus`);

// The mutations of tests/journey/oracle.test.ts (seed 7, 5000 inputs of at most 4000
// characters) and of tests/render/journey-security.test.ts (seed 3, 4000 of at most 4000).
out.mutate(corpus, new Function('return ' + constant(oracleTest, 'FRAGMENTS'))(), 7, 5000, 4000);
const fragments = /mutator\(\s*corpus,\s*(\[\.\.\.PAYLOADS\.flatMap\(variants\), [^\n]*\]),\s*random\(3\)\s*\)/.exec(read(security));
if (fragments === null) throw new Error(`${security} no longer mutates the corpus`);
out.mutate(inert, new Function('PAYLOADS', 'variants', 'return ' + fragments[1])(PAYLOADS, variants), 3, 4000, 4000);

// tests/journey/stress.test.ts: the source of every worst case, read by stand-ins for Pele's
// render and lexer that hand back what they are given.
{
  const given = (text) => text;
  const { CASES } = evaluate('journey/stress.test.ts', 'const N = ', "describe('journey worst cases'", ['CASES'], {
    metricsMeasurer: undefined,
    render: given,
    tokenize: given,
  });
  for (const [, run] of CASES) out.worst(run());
}

out.write();
