// Architecture ops: Pele's model for a source, or Pele's error with its position, as
// crates/pele-core/tests/architecture_front.rs compares them, and Pele's grid placement of that
// model, as crates/pele-core/tests/architecture_layout.rs holds the port to it.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { ArchitectureDb } from '../vendor/pele/src/diagrams/architecture/db.ts';
import { placeOnGrid } from '../vendor/pele/src/diagrams/architecture/layout.ts';

type Parsed = { db: ArchitectureDb } | { answer: string };

function parsed(text: string): Parsed {
  try {
    const model = parse(text);
    if (model.type !== 'architecture') return { answer: JSON.stringify({ type: model.type }) };
    return { db: model as unknown as ArchitectureDb };
  } catch (error) {
    if (!(error instanceof PeleError)) return { answer: JSON.stringify({ error: String(error) }) };
    if (error.type !== null && error.type !== 'architecture') return { answer: JSON.stringify({ type: error.type }) };
    const { code, type, mermaidType, line, column, snippet } = error;
    return { answer: JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet }) };
  }
}

function architectureFront(text: string): string {
  const found = parsed(text);
  if ('answer' in found) return found.answer;
  const { db } = found;
  return JSON.stringify({
    model: {
      title: db.title,
      accTitle: db.accTitle,
      accDescr: db.accDescr,
      nodes: [...db.nodes.values()],
      groups: [...db.groups.values()],
      edges: db.edges,
      layoutHints: db.layoutHints,
    },
  });
}

function architectureGrid(text: string): string {
  const found = parsed(text);
  if ('answer' in found) return found.answer;
  const { db } = found;
  const grid = placeOnGrid([...db.nodes.values()], [...db.groups.values()], db.edges, db.layoutHints);
  return JSON.stringify(Object.fromEntries(Object.entries(grid).map(([name, values]) => [name, Array.from(values)])));
}

export const architectureOps: Record<string, (args: string[]) => string> = {
  architecture_front: ([source]) => architectureFront(JSON.parse(source) as string),
  architecture_grid: ([source]) => architectureGrid(JSON.parse(source) as string),
};
