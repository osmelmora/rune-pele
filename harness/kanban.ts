// Oracle ops for kanban boards: Pele's model for a source, or Pele's error with its position, as
// crates/pele-core/tests/kanban_front.rs compares them. Each field is written in a fixed order,
// with an undefined one as null, so that how Pele's db builds its objects does not show.
// `front` is shared with harness/mindmap.ts and harness/ishikawa.ts.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { KanbanModel, KanbanNode } from '../vendor/pele/src/diagrams/kanban/db.ts';

export function front(source: string, type: string, model: (parsed: unknown) => unknown): string {
  let parsed: { type: string };
  try {
    parsed = parse(source) as { type: string };
    if (parsed.type !== type) return JSON.stringify({ type: parsed.type });
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== type) return JSON.stringify({ type: error.type });
    const { code, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type: error.type, mermaidType, line, column, snippet });
  }
  return JSON.stringify({ model: model(parsed) });
}

const card = (node: KanbanNode) => ({
  id: node.id,
  level: node.level,
  label: node.label,
  icon: node.icon ?? null,
  assigned: node.assigned ?? null,
  ticket: node.ticket ?? null,
  priority: node.priority ?? null,
  cssClasses: node.cssClasses ?? null,
});

function kanban(parsed: unknown): unknown {
  const model = parsed as KanbanModel;
  return {
    title: model.title ?? null,
    sections: model.sections.map((section) => ({ ...card(section), items: section.items.map(card) })),
  };
}

export const kanbanOps: Record<string, (args: string[]) => string> = {
  kanban_front: ([source]) => front(JSON.parse(source) as string, 'kanban', kanban),
};
