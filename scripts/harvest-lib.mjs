// What the harvest scripts of the standalone types share: reading Pele's test files, collecting
// sources and mutated inputs without duplicates, and writing tests/fixtures/<kind>/harvest.json.
// A source with a lone surrogate is left out, as it cannot cross into Rust as a string.
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import { join, relative } from 'node:path';
import { mutator, random } from '../vendor/pele/tests/support/corpus.ts';

export const root = join(import.meta.dirname, '..');
export const pele = join(root, 'vendor/pele');
const tests = join(pele, 'tests');

export const RE_LITERAL = /`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"/g;

function files(dir) {
  return readdirSync(dir, { withFileTypes: true })
    .sort((a, b) => (a.name < b.name ? -1 : 1))
    .flatMap((entry) =>
      entry.isDirectory() ? files(join(dir, entry.name)) : /\.[jt]s$/.test(entry.name) ? [join(dir, entry.name)] : []
    );
}

// A string literal's value, as Pele's loadCorpus reads it: a `${...}` stays as written.
export function value(raw) {
  try {
    return new Function('return `' + raw.replace(/`/g, '\\`').replace(/\$\{/g, '\\${') + '`')();
  } catch {
    return undefined;
  }
}

export function read(file) {
  return readFileSync(join(tests, file), 'utf8');
}

export function constant(file, name) {
  const found = new RegExp(`const ${name} = (\\[[\\s\\S]*?\\n\\]);`).exec(read(file));
  if (found === null) throw new Error(`${file} no longer declares ${name}`);
  return found[1];
}

// The code of a test file from one line to the next, with its types stripped, run to give `names`.
export function evaluate(file, from, to, names, args = {}) {
  const text = read(file);
  const start = text.indexOf(from);
  const end = text.indexOf(to, start);
  if (start < 0 || end < 0) throw new Error(`${file} no longer holds ${from} … ${to}`);
  const code = stripTypeScriptTypes(text.slice(start, end));
  return new Function(...Object.keys(args), `${code}\nreturn { ${names.join(', ')} };`)(...Object.values(args));
}

export const PAYLOADS = new Function('return ' + constant('support/inert.ts', 'PAYLOADS'))();

// The sources, mutated inputs and worst cases of one diagram type.
export function harvest(kind) {
  const sources = new Set();
  const mutated = new Set();
  const worst = [];
  const origin = new Map();
  let illFormed = 0;
  return {
    sources,
    add(text, from) {
      if (typeof text !== 'string' || sources.has(text)) return;
      if (!text.isWellFormed()) {
        illFormed++;
        return;
      }
      sources.add(text);
      origin.set(from, (origin.get(from) ?? 0) + 1);
    },
    // Every string literal of vendor/pele/tests that `starts` accepts and that holds no `${`.
    literals(starts) {
      for (const file of files(tests)) {
        const from = relative(tests, file);
        for (const match of readFileSync(file, 'utf8').matchAll(RE_LITERAL)) {
          const found = value(match[1] ?? match[2] ?? match[3]);
          if (found !== undefined && starts(found) && !found.includes('${')) this.add(found, from);
        }
      }
    },
    keep(text) {
      if (sources.has(text)) return;
      if (text.isWellFormed()) mutated.add(text);
      else illFormed++;
    },
    // `count` inputs of Pele's mutator, keeping those of at most `longest` characters.
    mutate(seeds, fragments, seed, count, longest) {
      const next = mutator(seeds, fragments, random(seed));
      for (let i = 0; i < count; i++) {
        const text = next();
        if (text.length <= longest) this.keep(text);
      }
    },
    worst(text) {
      if (text.isWellFormed()) worst.push(text);
      else illFormed++;
    },
    write() {
      const pin = execFileSync('git', ['-C', pele, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
      mkdirSync(join(root, `tests/fixtures/${kind}`), { recursive: true });
      const out = join(root, `tests/fixtures/${kind}/harvest.json`);
      const json = { pele: pin, sources: [...sources], mutated: [...mutated], worst };
      writeFileSync(out, JSON.stringify(json, null, 1) + '\n');
      for (const [from, count] of origin) console.log(`${String(count).padStart(5)} ${from}`);
      console.log(`${sources.size} sources, ${mutated.size} mutated inputs and ${worst.length} worst cases written to ${relative(root, out)}`);
      console.log(`${illFormed} inputs left out: a lone surrogate cannot cross into Rust as a string`);
    },
  };
}
