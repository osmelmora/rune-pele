// ER diagram ops: Pele's ER model and ER graph for a source, or Pele's error with its position,
// so that the Rust front end can be diffed against them.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { ErDb } from '../vendor/pele/src/diagrams/er/db.ts';
import { buildErGraph } from '../vendor/pele/src/diagrams/er/graph.ts';

// Each record's fields in the order its TypeScript interface declares them. A field that is
// undefined is left out, and a field the list does not name fails the request.
const FIELDS = {
  entity: ['id', 'label', 'alias', 'attributes', 'cssClasses', 'cssStyles'],
  attribute: ['type', 'name', 'keys', 'comment'],
  relationship: ['entityA', 'roleA', 'entityB', 'relSpec'],
  relSpec: ['cardA', 'relType', 'cardB'],
  classDef: ['id', 'styles', 'textStyles'],
  subgraph: ['id', 'nodes', 'title', 'classes', 'cssStyles', 'dir', 'labelType'],
  graphNode: ['id', 'label', 'alias', 'attributes', 'isGroup', 'parentId', 'cssStyles', 'cssCompiledStyles', 'cssClasses', 'dir', 'labelType'],
  graphEdge: ['id', 'start', 'end', 'label', 'arrowTypeStart', 'arrowTypeEnd', 'pattern'],
};

function record(item: object, kind: keyof typeof FIELDS, map: Record<string, (value: never) => unknown> = {}): Record<string, unknown> {
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

const attributes = (list: object[]) => list.map((a) => record(a, 'attribute'));

function erFront(text: string): string {
  let db: ErDb & { mermaidType: string };
  try {
    const model = parse(text);
    if (model.type !== 'er') return JSON.stringify({ type: model.type });
    db = model as unknown as ErDb & { mermaidType: string };
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'er') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  const graph = buildErGraph(db);
  const model = {
    type: db.type,
    mermaidType: db.mermaidType,
    direction: db.direction,
    entities: [...db.entities.values()].map((e) => record(e, 'entity', { attributes })),
    relationships: db.relationships.map((r) => record(r, 'relationship', { relSpec: (s: object) => record(s, 'relSpec') })),
    classes: [...db.classes.values()].map((c) => record(c, 'classDef')),
    subgraphs: db.subgraphs.map((s) => record(s, 'subgraph')),
    title: db.title,
    accTitle: db.accTitle,
    accDescr: db.accDescr,
  };
  return JSON.stringify({
    model,
    graph: {
      nodes: graph.nodes.map((n) => record(n, 'graphNode', { attributes })),
      edges: graph.edges.map((e) => record(e, 'graphEdge')),
    },
  });
}

export const erOps: Record<string, (args: string[]) => string> = {
  er_front: ([source]) => erFront(JSON.parse(source) as string),
};
