// Class diagram ops: Pele's class model and class graph for a source, or Pele's error with its
// position, so that the Rust front end can be diffed against them.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { ClassDb } from '../vendor/pele/src/diagrams/class/db.ts';
import { buildClassGraph } from '../vendor/pele/src/diagrams/class/graph.ts';
import type { ClassMember } from '../vendor/pele/src/diagrams/class/members.ts';
import { preprocess } from '../vendor/pele/src/preprocess.ts';

// Each record's fields in the order its TypeScript interface declares them. A field that is
// undefined is left out, and a field the list does not name fails the request.
const FIELDS = {
  class: ['id', 'type', 'label', 'cssClasses', 'methods', 'members', 'annotations', 'styles', 'parent', 'link', 'linkTarget', 'haveCallback', 'tooltip'],
  member: ['id', 'memberType', 'visibility', 'classifier', 'parameters', 'returnType'],
  relation: ['id1', 'id2', 'relationTitle1', 'relationTitle2', 'title', 'relation'],
  note: ['id', 'class', 'text', 'index', 'parent'],
  namespace: ['id', 'label', 'classes', 'notes', 'children', 'parent', 'explicit'],
  interface: ['id', 'label', 'classId'],
  styleClass: ['id', 'styles', 'textStyles'],
  graphNode: ['id', 'label', 'isGroup', 'shape', 'parentId', 'parent', 'classNode', 'colorIndex'],
  graphEdge: ['id', 'start', 'end', 'source', 'target', 'label', 'arrowTypeStart', 'arrowTypeEnd', 'startLabelRight', 'endLabelLeft', 'pattern', 'classes'],
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

// A member, with the text Pele draws for it.
function member(m: ClassMember): Record<string, unknown> {
  return { ...record(m, 'member'), display: m.getDisplayDetails().displayText };
}

const keys = (map: Map<string, unknown>): string[] => [...map.keys()];

function classFront(text: string, options: { config?: Record<string, unknown> }): string {
  let db: ClassDb & { mermaidType: string };
  try {
    const model = parse(text, options);
    if (model.type !== 'class') return JSON.stringify({ type: model.type });
    db = model as unknown as ClassDb & { mermaidType: string };
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'class') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  const pre = preprocess(text).config;
  const config = options.config ? { ...options.config, ...pre } : pre;
  const conf = (config.class ?? {}) as Record<string, unknown>;
  const graph = buildClassGraph(db, conf.hierarchicalNamespaces !== false);
  const members = (list: ClassMember[]) => list.map(member);
  const model = {
    type: db.type,
    mermaidType: db.mermaidType,
    direction: db.direction,
    classes: [...db.classes.values()].map((c) => record(c, 'class', { methods: members, members })),
    relations: db.relations.map((r) => record(r, 'relation')),
    notes: [...db.notes.values()].map((n) => record(n, 'note')),
    namespaces: [...db.namespaces.values()].map((n) => record(n, 'namespace', { classes: keys, notes: keys, children: keys })),
    interfaces: db.interfaces.map((i) => record(i, 'interface')),
    styleClasses: [...db.styleClasses.values()].map((s) => record(s, 'styleClass')),
    title: db.title,
    accTitle: db.accTitle,
    accDescr: db.accDescr,
  };
  return JSON.stringify({
    model,
    graph: {
      nodes: graph.nodes.map((n) => record(n, 'graphNode', { classNode: (c: { id: string }) => c.id })),
      edges: graph.edges.map((e) => record(e, 'graphEdge')),
    },
  });
}

export const classOps: Record<string, (args: string[]) => string> = {
  class_front: ([source, options]) => classFront(JSON.parse(source) as string, options ? JSON.parse(options) : {}),
};
