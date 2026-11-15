// Oracle ops for the requirement diagram: Pele's model and the graph it lays out, as
// crates/pele-core/tests/requirement_front.rs compares them.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { RequirementDb } from '../vendor/pele/src/diagrams/requirement/db.ts';
import { buildRequirementGraph } from '../vendor/pele/src/diagrams/requirement/graph.ts';

// Pele's requirement model and graph for a source and parse options, or Pele's error with its
// position. A source of another type answers only that type.
function requirementFront(text: string, options: { config?: Record<string, unknown> }): string {
  let db: RequirementDb & { mermaidType: string };
  try {
    const model = parse(text, options);
    if (model.type !== 'requirement') return JSON.stringify({ type: model.type });
    db = model as unknown as RequirementDb & { mermaidType: string };
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'requirement') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  const graph = buildRequirementGraph(db);
  return JSON.stringify({
    mermaidType: db.mermaidType,
    direction: db.direction,
    requirements: [...db.requirements.values()],
    elements: [...db.elements.values()],
    relations: db.relations,
    classes: [...db.classes.values()],
    title: db.title ?? null,
    accTitle: db.accTitle ?? null,
    accDescr: db.accDescr ?? null,
    graph: {
      nodes: graph.nodes.map((node) => ({ id: node.id, cssClasses: node.cssClasses })),
      edges: graph.edges.map((edge) => ({ id: edge.id, start: edge.start ?? null, end: edge.end ?? null, type: edge.type, contains: edge.contains })),
    },
  });
}

export const requirementOps: Record<string, (args: string[]) => string> = {
  requirement_front: ([source, options]) => requirementFront(JSON.parse(source) as string, options ? JSON.parse(options) : {}),
};
