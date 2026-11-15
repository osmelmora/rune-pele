// Venn ops: Pele's venn model for a source, or Pele's error with its position, and the geometry
// Pele places before it scales anything, so that the Rust port can be diffed one stage at a time.
// Numbers of the geometry travel as the bits of their doubles.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { VennModel } from '../vendor/pele/src/diagrams/venn/types.ts';
import { findSpot, layoutCircles, regionPath, type Circle } from '../vendor/pele/src/diagrams/venn/layout.ts';

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

// The parsed model of a venn source, or the answer for a source that is no venn diagram.
function model(text: string): VennModel | string {
  try {
    const parsed = parse(text);
    if (parsed.type !== 'venn') return JSON.stringify({ type: parsed.type });
    return parsed as VennModel;
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'venn') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
}

function vennFront(text: string): string {
  const db = model(text);
  if (typeof db === 'string') return db;
  const label = (value: string | undefined) => (value === undefined ? {} : { label: value });
  return JSON.stringify({
    type: db.type,
    title: db.title,
    subsets: db.subsets.map((s) => ({ sets: s.sets, size: bits(s.size), ...label(s.label) })),
    textNodes: db.textNodes.map((t) => ({ sets: t.sets, id: t.id, ...label(t.label) })),
    styles: db.styles.map((s) => ({ targets: s.targets, styles: [...s.styles] })),
  });
}

const keyOf = (ids: string[]): string => (ids.length === 1 ? ids[0] : JSON.stringify(ids));

// What render.ts works out before it scales: the circles, which circles touch, and for each
// region its circles and the spot for its text, then the outline each overlap would have.
function vennGeometry(text: string): string {
  const db = model(text);
  if (typeof db === 'string') return db;
  const circles = layoutCircles(db.subsets);
  const index = new Map<string, number>();
  circles.forEach((c, i) => index.set(c.id, i));
  const neighbors: number[][] = circles.map(() => []);
  for (let a = 0; a < circles.length; a++) {
    for (let b = a + 1; b < circles.length; b++) {
      if (Math.hypot(circles[a].x - circles[b].x, circles[a].y - circles[b].y) < circles[a].r + circles[b].r) {
        neighbors[a].push(b);
        neighbors[b].push(a);
      }
    }
  }
  const stated = new Map<string, string[]>();
  for (const subset of db.subsets) stated.set(keyOf(subset.sets), subset.sets);
  const regions = [...stated.values()].map((given) => {
    const inside = [...new Set(given)].map((id) => index.get(id)!);
    const within = new Set(inside);
    let smallest = inside[0];
    for (const i of inside) if (circles[i].r < circles[smallest].r) smallest = i;
    const others = neighbors[smallest].filter((i) => !within.has(i));
    let spot = findSpot(circles, inside, others);
    const across = others.length > 0 && spot.margin < circles[smallest].r * 0.2;
    if (across) spot = findSpot(circles, inside, []);
    const outline = inside.length > 1 ? regionPath(inside.map((i) => circles[i]), bits) : '';
    return { inside, others, across, spot: [bits(spot.x), bits(spot.y), bits(spot.margin)], outline };
  });
  return JSON.stringify({
    circles: circles.map((c) => [c.id, bits(c.x), bits(c.y), bits(c.r)]),
    neighbors,
    regions,
  });
}

// regionPath of circles given as the bits of x, y and r, with every number written as its bits.
function vennRegion(list: string): string {
  const circles: Circle[] = (JSON.parse(list) as string[][]).map(([x, y, r], i) => ({ id: String(i), x: double(x), y: double(y), r: double(r) }));
  return regionPath(circles, bits);
}

export const vennOps: Record<string, (args: string[]) => string> = {
  venn_front: ([source]) => vennFront(JSON.parse(source) as string),
  venn_geometry: ([source]) => vennGeometry(JSON.parse(source) as string),
  venn_region: ([circles]) => vennRegion(circles),
  acos: ([x]) => bits(Math.acos(double(x))),
  atan2: ([y, x]) => bits(Math.atan2(double(y), double(x))),
};
