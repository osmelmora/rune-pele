// Quadrant chart ops: Pele's quadrant model for a source, or Pele's error with its position, so
// that the Rust front end can be diffed against it. Coordinates travel as the bits of their doubles.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { PointStyle, QuadrantModel, QuadrantText } from '../vendor/pele/src/diagrams/quadrant/db.ts';

function bits(value: number): string {
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, value);
  return view.getBigUint64(0).toString(16).padStart(16, '0');
}

const STYLE = ['radius', 'color', 'strokeColor', 'strokeWidth'];
const POINT = ['x', 'y', 'text', 'className', ...STYLE];

// A point's or a class's style in a fixed field order, without the fields it does not set. A
// field the list does not name fails the request.
function style(item: PointStyle, names: string[], skip: string[] = []): Record<string, unknown> {
  for (const key of Object.keys(item)) {
    if (!names.includes(key)) throw new Error(`oracle: a quadrant point has the unlisted field ${key}`);
  }
  const out: Record<string, unknown> = {};
  for (const name of names) {
    const value = (item as Record<string, unknown>)[name];
    if (value === undefined || skip.includes(name)) continue;
    out[name] = typeof value === 'number' ? bits(value) : value;
  }
  return out;
}

const text = (label: QuadrantText | undefined) => label ?? null;

function quadrantFront(source: string): string {
  let db: QuadrantModel & { mermaidType: string };
  try {
    const model = parse(source);
    if (model.type !== 'quadrantChart') return JSON.stringify({ type: model.type });
    db = model as unknown as QuadrantModel & { mermaidType: string };
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'quadrantChart') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  return JSON.stringify({
    type: db.type,
    title: db.title ?? null,
    accTitle: db.accTitle ?? null,
    accDescr: db.accDescr ?? null,
    quadrants: db.quadrants.map(text),
    xAxisLeft: text(db.xAxisLeft),
    xAxisRight: text(db.xAxisRight),
    yAxisBottom: text(db.yAxisBottom),
    yAxisTop: text(db.yAxisTop),
    points: db.points.map((point) => ({ ...style(point, POINT, ['text', 'className']), text: point.text, className: point.className })),
    classes: [...db.classes].map(([name, shared]) => [name, style(shared, STYLE)]),
  });
}

export const quadrantOps: Record<string, (args: string[]) => string> = {
  quadrant_front: ([source]) => quadrantFront(JSON.parse(source) as string),
};
