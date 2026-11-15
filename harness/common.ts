// Ops for the helpers that several of Pele's diagram types share, so that their Rust ports can be
// diffed against Pele one helper at a time.
//
// `common` takes one JSON object whose `fn` names the helper and answers one JSON value. Strings
// travel as JSON, and answer as arrays of UTF-16 code units, so a lone surrogate survives both
// ways. Doubles travel as 16 hex digits of their bits, as in oracle.ts.
//
// `common_record` renders a source as the corpus test does, with any render options laid over, and
// answers every distinct input the render gave the helpers, in the form `common` takes. The option
// `icons: true` gives the render a host that resolves every icon name. harness/build.mjs routes the helpers through
// `globalThis.peleCommonTap`, which is how a call made by a real render is seen.
import { PeleError, render } from '../vendor/pele/src/index.ts';
import { ACC_TITLE, DIRECTIVE, NEWLINE, SINGLE_LINE_COMMENT, STRING, WHITESPACE, YAML, keyword, titleBefore, tokenize, type TokenType } from '../vendor/pele/src/diagrams/common/tokens.ts';
import { accDescr } from '../vendor/pele/src/diagrams/common/accDescr.ts';
import { wordAt } from '../vendor/pele/src/diagrams/common/caseless.ts';
import { accessibility, keywordAt, lineEnd, skipTrivia, type Tokens as ScanTokens } from '../vendor/pele/src/diagrams/common/scan.ts';
import { capWidth } from '../vendor/pele/src/diagrams/common/fit-width.ts';
import { fitLabel, plainLabel } from '../vendor/pele/src/diagrams/common/fit.ts';
import { fitText } from '../vendor/pele/src/diagrams/common/fit-text.ts';
import { breakLabel, leftLabelSvg, literalLabel } from '../vendor/pele/src/diagrams/common/literal-label.ts';
import { iconSvg } from '../vendor/pele/src/diagrams/common/icon.ts';
import { getType, parseOutline, tokenize as tokenizeOutline, type OutlineDb } from '../vendor/pele/src/diagrams/common/outline.ts';
import { palette, seriesInkFill, withPalette } from '../vendor/pele/src/svg/palette.ts';
import type { Label } from '../vendor/pele/src/text/label.ts';
import { metricsMeasurer } from '../vendor/pele/src/text/measurer.ts';
import type { DiagramType } from '../vendor/pele/src/detect.ts';
import { isBlank, isDigit, isHex, isLetter, isLineEnd, isSpace, isWord } from '../vendor/pele/src/util/chars.ts';

type Json = Record<string, unknown>;
type Tap = Record<string, (real: (...args: never[]) => unknown, args: never[]) => unknown>;

function double(hex: string): number {
  const view = new DataView(new ArrayBuffer(8));
  view.setBigUint64(0, BigInt('0x' + hex));
  return view.getFloat64(0);
}

function bits(value: number): string {
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, value);
  return view.getBigUint64(0).toString(16).padStart(16, '0');
}

const units = (text: string): number[] => Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
const text = (codes: number[]): string => codes.map((code) => String.fromCharCode(code)).join('');
const optional = (hex: unknown): number | undefined => (typeof hex === 'string' ? double(hex) : undefined);

// A label as JSON. This port has no icons and no math, so a label that holds either fails the op.
function labelOut(label: Label): Json {
  for (const line of label.lines) {
    for (const span of line) if (span.icon !== undefined || span.math !== undefined) throw new Error('oracle: a label with an icon or math');
  }
  if (label.heights !== undefined) throw new Error('oracle: a label with line heights');
  return {
    lines: label.lines.map((line) => line.map((span) => ({ text: units(span.text), style: span.style, width: bits(span.width) }))),
    widths: label.widths.map(bits),
    width: bits(label.width),
    height: bits(label.height),
    size: bits(label.size),
    lineHeight: bits(label.lineHeight),
  };
}

function labelIn(json: Json): Label {
  const lines = (json.lines as Json[][]).map((line) =>
    line.map((span) => ({ text: text(span.text as number[]), style: span.style as number, width: double(span.width as string) }))
  );
  return {
    lines,
    widths: (json.widths as string[]).map(double),
    width: double(json.width as string),
    height: double(json.height as string),
    size: double(json.size as string),
    lineHeight: double(json.lineHeight as string),
  };
}

