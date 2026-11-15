// Collects every tree view source in Pele's test suite into tests/fixtures/treeview/harvest.json,
// so that crates/pele-core/tests/treeview_front.rs and treeview_parity.rs can put each one through
// Pele and through this port.
//
//   mise exec -- node scripts/harvest-treeview.mjs
//
// The sources and mutations are those `scripts/harvest-standalone.mjs` describes, from
// tests/treeview/oracle.test.ts (mutation seed 7, 3000 inputs) and
// tests/render/treeview-security.test.ts (seed 11, 2000 inputs), with that test's sources whose
// names are object keys.
import { harvest } from './harvest-standalone.mjs';

const keys = ['__proto__', 'constructor', 'toString', 'hasOwnProperty', 'valueOf'].map((key) => [
  `---\nconfig:\n  treeView:\n    showIcons: true\n    defaultIconPack: p\n---\ntreeView-beta\n  ${key}\n  x.${key}\n  y icon(${key})\n`,
  'render/treeview-security.test.ts object keys',
]);

harvest({
  type: 'treeview',
  keyword: 'treeView-beta',
  compat: 'treeview',
  pattern: /treeView-beta/,
  oracle: 'treeview/oracle.test.ts',
  security: 'render/treeview-security.test.ts',
  seeds: ['VALID', 'INVALID'],
  oracleRun: { seed: 7, count: 3000 },
  securityRun: { seed: 11, count: 2000 },
  extra: keys,
});
