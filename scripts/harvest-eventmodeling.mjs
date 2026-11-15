// Collects every event modeling source in Pele's test suite into
// tests/fixtures/eventmodeling/harvest.json, so that crates/pele-core/tests/eventmodeling_front.rs
// and eventmodeling_parity.rs can put each one through Pele and through this port.
//
//   mise exec -- node scripts/harvest-eventmodeling.mjs
//
// The sources and mutations are those `scripts/harvest-standalone.mjs` describes, from
// tests/eventmodeling/oracle.test.ts (mutation seed 7, 3000 inputs) and
// tests/render/eventmodeling-security.test.ts (seed 19, 2000 inputs), with that test's sources
// whose names are object keys.
import { harvest } from './harvest-standalone.mjs';

const keys = ['__proto__', 'constructor', 'toString', 'hasOwnProperty', 'valueOf'].map((key) => [
  `eventmodeling\ntf 01 cmd ${key}.${key} [[${key}]]\ntf 02 evt ${key}\ndata ${key} {\n x\n}\nentity ${key}\ngwt 01 given evt ${key} then evt ${key}\n`,
  'render/eventmodeling-security.test.ts object keys',
]);

harvest({
  type: 'eventmodeling',
  keyword: 'eventmodeling',
  compat: 'eventmodeling',
  pattern: /eventmodeling/,
  oracle: 'eventmodeling/oracle.test.ts',
  security: 'render/eventmodeling-security.test.ts',
  seeds: ['VALID', 'INVALID'],
  oracleRun: { seed: 7, count: 3000 },
  securityRun: { seed: 19, count: 2000 },
  extra: keys,
});
