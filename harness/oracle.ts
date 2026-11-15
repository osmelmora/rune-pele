// Answers numeric questions the way Pele 1.13.0 does, so the Rust port can be diffed against it.
// Protocol: one request per stdin line, `<op>\t<arg>...`, one answer per stdout line.
// Doubles travel as 16 hex digits of their IEEE-754 bits so no decimal parse sits between the sides.
// `render` takes a JSON string and answers one JSON object. `render_with` also takes a JSON object
// of Pele render options, laid over the pinned ones. `detect`, `preprocess`, `model` and `label`
// expose the stages before drawing, so a port can be diffed one stage at a time.
import { createInterface } from 'node:readline';
import { detectDiagram } from '../vendor/pele/src/detect.ts';
import { detectType, parse, render } from '../vendor/pele/src/index.ts';
import { encodeEntities, preprocess } from '../vendor/pele/src/preprocess.ts';
import { num } from '../vendor/pele/src/svg/builder.ts';
import { layoutLabel } from '../vendor/pele/src/text/label.ts';
import { metricsMeasurer } from '../vendor/pele/src/text/measurer.ts';

// The same pinned inputs Pele's own render corpus test uses, so output depends only on the source.
const PARITY = { measurer: metricsMeasurer, now: Date.UTC(2026, 0, 15, 12) };

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

function rendered(text: string, options: object): string {
  try {
    const { type, svg, width, height } = render(text, { ...PARITY, ...options });
    return JSON.stringify({ type, svg, width, height });
  } catch (error) {
    let type: string | null = null;
    try {
      type = detectType(text);
    } catch {
      // Front matter that is not YAML fails detection as well.
    }
    return JSON.stringify({ type, error: String(error) });
  }
}

// JSON has no NaN or Infinity, and a config may hold either.
function plain(value: unknown): unknown {
  if (typeof value === 'number' && !Number.isFinite(value)) return { $number: String(value) };
  if (Array.isArray(value)) return value.map(plain);
  if (typeof value === 'object' && value !== null) {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, plain(item)]));
  }
  return value;
}

const ops: Record<string, (args: string[]) => string> = {
  round: ([x]) => {
    const r = Math.round(double(x));
    return Number.isNaN(r) ? 'NaN' : bits(r);
  },
  num: ([x]) => num(double(x)),
  string: ([x]) => String(double(x)),
  fixed: ([x, digits]) => double(x).toFixed(Number(digits)),
  sin: ([x]) => bits(Math.sin(double(x))),
  cos: ([x]) => bits(Math.cos(double(x))),
  pow: ([x, y]) => bits(Math.pow(double(x), double(y))),
  render: ([source]) => rendered(JSON.parse(source) as string, {}),
  render_with: ([source, options]) => rendered(JSON.parse(source) as string, JSON.parse(options) as object),
  detect: ([source]) => JSON.stringify(detectDiagram(JSON.parse(source) as string)),
  preprocess: ([source]) => {
    try {
      const { text, title, config } = preprocess(JSON.parse(source) as string);
      return JSON.stringify({ text, title: title ?? null, config: plain(config), parsed: encodeEntities(text) });
    } catch (error) {
      return JSON.stringify({ error: String(error) });
    }
  },
  model: ([source]) => {
    try {
      const model = parse(JSON.parse(source) as string);
      if (model.type !== 'pie') return JSON.stringify({ type: model.type });
      const { type, title, accTitle, accDescr, showData, sections } = model;
      return JSON.stringify({
        type,
        title: title ?? null,
        accTitle: accTitle ?? null,
        accDescr: accDescr ?? null,
        showData,
        sections: [...sections].map(([label, value]) => [label, bits(value)]),
      });
    } catch (error) {
      return JSON.stringify({ error: String(error) });
    }
  },
  label: ([raw, size, maxWidth, base, markdown]) => {
    const label = layoutLabel(JSON.parse(raw) as string, markdown === 'true', metricsMeasurer, double(size), double(maxWidth), Number(base));
    return JSON.stringify({
      lines: label.lines.map((line) => line.map((span) => ({ text: span.text, style: span.style, width: bits(span.width) }))),
      widths: label.widths.map(bits),
      width: bits(label.width),
      height: bits(label.height),
      lineHeight: bits(label.lineHeight),
    });
  },
};

