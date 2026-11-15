// Sequence diagram ops: Pele's sequence model for a source, or Pele's error with its position,
// so that the Rust front end can be diffed against them.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { SeqDb } from '../vendor/pele/src/diagrams/sequence/db.ts';
import { numberMessages } from '../vendor/pele/src/diagrams/sequence/render.ts';

// Each record's fields in a fixed order. A field that is undefined is left out, and a field the
// list does not name fails the request.
const FIELDS = {
  actor: ['box', 'name', 'description', 'wrap', 'prevActor', 'nextActor', 'links', 'properties', 'details', 'type'],
  box: ['name', 'wrap', 'fill', 'actorKeys'],
  message: ['id', 'from', 'to', 'message', 'wrap', 'type', 'activate', 'placement', 'centralConnection', 'answer'],
  note: ['actor', 'placement', 'message', 'wrap'],
  numbering: ['start', 'step', 'visible'],
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

// A number as JavaScript prints it, since JSON has no Infinity.
const printed = (value: number): string => String(value);

function sequenceFront(text: string): string {
  let db: SeqDb & { mermaidType: string };
  try {
    const model = parse(text);
    if (model.type !== 'sequence') return JSON.stringify({ type: model.type });
    db = model as unknown as SeqDb & { mermaidType: string };
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'sequence') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  const numbering = numberMessages(db.messages, false);
  const menu = (entries: Map<string, unknown>) => [...entries];
  const model = {
    type: db.type,
    mermaidType: db.mermaidType,
    actors: [...db.actors].map(([id, actor]) => [
      id,
      record(actor, 'actor', { box: (box: never) => db.boxes.indexOf(box), links: menu, properties: menu }),
    ]),
    createdActors: [...db.createdActors],
    destroyedActors: [...db.destroyedActors],
    boxes: db.boxes.map((box) => record(box, 'box')),
    messages: db.messages.map((message) =>
      record(message, 'message', {
        message: (value: string | object) =>
          typeof value === 'string' ? value : record(value, 'numbering', { start: printed, step: printed }),
      })
    ),
    notes: db.notes.map((note) => record(note, 'note')),
    title: db.title,
    accTitle: db.accTitle,
    accDescr: db.accDescr,
    numbers: numbering.index.map(printed),
    shown: numbering.shown,
    numbered: numbering.visible,
  };
  try {
    return JSON.stringify({ model });
  } catch (error) {
    // A menu's JSON can nest deeper than JSON.stringify can follow.
    return JSON.stringify({ unprintable: String(error) });
  }
}

export const sequenceOps: Record<string, (args: string[]) => string> = {
  sequence_front: ([source]) => sequenceFront(JSON.parse(source) as string),
};
