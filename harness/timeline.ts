// Timeline ops: Pele's model for a source, or Pele's error with its position, as
// crates/pele-core/tests/timeline_front.rs compares them.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { TimelineModel } from '../vendor/pele/src/diagrams/timeline/db.ts';

function timelineFront(text: string, limit?: string): string {
  let model: TimelineModel;
  try {
    const parsed = parse(text, limit === undefined ? {} : { limit: Number(limit) });
    if (parsed.type !== 'timeline') return JSON.stringify({ type: parsed.type });
    model = parsed as unknown as TimelineModel;
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'timeline') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  return JSON.stringify({
    model: {
      title: model.title,
      accTitle: model.accTitle,
      accDescr: model.accDescr,
      direction: model.direction,
      sections: model.sections,
      periods: model.periods.map(({ text, events, section, sectionIndex }) => ({ text, events, section, sectionIndex })),
    },
  });
}

export const timelineOps: Record<string, (args: string[]) => string> = {
  timeline_front: ([source, limit]) => timelineFront(JSON.parse(source) as string, limit),
};
