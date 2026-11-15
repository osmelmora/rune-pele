// Event modeling ops: Pele's model for a source, or Pele's error with its position, so that the
// front end can be diffed against it before anything is drawn.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { EventModelingModel } from '../vendor/pele/src/diagrams/eventmodeling/model.ts';

function failure(error: unknown): string {
  if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
  if (error.type !== null && error.type !== 'eventmodeling') return JSON.stringify({ type: error.type });
  const { code, type, mermaidType, line, column, snippet } = error;
  return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
}

// Boxes name their lane by its place among the lanes, and relations their boxes by theirs.
function eventmodelingFront(text: string): string {
  let parsed;
  try {
    parsed = parse(text);
  } catch (error) {
    return failure(error);
  }
  if (parsed.type !== 'eventmodeling') return JSON.stringify({ type: parsed.type });
  const model = parsed as EventModelingModel;
  const lane = (l: EventModelingModel['lanes'][number]) => ({ index: l.index, label: l.label, group: l.group, namespace: l.namespace ?? null });
  return JSON.stringify({
    type: model.type,
    title: model.title ?? null,
    accTitle: model.accTitle ?? null,
    accDescr: model.accDescr ?? null,
    boxes: model.boxes.map((box) => ({
      id: box.id,
      kind: box.kind,
      entity: box.entity,
      name: box.name,
      namespace: box.namespace ?? null,
      reset: box.reset,
      lane: model.lanes.indexOf(box.lane),
      data: box.data ?? null,
      dataType: box.dataType ?? null,
    })),
    lanes: model.lanes.map(lane),
    relations: model.relations.map((relation) => [model.boxes.indexOf(relation.source), model.boxes.indexOf(relation.target)]),
    notes: model.notes.map((note) => ({ frame: note.frame, text: note.text, dataType: note.dataType ?? null })),
    specifications: model.specifications.map((spec) => ({ frame: spec.frame, given: spec.given, when: spec.when, then: spec.then })),
    entities: model.entities,
  });
}

export const eventmodelingOps: Record<string, (args: string[]) => string> = {
  eventmodeling_front: ([source]) => eventmodelingFront(JSON.parse(source) as string),
};
