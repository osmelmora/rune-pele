// Oracle ops for use case diagrams: Pele's model and graph for a source, or its error.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import { buildUsecaseGraph } from '../vendor/pele/src/diagrams/usecase/graph.ts';
import type { UsecaseModel } from '../vendor/pele/src/diagrams/usecase/types.ts';

// Each record's fields in the order its TypeScript interface declares them. A field that is
// undefined is left out, and a field the list does not name fails the request.
const FIELDS = {
  actor: ['id', 'label', 'labelType', 'type', 'icon', 'business', 'stereotype', 'parentId', 'classes', 'styles'],
  useCase: ['id', 'label', 'labelType', 'shape', 'business', 'stereotype', 'parentId', 'classes', 'styles'],
  boundary: ['id', 'label', 'labelType', 'type', 'members', 'classes', 'styles'],
  relationship: ['id', 'explicitId', 'source', 'target', 'type', 'arrowType', 'label', 'labelType', 'minlen', 'classes', 'styles', 'animate', 'animation'],
  note: ['id', 'target', 'label', 'labelType'],
  classDef: ['id', 'styles'],
  graphNode: ['id', 'label', 'labelType', 'shape', 'isGroup', 'padding', 'cssClasses', 'cssStyles', 'cssCompiledStyles', 'actorType', 'business', 'icon', 'stereotype', 'parentId', 'noteTarget', 'jsonRows', 'boundaryType'],
  graphEdge: ['id', 'start', 'end', 'relationshipType', 'pattern', 'arrowTypeStart', 'arrowTypeEnd', 'label', 'labelType', 'classes', 'style', 'cssCompiledStyles', 'animate', 'animation', 'minlen', 'explicitId', 'internal'],
};

function record(item: object, kind: keyof typeof FIELDS): Record<string, unknown> {
  const names = FIELDS[kind];
  for (const key of Object.keys(item)) {
    if (!names.includes(key)) throw new Error(`oracle: ${kind} has the unlisted field ${key}`);
  }
  const out: Record<string, unknown> = {};
  for (const name of names) {
    const value = (item as Record<string, unknown>)[name];
    if (value !== undefined) out[name] = value;
  }
  return out;
}

// A JSON node's value can be nested deeper than JSON.stringify can follow, and loses the order of
// integer-like keys. The order of its keys by JSON Pointer says what the port has to agree on;
// the graph's rows carry the values.
function order(propertyOrder: Map<string, string[]>): [string, string[]][] {
  return [...propertyOrder].sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
}

// Pele's use case model and graph for a source, or Pele's error with its position. A source of
// another type answers only that type. A message can hold half of a surrogate pair, which a
// Rust string cannot, so both sides show it as U+FFFD.
function usecaseFront(text: string): string {
  let model: UsecaseModel & { mermaidType: string };
  try {
    const parsed = parse(text);
    if (parsed.type !== 'usecase') return JSON.stringify({ type: parsed.type });
    model = parsed as unknown as UsecaseModel & { mermaidType: string };
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'usecase') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error).toWellFormed(), code, type, mermaidType, line, column, snippet });
  }
  const graph = buildUsecaseGraph(model);
  return JSON.stringify({
    model: {
      type: model.type,
      mermaidType: model.mermaidType,
      actors: [...model.actors.values()].map((actor) => record(actor, 'actor')),
      useCases: [...model.useCases.values()].map((useCase) => record(useCase, 'useCase')),
      boundaries: [...model.systemBoundaries.values()].map((boundary) => record(boundary, 'boundary')),
      relationships: model.relationships.map((relationship) => record(relationship, 'relationship')),
      notes: [...model.notes.values()].map((note) => record(note, 'note')),
      jsonNodes: [...model.jsonNodes.values()].map((json) => ({
        id: json.id,
        order: order(json.propertyOrder),
        classes: json.classes,
        styles: json.styles,
      })),
      classDefs: [...model.classDefs.values()].map((classDef) => record(classDef, 'classDef')),
      direction: model.direction,
      title: model.title,
      accTitle: model.accTitle,
      accDescr: model.accDescr,
    },
    graph: {
      nodes: graph.nodes.map((node) => record(node, 'graphNode')),
      edges: graph.edges.map((edge) => record(edge, 'graphEdge')),
    },
  });
}

export const usecaseOps: Record<string, (args: string[]) => string> = {
  usecase_front: ([source]) => usecaseFront(JSON.parse(source) as string),
};
