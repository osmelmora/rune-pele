import { build } from 'esbuild';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

const here = import.meta.dirname;
const source = createHash('sha256').update(readFileSync(`${here}/oracle.ts`)).digest('hex');
const pele = execFileSync('git', ['-C', `${here}/../vendor/pele`, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();

await build({
  absWorkingDir: here,
  entryPoints: ['oracle.ts'],
  outfile: 'dist/oracle.mjs',
  bundle: true,
  format: 'esm',
  platform: 'node',
  target: 'node26',
  minify: false,
  logLevel: 'warning',
  banner: { js: `// oracle.ts sha256 ${source} pele ${pele}` },
});