// flowchart front end
import { PeleError } from '../vendor/pele/src/index.ts';
import type { FlowDb } from '../vendor/pele/src/diagrams/flowchart/db.ts';
import { buildFlowGraph } from '../vendor/pele/src/diagrams/flowchart/graph.ts';
import { canonicalShape, isValidShape } from '../vendor/pele/src/diagrams/flowchart/shapes.ts';
import { safeUrl } from '../vendor/pele/src/util/url.ts';

// Each record's fields in the order its TypeScript interface declares them. A field that is
// undefined is left out, and a field the list does not name fails the request.
const FLOW_FIELDS = {
  node: ['id', 'text', 'labelType', 'type', 'styles', 'classes', 'dir', 'props', 'link', 'linkTarget', 'icon', 'form', 'pos', 'img', 'constraint', 'assetWidth', 'assetHeight'],
  edge: ['start', 'end', 'type', 'text', 'labelType', 'stroke', 'length', 'style', 'classes', 'id', 'isUserDefinedId', 'interpolate', 'animate', 'animation'],
  classDef: ['id', 'styles', 'textStyles'],
  subgraph: ['id', 'nodes', 'title', 'classes', 'dir', 'labelType', 'metadata'],
  graphNode: ['id', 'label', 'labelType', 'shape', 'isGroup', 'parentId', 'cssStyles', 'cssCompiledStyles', 'cssClasses', 'dir', 'link', 'linkTarget', 'tooltip', 'icon', 'pos', 'img', 'assetWidth', 'assetHeight', 'constraint', 'colorIndex', 'metadata'],
  graphEdge: ['id', 'isUserDefinedId', 'start', 'end', 'type', 'label', 'labelType', 'thickness', 'pattern', 'minlen', 'arrowTypeStart', 'arrowTypeEnd', 'cssCompiledStyles', 'style', 'classes', 'animate', 'animation', 'curve'],
};

function flowRecord(record: object, kind: keyof typeof FLOW_FIELDS): Record<string, unknown> {
  const names = FLOW_FIELDS[kind];
  for (const key of Object.keys(record)) {
    if (!names.includes(key)) throw new Error(`oracle: ${kind} has the unlisted field ${key}`);
  }
  const out: Record<string, unknown> = {};
  for (const name of names) {
    const value = (record as Record<string, unknown>)[name];
    if (value !== undefined) out[name] = value;
  }
  return out;
}

// Pele's flowchart model and flow graph for a source and parse options, or Pele's error with its
// position. A source of another type answers only that type.
function flowFront(text: string, options: { config?: Record<string, unknown> }): string {
  let db: FlowDb & { mermaidType: string };
  try {
    const model = parse(text, options);
    if (model.type !== 'flowchart') return JSON.stringify({ type: model.type });
    db = model as unknown as FlowDb & { mermaidType: string };
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'flowchart') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  const pre = preprocess(text).config;
  const config = options.config ? { ...options.config, ...pre } : pre;
  const flow = (config.flowchart ?? {}) as Record<string, unknown>;
  const graph = buildFlowGraph(db, typeof flow.curve === 'string' ? flow.curve : undefined);
  const model = {
    type: db.type,
    mermaidType: db.mermaidType,
    direction: db.direction,
    nodes: [...db.nodes.values()].map((node) => flowRecord(node, 'node')),
    edges: db.edges.map((edge) => flowRecord(edge, 'edge')),
    subgraphs: db.subgraphs.map((subgraph) => flowRecord(subgraph, 'subgraph')),
    classes: [...db.classes.values()].map((classDef) => flowRecord(classDef, 'classDef')),
    tooltips: [...db.tooltips],
    defaultEdgeStyle: db.defaultEdgeStyle,
    defaultInterpolate: db.defaultInterpolate,
    title: db.title,
    accTitle: db.accTitle,
    accDescr: db.accDescr,
  };
  return JSON.stringify({
    model,
    graph: {
      nodes: graph.nodes.map((node) => flowRecord(node, 'graphNode')),
      edges: graph.edges.map((edge) => flowRecord(edge, 'graphEdge')),
    },
  });
}

ops.flow_front = ([source, options]) => flowFront(JSON.parse(source) as string, options ? JSON.parse(options) : {});
ops.canonical_shape = ([name]) => {
  const shape = JSON.parse(name) as string;
  return JSON.stringify([canonicalShape(shape), isValidShape(shape)]);
};
ops.safe_url = ([url]) => JSON.stringify(safeUrl(JSON.parse(url) as string));

