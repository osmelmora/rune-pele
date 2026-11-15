// Collects every timeline source in Pele's test suite into tests/fixtures/timeline/harvest.json,
// so that crates/pele-core/tests/timeline_front.rs and timeline_parity.rs can put each one through
// Pele and through this port.
//
//   mise exec -- node scripts/harvest-timeline.mjs
//
// `sources` are the string literals of vendor/pele/tests that have a line starting with
// `timeline`; every template of tests/render/timeline-security.test.ts with every hostile
// payload; and the corpus and seeds of Pele's timeline parser test. `mutated` are the inputs
// Pele's timeline tests derive from those: the mutations of tests/timeline/oracle.test.ts and of
// tests/render/timeline-security.test.ts. `worst` are the sources of
// tests/timeline/stress.test.ts, which Pele renders with no limit on the source's length.
// Rerunning at the same Pele pin writes the same file.
import { loadCorpus } from '../vendor/pele/tests/support/corpus.ts';
import { PAYLOADS, constant, evaluate, harvest, pele, read } from './harvest-lib.mjs';

const out = harvest('timeline');
out.literals((text) => /(?:^|\n)\s*timeline/.test(text));

// tests/render/timeline-security.test.ts: each template with each payload.
const security = 'render/timeline-security.test.ts';
{
  const { TEMPLATES } = evaluate(security, 'const TEMPLATES', "describe('inert timeline output'", ['TEMPLATES']);
  for (const payload of PAYLOADS) for (const template of TEMPLATES) out.add(template(payload), `${security} templates`);
}

// tests/timeline/oracle.test.ts: the spec and documentation corpus and the seeds it adds.
process.chdir(pele);
const oracleTest = 'timeline/oracle.test.ts';
const { corpus } = evaluate(oracleTest, 'const SEEDS = [', 'const expecting', ['corpus'], { loadCorpus });
for (const text of corpus) out.add(text, `${oracleTest} corpus`);

// The mutations of tests/timeline/oracle.test.ts (seed 7, 5000 inputs of at most 4000
// characters) and of tests/render/timeline-security.test.ts (seed 3, 4000 of at most 4000).
out.mutate(corpus, new Function('return ' + constant(oracleTest, 'FRAGMENTS'))(), 7, 5000, 4000);
const fragments = /mutator\(corpus, (\[\.\.\.PAYLOADS, [^\n]*\]), random\(3\)\)/.exec(read(security));
if (fragments === null) throw new Error(`${security} no longer mutates the corpus`);
out.mutate(loadCorpus('timeline', /timeline/), new Function('PAYLOADS', 'return ' + fragments[1])(PAYLOADS), 3, 4000, 4000);

// tests/timeline/stress.test.ts: the source of every worst case, read by stand-ins for Pele's
// render and lexer that hand back what they are given.
{
  const given = (text) => text;
  const { CASES } = evaluate('timeline/stress.test.ts', 'const N = ', "describe('timeline worst cases'", ['CASES'], {
    metricsMeasurer: undefined,
    render: given,
    tokenize: given,
  });
  for (const [, run] of CASES) out.worst(run());
}

out.write();
