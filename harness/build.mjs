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
  lanes: ['laneLayout'],
};
const layoutDir = resolve(here, '../vendor/pele/src/layout');
const layoutTap = {
  name: 'layout-tap',
  setup(b) {
    b.onResolve({ filter: /(^|\/)(compound|layered|components|lanes)\.js$/ }, (args) => {
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

// The helpers the diagram types share, which harness/common.ts watches the same way during a real
// render, through `globalThis.peleCommonTap['<file>.<name>'](real, args)`.
const COMMON = {
  'diagrams/common/literal-label': ['literalLabel', 'breakLabel', 'leftLabelSvg'],
  'diagrams/common/fit': ['plainLabel', 'fitLabel'],
  'diagrams/common/fit-text': ['fitText'],
  'diagrams/common/fit-width': ['capWidth'],
  'diagrams/common/icon': ['iconSvg'],
  'diagrams/common/scan': ['lineEnd', 'keywordAt', 'skipTrivia', 'accessibility'],
  'diagrams/common/caseless': ['wordAt'],
  'diagrams/common/outline': ['tokenize', 'parseOutline'],
  'svg/palette': ['palette'],
  'diagrams/block/layout': ['layout'],
};
const peleSrc = resolve(here, '../vendor/pele/src');
const commonTap = {
  name: 'common-tap',
  setup(b) {
    b.onResolve({ filter: /\.js$/ }, (args) => {
      if (args.namespace === 'common-tap') return { path: join(peleSrc, `${args.path.slice(2, -3)}.ts`) };
      const file = resolve(args.resolveDir, args.path);
      const module = Object.keys(COMMON).find((name) => file === join(peleSrc, `${name}.js`));
      return module === undefined ? undefined : { path: module, namespace: 'common-tap' };
    });
    b.onLoad({ filter: /.*/, namespace: 'common-tap' }, (args) => {
      const file = `./${args.path}.js`;
      const key = basename(args.path);
      return {
        resolveDir: peleSrc,
        loader: 'js',
        contents: [
          `import * as real from '${file}';`,
          `export * from '${file}';`,
          `const tap = (name) => (...args) => { const t = globalThis.peleCommonTap; const f = t && t['${key}.' + name]; return f ? f(real[name], args) : real[name](...args); };`,
          ...COMMON[args.path].map((name) => `export const ${name} = tap('${name}');`),
        ].join('\n'),
      };
    });
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
  plugins: [layoutTap, commonTap],
  banner: { js: `// harness ts sha256 ${source} pele ${pele}` },
});
