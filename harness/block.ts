// Oracle ops for block diagrams: Pele's model for a source, or Pele's error with its position, as
// crates/pele-core/tests/block_front.rs compares them.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { BlockDb } from '../vendor/pele/src/diagrams/block/db.ts';
import type { Block } from '../vendor/pele/src/diagrams/block/types.ts';

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

export const blockOps: Record<string, (args: string[]) => string> = {
  block_front: ([source]) => blockFront(JSON.parse(source) as string),
};
