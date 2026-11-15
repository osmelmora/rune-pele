// C4 ops: Pele's C4 model for a source, or Pele's error with its position, so that the Rust front
// end can be diffed against it.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { C4Db } from '../vendor/pele/src/diagrams/c4/db.ts';
import type { C4Boundary } from '../vendor/pele/src/diagrams/c4/types.ts';
import { layoutC4 } from '../vendor/pele/src/diagrams/c4/layout.ts';
import { Lanes, route, type Box } from '../vendor/pele/src/diagrams/c4/route.ts';
import { metricsMeasurer } from '../vendor/pele/src/text/measurer.ts';

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

// Each record's fields in the order its TypeScript interface declares them. A field that is
// undefined is left out, and a field the list does not name fails the request.
const ELEMENT = ['alias', 'label', 'aliasAttr', 'labelAttr', 'parent', 'type', 'descr', 'techn', 'sprite', 'tags', 'link', 'bgColor', 'fontColor', 'borderColor', 'shadowing', 'shape', 'legendText', 'legendSprite', 'extra'];
const FIELDS = {
  shape: [...ELEMENT, 'kind'],
  boundary: [...ELEMENT, 'nodeType'],
  rel: ['type', 'from', 'to', 'label', 'fromAttr', 'toAttr', 'labelAttr', 'techn', 'descr', 'sprite', 'tags', 'link', 'textColor', 'lineColor', 'offsetX', 'offsetY', 'extra'],
};

function record(item: object, kind: keyof typeof FIELDS, map: Record<string, (value: never) => unknown>): Record<string, unknown> {
  const names = FIELDS[kind];
  for (const key of Object.keys(item)) {
    if (!names.includes(key)) throw new Error(`oracle: ${kind} has the unlisted field ${key}`);
  }
  const out: Record<string, unknown> = {};
  for (const name of names) {
    const value = (item as Record<string, unknown>)[name];
    if (value !== undefined) out[name] = map[name] ? map[name](value as never) : value;
  }
  return out;
}

function c4Front(text: string): string {
  let db: C4Db & { mermaidType: string };
  try {
    const model = parse(text);
    if (model.type !== 'c4') return JSON.stringify({ type: model.type });
    db = model as unknown as C4Db & { mermaidType: string };
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'c4') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  const parent = (boundary: C4Boundary) => db.boundaries.indexOf(boundary);
  const extra = (map: Map<string, string>) => [...map.entries()];
  const number = (value: number) => String(value);
  return JSON.stringify({
    type: db.type,
    mermaidType: db.mermaidType,
    c4Type: db.c4Type,
    title: db.title,
    accTitle: db.accTitle,
    accDescr: db.accDescr,
    shapes: db.shapes.map((s) => record(s, 'shape', { parent, extra })),
    boundaries: db.boundaries.map((b) => record(b, 'boundary', { parent, extra })),
    rels: db.rels.map((r) => record(r, 'rel', { extra, offsetX: number, offsetY: number })),
    shapeInRow: String(db.shapeInRow),
    boundaryInRow: String(db.boundaryInRow),
  });
}

// Pele's layoutC4 of a source at a font size, with a room and a gap as a tight drawing gives them:
// the boundaries that are drawn, in order, with their shapes. Numbers travel as bits.
function c4Layout(text: string, size: number, room: number | undefined, gap: number): string {
  let db: C4Db;
  try {
    const model = parse(text);
    if (model.type !== 'c4') return JSON.stringify({ type: model.type });
    db = model as unknown as C4Db;
  } catch (error) {
    return JSON.stringify({ error: String(error) });
  }
  const small = Math.round(size * 0.8125);
  const { order } = layoutC4(db, metricsMeasurer, size, small, room, gap);
  const frame = (b: Box) => [b.x, b.y, b.w, b.h].map(bits);
  const sizes = (labels: { width: number; height: number }[]) => labels.map((l) => [bits(l.width), bits(l.height)]);
  return JSON.stringify(
    order.map((g) => ({
      boundary: db.boundaries.indexOf(g.boundary),
      frame: frame(g),
      head: bits(g.head),
      headW: bits(g.headW),
      headX: bits(g.headX),
      kids: g.kids.map((k) => db.boundaries.indexOf(k.boundary)),
      labels: sizes(g.labels),
      nodes: g.nodes.map((n) => ({
        shape: db.shapes.indexOf(n.shape),
        form: n.form,
        frame: frame(n),
        textH: bits(n.textH),
        labels: sizes(n.labels),
      })),
    }))
  );
}

interface RouteRequest {
  boxes: [string, string, string, string, string][];
  frames: number[];
  calls: { a: number; b: number; shift: string; others: number[] | null; startTrim: string; endTrim: string }[];
}

// Frames boxes in one set of lanes, then routes each call in turn through them, as renderC4 routes
// its relations. Numbers travel as bits.
function c4Route(request: RouteRequest): string {
  const boxes: Box[] = request.boxes.map(([x, y, w, h, form]) => ({ x: double(x), y: double(y), w: double(w), h: double(h), form }));
  const lanes = new Lanes();
  for (const frame of request.frames) lanes.frame(boxes[frame]);
  const routes = request.calls.map((call) => {
    const others = call.others === null ? undefined : call.others.map((i) => boxes[i]);
    const r = route(boxes[call.a], boxes[call.b], double(call.shift), others, lanes, double(call.startTrim), double(call.endTrim));
    return {
      d: r.d,
      start: [r.sx, r.sy, r.sdx, r.sdy].map(bits),
      end: [r.ex, r.ey, r.edx, r.edy].map(bits),
      stretch: [r.x0, r.y0, r.x1, r.y1].map(bits),
      nx: bits(r.nx),
      ny: bits(r.ny),
      lean: bits(r.lean),
      room: bits(r.room),
      points: r.points.map(bits),
    };
  });
  return JSON.stringify({ routes, squeezed: lanes.squeezed });
}

export const c4Ops: Record<string, (args: string[]) => string> = {
  c4_front: ([source]) => c4Front(JSON.parse(source) as string),
  c4_layout: ([source, size, room, gap]) =>
    c4Layout(JSON.parse(source) as string, double(size), room === 'null' ? undefined : double(room), double(gap)),
  c4_route: ([request]) => c4Route(JSON.parse(request) as RouteRequest),
};
