// Cynefin ops: Pele's model for a source, or Pele's error with its position, as
// crates/pele-core/tests/cynefin_front.rs compares them.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { CynefinModel } from '../vendor/pele/src/diagrams/cynefin/model.ts';

function cynefinFront(text: string): string {
  let model: CynefinModel;
  try {
    const parsed = parse(text);
    if (parsed.type !== 'cynefin') return JSON.stringify({ type: parsed.type });
    model = parsed as unknown as CynefinModel;
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'cynefin') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  return JSON.stringify({
    model: {
      title: model.title,
      accTitle: model.accTitle,
      accDescr: model.accDescr,
      domains: [...model.domains.values()],
      transitions: model.transitions,
    },
  });
}

export const cynefinOps: Record<string, (args: string[]) => string> = {
  cynefin_front: ([source]) => cynefinFront(JSON.parse(source) as string),
};