// compound layout
// A layout call as JSON: `{ fn, in, out }`. Doubles are packed as 16 hex digits each, in the
// field order of `NODE`, `EDGE`, `LNODE` and `LEDGE`. harness/build.mjs routes Pele's layout
// functions through `globalThis.peleLayoutTap`, which is how a call made by a real render is seen.
import { compoundLayout, inheritCrossedDirections, shiftLayout, type CEdge, type CNode, type Dir } from '../vendor/pele/src/layout/compound.ts';
import { wrapComponents } from '../vendor/pele/src/layout/components.ts';
import type { LEdge, LNode, LayeredOptions, LayeredResult } from '../vendor/pele/src/layout/layered.ts';
type Json = Record<string, unknown>;
type Tap = Record<string, (real: (...args: never[]) => unknown, args: never[]) => unknown>;
const NODE = ['w', 'h', 'padX', 'padTop', 'padBottom', 'minW', 'seq', 'span', 'x', 'y'] as const;
const EDGE = ['minlen', 'labelW', 'labelH', 'labelX', 'labelY'] as const;
const LNODE = ['w', 'h', 'pin', 'hint', 'key', 'span', 'x', 'y', 'rank'] as const;
const LEDGE = ['minlen', 'labelW', 'labelH', 'tailDx', 'headDx', 'labelX', 'labelY'] as const;
const OPT = ['nodeSep', 'edgeSep', 'rankSep', 'portSep'] as const;

const pack = (values: number[]): string => values.map(bits).join('');
const unpack = (hex: string): number[] => (hex.match(/.{16}/g) ?? []).map(double);
const fields = (item: object, names: readonly string[]): string => pack(names.map((name) => (item as Record<string, number>)[name]));
function assign(item: object, names: readonly string[], hex: string): void {
  unpack(hex).forEach((value, i) => ((item as Record<string, number>)[names[i]] = value));
}

const graphOut = (nodes: CNode[], edges: CEdge[]): Json => ({
  nodes: nodes.map((c) => ({ parent: c.parent, group: c.isGroup, dir: c.dir ?? null, n: fields(c, NODE) })),
  edges: edges.map((e) => ({
    src: e.src,
    dst: e.dst,
    n: fields(e, EDGE),
    route: pack(e.route.filter((_, i) => i % 3 !== 2)),
    axes: e.route.filter((_, i) => i % 3 === 2).join(''),
  })),
});
function graphIn(graph: Json): { nodes: CNode[]; edges: CEdge[] } {
  const nodes = (graph.nodes as Json[]).map((c) => {
    const node = { parent: c.parent, isGroup: c.group, dir: c.dir ?? undefined } as CNode;
    assign(node, NODE, c.n as string);
    return node;
  });
  const edges = ((graph.edges ?? []) as Json[]).map((e) => {
    const points = unpack(e.route as string);
    const edge = { src: e.src, dst: e.dst, route: [...(e.axes as string)].flatMap((axis, i) => [points[2 * i], points[2 * i + 1], Number(axis)]) } as CEdge;
    assign(edge, EDGE, e.n as string);
    return edge;
  });
  return { nodes, edges };
}
const optOut = (opt: LayeredOptions): Json => ({
  n: fields(opt, OPT),
  headRoom: opt.headRoom === undefined ? null : bits(opt.headRoom),
  tracks: !!opt.tracks,
  straight: !!opt.straight,
  apart: !!opt.apart,
  orderOnly: !!opt.orderOnly,
  groups: opt.groups ? { of: [...opt.groups.of], parent: [...opt.groups.parent] } : null,
});
function optIn(opt: Json): LayeredOptions {
  const groups = opt.groups as { of: number[]; parent: number[] } | null;
  const out = { tracks: opt.tracks, straight: opt.straight, apart: opt.apart, orderOnly: opt.orderOnly } as LayeredOptions;
  assign(out, OPT, opt.n as string);
  if (opt.headRoom !== null) out.headRoom = double(opt.headRoom as string);
  if (groups) out.groups = { of: Int32Array.from(groups.of), parent: Int32Array.from(groups.parent) };
  return out;
}
const layeredOut = (nodes: LNode[], edges: LEdge[]): Json => ({
  nodes: nodes.map((v) => ({ kind: v.kind, n: fields(v, LNODE) })),
  edges: edges.map((e) => ({
    tail: e.tail,
    head: e.head,
    reversed: e.reversed,
    tailFixed: e.tailFixed,
    headFixed: e.headFixed,
    n: fields(e, LEDGE),
    points: pack(e.points),
  })),
});
const sizeOut = (size: { width: number; height: number } | undefined): string | null => (size ? pack([size.width, size.height]) : null);