// The attributes a caller hands leftLabelSvg, as name and value pairs.
const RE_ATTR = / ([\w:-]+)="([^"]*)"/g;
function attrsOut(attrs: string): [string, string][] | undefined {
  const pairs = Array.from(attrs.matchAll(RE_ATTR), (m): [string, string] => [m[1], m[2]]);
  return pairs.map(([name, value]) => ` ${name}="${value}"`).join('') === attrs ? pairs : undefined;
}

const attrsIn = (pairs: [string, string][]): string => pairs.map(([name, value]) => ` ${name}="${value}"`).join('');

const fitted = (result: { text: string; size: number; width: number } | undefined): Json | null =>
  result === undefined ? null : { text: units(result.text), size: bits(result.size), width: bits(result.width) };

// Synthetic token tables in the forms Pele's Langium types build theirs: keywords and words that
// give way to a longer name, a dash that gives way to front matter, and a title that does not
// split a longer name. `rest` is the name suffix `titleBefore` is given.
const LEX_ID: TokenType = { name: 'ID', pattern: /[\w-]+/y };
function lexTable(rest: string): TokenType[] {
  return [
    { ...keyword('radar-beta'), longer: [LEX_ID] },
    { name: 'axis', pattern: 'axis', longer: [LEX_ID] },
    { name: '-', pattern: '-', longer: [YAML] },
    titleBefore(rest),
    accDescr(),
    ACC_TITLE,
    YAML,
    DIRECTIVE,
    NEWLINE,
    WHITESPACE,
    SINGLE_LINE_COMMENT,
    STRING,
    LEX_ID,
    { name: ':', pattern: ':' },
  ];
}
const LEX_RESTS = [String.raw`[-\w]*\w`, String.raw`\w`, String.raw`[-./\w]*[-\w]`, String.raw`(?!%%)[^\n\r]`];
const lexTables = new Map(LEX_RESTS.map((rest) => [rest, lexTable(rest)]));

function errorOut(error: unknown): Json {
  if (!(error instanceof PeleError)) throw error;
  return { message: units(error.message), line: error.line, column: error.column };
}

// An outline db that records each call the grammar's actions make. `getType` is Pele's own, whose
// answer each addNode call shows.
function recordingDb(calls: Json[]): OutlineDb {
  return {
    getType,
    addNode(level, id, descr, type, shapeData) {
      calls.push({ call: 'addNode', level, id: units(id), descr: units(descr), type, shapeData: shapeData === undefined ? null : units(shapeData) });
    },
    decorateNode(decoration) {
      calls.push({ call: 'decorateNode', icon: decoration.icon === undefined ? null : units(decoration.icon), class: decoration.class === undefined ? null : units(decoration.class) });
    },
  };
}

// One scan call on `src`: `[fn, at, word]`.
type ScanCall = [string, number, string | null];

function scan(src: string, [fn, at, word]: ScanCall): unknown {
  switch (fn) {
    case 'lineEnd':
      return lineEnd(src, at);
    case 'keywordAt':
      return keywordAt(src, at, word!);
    case 'wordAt':
      return wordAt(src, at, word!);
    case 'skipTrivia':
      return skipTrivia(src, at);
    case 'accessibility': {
      const out: ScanTokens = { types: [], texts: [], starts: [] };
      const end = accessibility(src, at, out, 0);
      return { end, tokens: out.types.map((type, i) => [type, out.starts[i], out.starts[i] + out.texts[i].length]) };
    }
  }
  throw new Error(`oracle: no scan function ${fn}`);
}

const CHAR_TESTS = [isSpace, isBlank, isLineEnd, isDigit, isLetter, isHex, isWord];

