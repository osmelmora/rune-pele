// Journey ops: Pele's model for a source, or Pele's error with its position, as
// crates/pele-core/tests/journey_front.rs compares them. A score travels as the bits of its
// double, since it may be NaN.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { JourneyModel } from '../vendor/pele/src/diagrams/journey/db.ts';

function bits(value: number): string {
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, value);
  return view.getBigUint64(0).toString(16).padStart(16, '0');
}

function journeyFront(text: string, limit?: string): string {
  let model: JourneyModel;
  try {
    const parsed = parse(text, limit === undefined ? {} : { limit: Number(limit) });
    if (parsed.type !== 'journey') return JSON.stringify({ type: parsed.type });
    model = parsed as unknown as JourneyModel;
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'journey') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  return JSON.stringify({
    model: {
      title: model.title,
      accTitle: model.accTitle,
      accDescr: model.accDescr,
      sections: model.sections,
      tasks: model.tasks.map(({ task, score, people, section }) => ({ task, score: bits(score), people, section })),
    },
  });
}

export const journeyOps: Record<string, (args: string[]) => string> = {
  journey_front: ([source, limit]) => journeyFront(JSON.parse(source) as string, limit),
};