// What each tapped function is given and what it leaves behind, as JSON.
const CALLS: Record<string, { input(args: never[]): Json; output(args: never[], result: unknown): Json }> = {
  compoundLayout: {
    input: ([nodes, edges, dir, opt]: [CNode[], CEdge[], Dir, LayeredOptions]) => ({ ...graphOut(nodes, edges), dir, opt: optOut(opt) }),
    output: ([nodes, edges]: [CNode[], CEdge[]], result) => ({ ...graphOut(nodes, edges), size: sizeOut(result as { width: number; height: number }) }),
  },
  shiftLayout: {
    input: ([nodes, edges, ox, oy]: [CNode[], CEdge[], number, number]) => ({ ...graphOut(nodes, edges), by: pack([ox, oy]) }),
    output: ([nodes, edges]: [CNode[], CEdge[]]) => graphOut(nodes, edges),
  },
  inheritCrossedDirections: {
    input: ([nodes, ends]: [CNode[], [number, number][]]) => ({ ...graphOut(nodes, []), ends }),
    output: ([nodes]: [CNode[]]) => graphOut(nodes, []),
  },
  wrapComponents: {
    input: ([nodes, edges, width, gap]: [CNode[], CEdge[], number, number]) => ({ ...graphOut(nodes, edges), by: pack([width, gap]) }),
    output: ([nodes, edges]: [CNode[], CEdge[]], result) => ({ ...graphOut(nodes, edges), size: sizeOut(result as { width: number; height: number } | undefined) }),
  },
  layered: {
    input: ([nodes, edges, opt]: [LNode[], LEdge[], LayeredOptions]) => ({ ...layeredOut(nodes, edges), opt: optOut(opt) }),
    output: ([nodes, edges]: [LNode[], LEdge[]], result) => {
      const { width, height, crossings, declared } = result as LayeredResult;
      return { ...layeredOut(nodes, edges), size: pack([width, height]), crossings: crossings ?? null, declared: declared ?? null };
    },
  },
};

// Runs `work` and returns the calls it made to the functions in `names`.
function tapped(names: string[], work: () => void): Json[] {
  const calls: Json[] = [];
  const tap: Tap = {};
  for (const name of names) {
    tap[name] = (real, args) => {
      // A caller may pass a one-shot iterable, and the record must hold what the function saw.
      if (name === 'inheritCrossedDirections') (args as unknown[])[1] = [...(args[1] as Iterable<unknown>)];
      const call: Json = { fn: name, in: CALLS[name].input(args) };
      calls.push(call);
      const result = real(...args);
      call.out = CALLS[name].output(args, result);
      return result;
    };
  }
  (globalThis as { peleLayoutTap?: Tap }).peleLayoutTap = tap;
  try {
    work();
  } finally {
    (globalThis as { peleLayoutTap?: Tap }).peleLayoutTap = undefined;
  }
  return calls;
}

// The arguments of one recorded call, rebuilt for Pele's own function.
function replayArgs(fn: string, input: Json): { run(): unknown; args: never[] } {
  const { nodes, edges } = graphIn(input);
  const by = typeof input.by === 'string' ? unpack(input.by) : [];
  const calls: Record<string, () => { run(): unknown; args: unknown[] }> = {
    compoundLayout: () => {
      const opt = optIn(input.opt as Json);
      return { args: [nodes, edges, input.dir, opt], run: () => compoundLayout(nodes, edges, input.dir as Dir, opt) };
    },
    shiftLayout: () => ({ args: [nodes, edges, by[0], by[1]], run: () => shiftLayout(nodes, edges, by[0], by[1]) }),
    inheritCrossedDirections: () => ({ args: [nodes, input.ends], run: () => inheritCrossedDirections(nodes, input.ends as [number, number][]) }),
    wrapComponents: () => ({ args: [nodes, edges, by[0], by[1]], run: () => wrapComponents(nodes, edges, by[0], by[1]) }),
  };
  if (!calls[fn]) throw new Error(`no layout function ${fn}`);
  return calls[fn]() as { run(): unknown; args: never[] };
}

