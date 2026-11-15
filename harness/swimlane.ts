// Oracle ops for swimlanes: Pele's lane layout, as crates/pele-core/tests/lanes_replay.rs holds
// the port to it. A call travels as `{ fn, in, out }`, with doubles packed as 16 hex digits each in
// the field order of NODE and EDGE, the shape oracle.ts gives the compound layout's calls.
// harness/build.mjs routes `laneLayout` through `globalThis.peleLayoutTap`, which is how a call
// made by a real render is seen.
import { render } from '../vendor/pele/src/index.ts';
import type { CEdge, CNode, Dir } from '../vendor/pele/src/layout/compound.ts';
import { laneLayout } from '../vendor/pele/src/layout/lanes.ts';
import type { LayeredOptions } from '../vendor/pele/src/layout/layered.ts';
import { metricsMeasurer } from '../vendor/pele/src/text/measurer.ts';

type Json = Record<string, unknown>;
type Tap = Record<string, (real: (...args: never[]) => unknown, args: never[]) => unknown>;
const NODE = ['w', 'h', 'padX', 'padTop', 'padBottom', 'minW', 'seq', 'span', 'x', 'y'] as const;
const EDGE = ['minlen', 'labelW', 'labelH', 'labelX', 'labelY'] as const;
const OPT = ['nodeSep', 'edgeSep', 'rankSep', 'portSep'] as const;
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

const laneIn = (nodes: CNode[], edges: CEdge[], dir: Dir, opt: LayeredOptions, lanePad: number | undefined): Json => ({
  ...graphOut(nodes, edges),
  dir,
  opt: fields(opt, OPT),
  lanePad: lanePad === undefined ? null : bits(lanePad),
});

const laneOut = (nodes: CNode[], edges: CEdge[], size: { width: number; height: number }): Json => ({
  ...graphOut(nodes, edges),
  size: pack([size.width, size.height]),
});

// Renders a source and answers every lane layout call the render made.
function laneRecord(source: string, options: object): string {
  const calls: Json[] = [];
  let result: Json = {};
  const tap: Tap = {
    laneLayout: (real, args) => {
      const [nodes, edges, dir, opt, lanePad] = args as unknown as [CNode[], CEdge[], Dir, LayeredOptions, number | undefined];
      const call: Json = { fn: 'laneLayout', in: laneIn(nodes, edges, dir, opt, lanePad) };
      calls.push(call);
      const size = real(...args) as { width: number; height: number };
      call.out = laneOut(nodes, edges, size);
      return size;
    },
  };
  (globalThis as { peleLayoutTap?: Tap }).peleLayoutTap = tap;
  try {
    const { type } = render(source, { ...PARITY, ...options });
    result = { type };
  } catch (error) {
    result = { error: String(error) };
  } finally {
    (globalThis as { peleLayoutTap?: Tap }).peleLayoutTap = undefined;
  }
  return JSON.stringify({ ...result, calls });
}

// Runs Pele's laneLayout on the input of a call, and answers what it left behind.
function laneRun(call: string): string {
  const input = (JSON.parse(call) as { in: Json }).in;
  const { nodes, edges } = graphIn(input);
  const opt = {} as LayeredOptions;
  assign(opt, OPT, input.opt as string);
  const lanePad = input.lanePad === null ? undefined : double(input.lanePad as string);
  const size = laneLayout(nodes, edges, input.dir as Dir, opt, lanePad);
  return JSON.stringify({ out: laneOut(nodes, edges, size) });
}

export const swimlaneOps: Record<string, (args: string[]) => string> = {
  lane_record: ([source, options]) => laneRecord(JSON.parse(source) as string, options ? (JSON.parse(options) as object) : {}),
  lane_run: ([call]) => laneRun(call),
};
