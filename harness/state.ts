// State diagram ops for the oracle. `state_front` answers Pele's statement tree, db and graph for
// a source, or Pele's error with its position.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { StateDb } from '../vendor/pele/src/diagrams/state/db.ts';

// Each record's fields in the order its TypeScript interface declares them. A field that is
// undefined is left out, and a field the list does not name fails the request.
const FIELDS = {
  state: ['stmt', 'id', 'type', 'description', 'descriptions', 'doc', 'note', 'start', 'classes', 'styles', 'textStyles'],
  relation: ['stmt', 'state1', 'state2', 'description'],
  classDef: ['stmt', 'id', 'classes'],
  style: ['stmt', 'id', 'styleClass'],
  applyClass: ['stmt', 'id', 'styleClass'],
  dir: ['stmt', 'value'],
  click: ['stmt', 'id', 'url', 'tooltip'],
  note: ['position', 'text'],
  relationRecord: ['id1', 'id2', 'relationTitle'],
  classRecord: ['id', 'styles', 'textStyles'],
  link: ['url', 'tooltip'],
  node: ['id', 'shape', 'label', 'description', 'cssClasses', 'cssCompiledStyles', 'cssStyles', 'type', 'isGroup', 'parentId', 'dir', 'position', 'noteFor', 'colorIndex'],
  edge: ['id', 'start', 'end', 'label', 'classes', 'pattern'],
};

type Json = Record<string, unknown>;

function record(item: object, kind: keyof typeof FIELDS, nested: (name: string, value: unknown) => unknown = (_, value) => value): Json {
  const names = FIELDS[kind];
  for (const key of Object.keys(item)) {
    if (!names.includes(key)) throw new Error(`oracle: ${kind} has the unlisted field ${key}`);
  }
  const out: Json = {};
  for (const name of names) {
    const value = (item as Json)[name];
    if (value !== undefined) out[name] = nested(name, value);
  }
  return out;
}

function statement(item: unknown): unknown {
  if (typeof item === 'string') return item;
  const stmt = item as Json;
  const kind = stmt.stmt as keyof typeof FIELDS;
  if (!(kind in FIELDS)) throw new Error(`oracle: a statement of kind ${String(kind)}`);
  return record(stmt, kind, (name, value) => {
    if (name === 'doc') return (value as unknown[]).map(statement);
    if (name === 'note') return record(value as object, 'note');
    if (name === 'state1' || name === 'state2' || (name === 'id' && kind === 'click')) return statement(value);
    return value;
  });
}

function stateFront(text: string): string {
  let db: StateDb;
  try {
    const model = parse(text);
    if (model.type !== 'state') return JSON.stringify({ type: model.type });
    db = model as unknown as StateDb;
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'state') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  return JSON.stringify({
    model: {
      direction: db.direction,
      title: db.title,
      accTitle: db.accTitle,
      accDescr: db.accDescr,
      rootDoc: db.rootDoc.map(statement),
      states: [...db.states.values()].map(statement),
      relations: db.relations.map((relation) => record(relation, 'relationRecord')),
      classes: [...db.classes.values()].map((classDef) => record(classDef, 'classRecord')),
      links: [...db.links].map(([id, link]) => [id, record(link, 'link')]),
    },
    graph: {
      nodes: db.graph.nodes.map((node) => record(node, 'node')),
      edges: db.graph.edges.map((edge) => record(edge, 'edge')),
    },
  });
}

export const stateOps: Record<string, (args: string[]) => string> = {
  state_front: ([source]) => stateFront(JSON.parse(source) as string),
};
