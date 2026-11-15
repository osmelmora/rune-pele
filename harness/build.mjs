import { build } from 'esbuild';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';

const here = import.meta.dirname;
// The stamp covers every harness/*.ts in byte order of name, so a per-type op file counts too.
const sources = readdirSync(here).filter((name) => name.endsWith('.ts')).sort();
const hash = createHash('sha256');
for (const name of sources) hash.update(readFileSync(join(here, name)));
const source = hash.digest('hex');
const pele = execFileSync('git', ['-C', `${here}/../vendor/pele`, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();

// Pele's layout entry points, which oracle.ts can watch during a real render. Every import of
// one of these modules gets a shim instead, whose functions hand the call to
// `globalThis.peleLayoutTap[name](real, args)` while a tap is set, and call Pele's own otherwise.
const TAPPED = {
  compound: ['compoundLayout', 'shiftLayout', 'inheritCrossedDirections'],
  components: ['wrapComponents'],
  layered: ['layered'],
};
const layoutDir = resolve(here, '../vendor/pele/src/layout');
const layoutTap = {
  name: 'layout-tap',
  setup(b) {
    b.onResolve({ filter: /(^|\/)(compound|layered|components)\.js$/ }, (args) => {
      const name = basename(args.path, '.js');
      if (args.namespace === 'layout-tap') return { path: join(layoutDir, `${name}.ts`) };
      if (resolve(args.resolveDir, args.path) !== join(layoutDir, `${name}.js`)) return undefined;
      return { path: name, namespace: 'layout-tap' };
    });
    b.onLoad({ filter: /.*/, namespace: 'layout-tap' }, (args) => ({
      resolveDir: here,
      loader: 'js',
      contents: [
        `import * as real from './${args.path}.js';`,
        `export * from './${args.path}.js';`,
        `const tap = (name) => (...args) => { const t = globalThis.peleLayoutTap; return t && t[name] ? t[name](real[name], args) : real[name](...args); };`,
        ...TAPPED[args.path].map((name) => `export const ${name} = tap('${name}');`),
      ].join('\n'),
    }));
  },
};

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
  plugins: [layoutTap],
  banner: { js: `// harness ts sha256 ${source} pele ${pele}` },
});