const helpers: Record<string, (input: Json) => unknown> = {
  // For every UTF-16 code unit, a bit for each class of util/chars.ts it is in, in CHAR_TESTS order.
  chars: () => Array.from({ length: 0x10000 }, (_, c) => CHAR_TESTS.reduce((bits, test, bit) => bits | (Number(test(c)) << bit), 0)),
  scan: ({ src, calls }) => (calls as ScanCall[]).map((call) => scan(src as string, call)),
  // The ends a fresh `accDescr` scanner answers at each position in turn, or null for no match.
  acc_descr: ({ src, at }) => {
    const scanner = accDescr().scanner!();
    return (at as number[]).map((p) => {
      const end = scanner(src as string, p, -1);
      return end === -1 ? null : end;
    });
  },
  title_before: ({ rest, src, at }) => {
    const pattern = titleBefore(rest as string).pattern as RegExp;
    pattern.lastIndex = at as number;
    return pattern.test(src as string) ? pattern.lastIndex : null;
  },
  lex: ({ rest, src }) => {
    try {
      const { kinds, starts, ends } = tokenize(src as string, lexTables.get(rest as string)!, 'lex');
      return { kinds, starts, ends };
    } catch (error) {
      return { error: errorOut(error) };
    }
  },
  cap_width: ({ maxWidth, natural, least, around }) =>
    bits(capWidth({ maxWidth: optional(maxWidth) }, double(natural as string), double(least as string), optional(around))),
  palette: ({ type, config }) => {
    const found = palette(type as DiagramType, config as Json);
    const fills = withPalette(found, () => Array.from({ length: 24 }, (_, i) => seriesInkFill(i)));
    return { style: units(found.style), inks: Array.from({ length: 8 }, (_, i) => found.inks[i] ?? null), fills };
  },
  fit_text: ({ text, max, size, min, style }) =>
    fitted(fitText(text as string, double(max as string), double(size as string), double(min as string), metricsMeasurer, style as number)),
  plain_label: ({ raw, size, max, base }) =>
    labelOut(plainLabel((raw as string | null) ?? undefined, metricsMeasurer, double(size as string), double(max as string), base as number)),
  fit_label: ({ raw, size, max, base }) =>
    labelOut(fitLabel((raw as string | null) ?? undefined, metricsMeasurer, double(size as string), double(max as string), base as number)),
  literal_label: ({ text, size, room, style }) =>
    labelOut(literalLabel(text as string, metricsMeasurer, double(size as string), double(room as string), style as number)),
  break_label: ({ label, room }) => labelOut(breakLabel(labelIn(label as Json), metricsMeasurer, double(room as string))),
  left_label: ({ label, x, top, attrs }) =>
    units(leftLabelSvg(labelIn(label as Json), double(x as string), double(top as string), attrsIn(attrs as [string, string][]))),
  icon: ({ name, x, y, side }) => units(iconSvg(name as string, double(x as string), double(y as string), double(side as string), undefined)),
  outline_tokenize: ({ src, kanban }) => {
    const { types, texts, starts } = tokenizeOutline(src as string, kanban as boolean);
    return { types, texts: texts.map(units), starts };
  },
  outline_parse: ({ src, kanban }) => {
    const calls: Json[] = [];
    try {
      parseOutline(src as string, recordingDb(calls), kanban as boolean);
      return { calls, error: null };
    } catch (error) {
      return { calls, error: errorOut(error) };
    }
  },
};

// What each tapped helper was given, in the form `common` takes, or undefined for a call this
// port cannot be given: one with a measurer other than Pele's parity metrics, or icons.
const RECORD: Record<string, (args: never[]) => Json | undefined> = {
  'literal-label.literalLabel': ([text, measurer, size, room, style]: [string, unknown, number, number, number?]) =>
    measurer === metricsMeasurer ? { fn: 'literal_label', text, size: bits(size), room: bits(room), style: style ?? 0 } : undefined,
  'literal-label.breakLabel': ([label, measurer, room]: [Label, unknown, number]) =>
    measurer === metricsMeasurer ? { fn: 'break_label', label: labelOut(label), room: bits(room) } : undefined,
  'literal-label.leftLabelSvg': ([label, x, top, attrs]: [Label, number, number, string]) => {
    const pairs = attrsOut(attrs);
    return pairs && { fn: 'left_label', label: labelOut(label), x: bits(x), top: bits(top), attrs: pairs };
  },
  'fit.plainLabel': ([raw, measurer, size, max, base]: [string | undefined, unknown, number, number, number?]) =>
    measurer === metricsMeasurer ? { fn: 'plain_label', raw: raw ?? null, size: bits(size), max: bits(max), base: base ?? 0 } : undefined,
  'fit.fitLabel': ([raw, measurer, size, max, base]: [string | undefined, unknown, number, number, number?]) =>
    measurer === metricsMeasurer ? { fn: 'fit_label', raw: raw ?? null, size: bits(size), max: bits(max), base: base ?? 0 } : undefined,
  'fit-text.fitText': ([text, max, size, min, measurer, style]: [string, number, number, number, unknown, number?]) =>
    measurer === metricsMeasurer ? { fn: 'fit_text', text, max: bits(max), size: bits(size), min: bits(min), style: style ?? 0 } : undefined,
  'fit-width.capWidth': ([options, natural, least, around]: [{ maxWidth?: number }, number, number, number?]) => ({
    fn: 'cap_width',
    maxWidth: options.maxWidth === undefined ? null : bits(options.maxWidth),
    natural: bits(natural),
    least: bits(least),
    around: around === undefined ? null : bits(around),
  }),
  // Pele draws an icon only where the host resolves it. This port's Scene draws the box, and the
  // host's interpreter what goes in it, so the input is the box alone.
  'icon.iconSvg': ([name, x, y, side]: [string, number, number, number]) => ({ fn: 'icon', name, x: bits(x), y: bits(y), side: bits(side) }),
  'outline.tokenize': ([src, kanban]: [string, boolean]) => ({ fn: 'outline_tokenize', src, kanban }),
  'outline.parseOutline': ([src, , kanban]: [string, unknown, boolean]) => ({ fn: 'outline_parse', src, kanban }),
  'palette.palette': ([type, config]: [string, Json]) => ({ fn: 'palette', type, config }),
};
const SCANS = ['lineEnd', 'keywordAt', 'skipTrivia', 'accessibility'].map((name) => `scan.${name}`).concat('caseless.wordAt');

