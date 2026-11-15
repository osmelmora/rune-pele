// C4 ops: Pele's C4 model for a source, or Pele's error with its position, so that the Rust front
// end can be diffed against it.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { C4Db } from '../vendor/pele/src/diagrams/c4/db.ts';
import type { C4Boundary } from '../vendor/pele/src/diagrams/c4/types.ts';

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

export const c4Ops: Record<string, (args: string[]) => string> = {
  c4_front: ([source]) => c4Front(JSON.parse(source) as string),
};
