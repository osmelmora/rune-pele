// Oracle ops for block diagrams: Pele's model for a source, or Pele's error with its position, as
// crates/pele-core/tests/block_front.rs compares them, and Pele's grid layout, as
// crates/pele-core/tests/block_layout.rs holds the port to it. harness/build.mjs routes the
// layout through `globalThis.peleCommonTap['layout.layout']`, which is how a call made by a real
// render is seen.
import { PeleError, parse, render } from '../vendor/pele/src/index.ts';
import type { BlockDb } from '../vendor/pele/src/diagrams/block/db.ts';
import { layout, type Cell } from '../vendor/pele/src/diagrams/block/layout.ts';
import type { Block } from '../vendor/pele/src/diagrams/block/types.ts';
import { metricsMeasurer } from '../vendor/pele/src/text/measurer.ts';

// The fields of Pele's `Block` interface, in its order. A block that a link statement headed
// also holds that statement's items under the keys 0, 1, …, which nothing reads.
const FIELDS = [
  'id', 'type', 'label', 'children', 'columns', 'widthInColumns', 'width', 'directions', 'classes', 'styles',
  'colorIndex', 'start', 'end', 'arrowTypeStart', 'arrowTypeEnd', 'thickness', 'pattern', 'css', 'styleClass', 'stylesStr',
];

// A block's fields in order, without the undefined ones. Its children are in full when `nested`,
// or else only counted.
function record(block: Block, nested: boolean): Record<string, unknown> {
  for (const key of Object.keys(block)) {
    if (!FIELDS.includes(key) && !/^\d+$/.test(key)) throw new Error(`oracle: a block has the unlisted field ${key}`);
  }
  const out: Record<string, unknown> = {};
  for (const name of FIELDS) {
    const value = (block as unknown as Record<string, unknown>)[name];
    if (value === undefined) continue;
    out[name] = name === 'children' ? (nested ? (value as Block[]).map((child) => record(child, true)) : (value as Block[]).length) : value;
  }
  return out;
}

// JSON has no Infinity, and a count of 400 digits parses to one.
const numbers = (_key: string, value: unknown): unknown =>
  typeof value === 'number' && !Number.isFinite(value) ? { $number: String(value) } : value;

function blockFront(text: string): string {
  let db: BlockDb;
  try {
    const model = parse(text);
    if (model.type !== 'block') return JSON.stringify({ type: model.type });
    db = model as unknown as BlockDb;
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'block') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  const model = {
    title: db.title,
    root: record(db.root, true),
    edges: db.edges.map((edge) => record(edge, false)),
    classes: [...db.classes.values()],
    byId: [...db.byId].map(([id, block]) => [id, record(block, false)]),
  };
  return JSON.stringify({ model }, numbers);
}

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
// A hole in a gap list counts as 0 everywhere layout reads it, so it travels as one.
const dense = (values: number[]): number[] => Array.from(values, (value) => value ?? 0);

// The doubles of a cell, in this order.
const NUMBERS = ['columns', 'span', 'head', 'nw', 'nh', 'col', 'row', 'cols', 'x', 'y', 'w', 'h'] as const;
type Json = Record<string, unknown>;

// A tree of cells as a list in breadth-first order, each child named by its place in the list.
function treeOut(root: Cell): Json[] {
  const order = [root];
  for (let i = 0; i < order.length; i++) for (const child of order[i].children) order.push(child);
  const at = new Map(order.map((cell, i) => [cell, i]));
  return order.map((cell) => ({
    children: cell.children.map((child) => at.get(child)),
    space: cell.space,
    n: pack(NUMBERS.map((name) => cell[name])),
    heights: pack(cell.heights),
    rowGaps: pack(dense(cell.rowGaps)),
    colGaps: pack(dense(cell.colGaps)),
  }));
}

function treeIn(list: Json[]): Cell {
  const cells = list.map((item) => {
    const cell = { children: [], space: item.space } as unknown as Cell;
    unpack(item.n as string).forEach((value, i) => ((cell as unknown as Record<string, number>)[NUMBERS[i]] = value));
    cell.heights = unpack(item.heights as string);
    cell.rowGaps = unpack(item.rowGaps as string);
    cell.colGaps = unpack(item.colGaps as string);
    return cell;
  });
  list.forEach((item, i) => (cells[i].children = (item.children as number[]).map((child) => cells[child])));
  return cells[0];
}

// What one call of `layout` was given and left behind, or the error it threw.
function run(real: (root: Cell, gap: number) => { width: number; height: number }, root: Cell, gap: number): Json {
  const call: Json = { in: treeOut(root), gap: bits(gap) };
  try {
    const size = real(root, gap);
    call.out = treeOut(root);
    call.size = pack([size.width, size.height]);
  } catch (error) {
    call.error = String(error);
  }
  return call;
}

type Tap = Record<string, (real: (...args: never[]) => unknown, args: never[]) => unknown>;
const PARITY = { measurer: metricsMeasurer, now: Date.UTC(2026, 0, 15, 12) };

// Renders a source with the options given and answers every layout call the render made.
function layoutRecord(source: string, options: object): string {
  const calls: Json[] = [];
  const tap: Tap = {
    'layout.layout': (real, [root, gap]: [Cell, number]) => {
      const call = run(real as never, root, gap);
      calls.push(call);
      if (call.error !== undefined) throw new Error('replayed');
      return { width: unpack(call.size as string)[0], height: unpack(call.size as string)[1] };
    },
  };
  (globalThis as { peleCommonTap?: Tap }).peleCommonTap = tap;
  try {
    render(source, { ...PARITY, ...options });
  } catch {
    // A source Pele rejects still made the calls it made.
  } finally {
    (globalThis as { peleCommonTap?: Tap }).peleCommonTap = undefined;
  }
  return JSON.stringify({ calls });
}

export const blockOps: Record<string, (args: string[]) => string> = {
  block_front: ([source]) => blockFront(JSON.parse(source) as string),
  block_layout_record: ([source, options]) => layoutRecord(JSON.parse(source) as string, options ? JSON.parse(options) : {}),
  block_layout_run: ([call]) => {
    const { in: tree, gap } = JSON.parse(call) as { in: Json[]; gap: string };
    return JSON.stringify(run(layout, treeIn(tree), double(gap)));
  },
};
