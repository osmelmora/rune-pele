// Radar ops: Pele's model for a source, or Pele's error with its position, so that the front end
// can be diffed against it before anything is drawn.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { RadarModel } from '../vendor/pele/src/diagrams/radar/model.ts';

function failure(error: unknown): string {
  if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
  if (error.type !== null && error.type !== 'radar') return JSON.stringify({ type: error.type });
  const { code, type, mermaidType, line, column, snippet } = error;
  return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
}

// Each number as `String(number)`, since JSON has no Infinity.
function radarFront(text: string): string {
  let parsed;
  try {
    parsed = parse(text);
  } catch (error) {
    return failure(error);
  }
  if (parsed.type !== 'radar') return JSON.stringify({ type: parsed.type });
  const model = parsed as RadarModel;
  const { showLegend, ticks, max, min, graticule } = model.options;
  return JSON.stringify({
    type: model.type,
    title: model.title ?? null,
    accTitle: model.accTitle ?? null,
    accDescr: model.accDescr ?? null,
    axes: model.axes.map((axis) => ({ name: axis.name, label: axis.label })),
    curves: model.curves.map((curve) => ({ name: curve.name, label: curve.label, entries: curve.entries.map(String) })),
    options: { showLegend, ticks: String(ticks), max: max === null ? null : String(max), min: String(min), graticule },
  });
}

export const radarOps: Record<string, (args: string[]) => string> = {
  radar_front: ([source]) => radarFront(JSON.parse(source) as string),
};
