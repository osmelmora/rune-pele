// Collects every radar source in Pele's test suite into tests/fixtures/radar/harvest.json, so that
// crates/pele-core/tests/radar_front.rs and radar_parity.rs can put each one through Pele and
// through this port.
//
//   mise exec -- node scripts/harvest-radar.mjs
//
// The sources and mutations are those `scripts/harvest-standalone.mjs` describes, from
// tests/radar/oracle.test.ts (its own corpus, mutation seed 7, 3000 inputs) and
// tests/render/radar-security.test.ts (its own fragments, seed 3, 4000 inputs).
import { constant, harvest, loadTestStrings, read } from './harvest-standalone.mjs';
import { loadCorpus } from '../vendor/pele/tests/support/corpus.ts';

const oracle = 'radar/oracle.test.ts';

function corpus() {
  const headless = /const HEADLESS = (\/.*\/);/.exec(read(oracle));
  if (headless === null) throw new Error(`${oracle} no longer declares HEADLESS`);
  const HEADLESS = new Function('return ' + headless[1])();
  return new Function('loadCorpus', 'loadTestStrings', 'HEADLESS', 'return ' + constant(oracle, 'corpus'))(loadCorpus, loadTestStrings, HEADLESS);
}

function hostile(PAYLOADS) {
  const found = /const fragments = (\[\.\.\.PAYLOADS, [^\n]*\]);/.exec(read('render/radar-security.test.ts'));
  if (found === null) throw new Error('render/radar-security.test.ts no longer mutates the corpus');
  return new Function('PAYLOADS', 'return ' + found[1])(PAYLOADS);
}

harvest({
  type: 'radar',
  keyword: 'radar-beta',
  compat: 'radar',
  pattern: /radar/,
  oracle,
  security: 'render/radar-security.test.ts',
  oracleRun: { seed: 7, count: 3000 },
  securityRun: { seed: 3, count: 4000 },
  corpus,
  hostile,
});