// The record is read as JSON text, which cannot hold half of a surrogate pair.
function wellFormed(value: unknown): boolean {
  if (typeof value === 'string') return value.isWellFormed();
  if (Array.isArray(value)) return value.every(wellFormed);
  if (typeof value === 'object' && value !== null) return Object.values(value).every(wellFormed);
  return true;
}

// What a host's resolver puts in an icon's box, for a render with `icons: true` in its options.
const ICON = '<circle cx="12" cy="12" r="9"/>';

function record(source: string, { icons, ...options }: Json): Json {
  const seen = new Set<string>();
  const inputs: Json[] = [];
  const skipped: Record<string, number> = {};
  const scans = new Map<string, Map<string, ScanCall>>();
  const tap: Tap = {};
  for (const [name, input] of Object.entries(RECORD)) {
    tap[name] = (real, args) => {
      let found: Json | undefined;
      try {
        found = input(args);
      } catch {
        // A label that holds an icon, which only a host with icons draws.
        found = undefined;
      }
      if (found === undefined || !wellFormed(found)) {
        skipped[name] = (skipped[name] ?? 0) + 1;
      } else {
        const key = JSON.stringify(found);
        if (!seen.has(key)) {
          seen.add(key);
          inputs.push(found);
        }
      }
      return real(...args);
    };
  }
  for (const name of SCANS) {
    tap[name] = (real, args) => {
      const [src, at, word] = args as [string, number, unknown];
      const fn = name.slice(name.indexOf('.') + 1);
      const call: ScanCall = [fn, at, typeof word === 'string' ? word : null];
      if (!src.isWellFormed()) {
        skipped[name] = (skipped[name] ?? 0) + 1;
        return real(...args);
      }
      if (!scans.has(src)) scans.set(src, new Map());
      scans.get(src)!.set(JSON.stringify(call), call);
      return real(...args);
    };
  }
  let type: string | null = null;
  (globalThis as { peleCommonTap?: Tap }).peleCommonTap = tap;
  try {
    const resolver = icons ? { icons: () => ICON } : {};
    type = render(source, { measurer: metricsMeasurer, now: Date.UTC(2026, 0, 15, 12), ...options, ...resolver }).type;
  } catch (error) {
    type = error instanceof PeleError ? error.type : null;
  } finally {
    (globalThis as { peleCommonTap?: Tap }).peleCommonTap = undefined;
  }
  for (const [src, calls] of scans) inputs.push({ fn: 'scan', src, calls: [...calls.values()] });
  return { type, inputs, skipped };
}

export const commonOps: Record<string, (args: string[]) => string> = {
  common: ([input]) => {
    // `{"$units": [...]}` stands for the string of those code units, which JSON text from Rust
    // cannot otherwise hold when it has half of a surrogate pair.
    const json = JSON.parse(input, (_, value) => (Array.isArray(value?.$units) ? text(value.$units) : value)) as Json;
    const helper = helpers[json.fn as string];
    if (!helper) throw new Error(`oracle: no common helper ${json.fn}`);
    return JSON.stringify(helper(json));
  },
  common_record: ([source, options]) => JSON.stringify(record(JSON.parse(source) as string, options ? (JSON.parse(options) as Json) : {})),
};