const layoutOps: Record<string, (args: string[]) => string> = {
  // Renders a source and answers every layout call the render made.
  layout_record: ([source, options]) => {
    let result: Json = {};
    const calls = tapped(['compoundLayout', 'shiftLayout', 'inheritCrossedDirections', 'wrapComponents'], () => {
      result = JSON.parse(rendered(JSON.parse(source) as string, JSON.parse(options) as object)) as Json;
    });
    return JSON.stringify({ type: result.type, error: result.error ?? null, calls });
  },
  // Runs Pele's own function on the input of a call, and answers what it left behind. With
  // `trace`, also every `layered` call made on the way.
  layout_run: ([call]) => {
    const { fn, in: input, trace } = JSON.parse(call) as { fn: string; in: Json; trace?: boolean };
    const { run, args } = replayArgs(fn, input);
    let out: Json = {};
    const layered = tapped(trace ? ['layered'] : [], () => {
      out = CALLS[fn].output(args, run());
    });
    return JSON.stringify(trace ? { out, layered } : { out });
  },
  // Times Pele's own function on the input of a call: nanoseconds per run, the input rebuilt
  // outside the clock each time.
  layout_time: ([call, runs]) => {
    const { fn, in: input } = JSON.parse(call) as { fn: string; in: Json };
    const ns: number[] = [];
    for (let i = 0; i < Number(runs); i++) {
      const { run } = replayArgs(fn, input);
      const start = process.hrtime.bigint();
      run();
      ns.push(Number(process.hrtime.bigint() - start));
    }
    return JSON.stringify({ ns });
  },
  hypot: ([x, y]) => bits(Math.hypot(double(x), double(y))),
};
Object.assign(ops, layoutOps);

// shared helpers that several diagram types import
import { parseGenericTypes } from '../vendor/pele/src/diagrams/common/generics.ts';
import { isUnicodeLetter } from '../vendor/pele/src/util/unicode.ts';

ops.generics = ([text]) => JSON.stringify(parseGenericTypes(JSON.parse(text) as string));
// Every UTF-16 code unit `isUnicodeLetter` accepts, as one JSON array.
ops.unicode_letters = () => {
  const accepted: number[] = [];
  for (let code = 0; code <= 0xffff; code++) if (isUnicodeLetter(code)) accepted.push(code);
  return JSON.stringify(accepted);
};

import { classOps } from './class.ts';
Object.assign(ops, classOps);
import { stateOps } from './state.ts';
Object.assign(ops, stateOps);
import { agentflowOps } from './agentflow.ts';
Object.assign(ops, agentflowOps);

import { requirementOps } from './requirement.ts';
Object.assign(ops, requirementOps);
import { swimlaneOps } from './swimlane.ts';
Object.assign(ops, swimlaneOps);
import { usecaseOps } from './usecase.ts';
Object.assign(ops, usecaseOps);
import { erOps } from './er.ts';
Object.assign(ops, erOps);
import { commonOps } from './common.ts';
Object.assign(ops, commonOps);
import { blockOps } from './block.ts';
Object.assign(ops, blockOps);
import { c4Ops } from './c4.ts';
Object.assign(ops, c4Ops);
import { sequenceOps } from './sequence.ts';
Object.assign(ops, sequenceOps);
import { ganttOps } from './gantt.ts';
Object.assign(ops, ganttOps);
import { gitOps } from './git.ts';
Object.assign(ops, gitOps);
import { sankeyOps } from './sankey.ts';
Object.assign(ops, sankeyOps);
import { packetOps } from './packet.ts';
Object.assign(ops, packetOps);
const out: string[] = [];
for await (const line of createInterface({ input: process.stdin, crlfDelay: Infinity })) {
  const [op, ...args] = line.split('\t');
  const run = ops[op];
  if (!run) throw new Error(`unknown op ${op}`);
  out.push(run(args));
  if (out.length >= 4096) process.stdout.write(out.splice(0).join('\n') + '\n');
}
if (out.length > 0) process.stdout.write(out.join('\n') + '\n');
