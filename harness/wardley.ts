// Oracle ops for wardley maps: Pele's model for a source, or Pele's error with its position, as
// crates/pele-core/tests/wardley_front.rs compares them. Each field is written in a fixed order,
// with an undefined one as null, so that how Pele's builder spreads its objects does not show.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { WardleyModel } from '../vendor/pele/src/diagrams/wardley/model.ts';

// JSON has no Infinity, and -0 prints as 0.
const n = (value: number | undefined): unknown =>
  value === undefined ? null : Object.is(value, -0) || !Number.isFinite(value) ? { $number: Object.is(value, -0) ? '-0' : String(value) } : value;
const text = (value: string | undefined): string | null => value ?? null;
const flag = (value: boolean | undefined): boolean | null => value ?? null;

function wardleyFront(source: string): string {
  let model: WardleyModel;
  try {
    const parsed = parse(source);
    if (parsed.type !== 'wardley') return JSON.stringify({ type: parsed.type });
    model = parsed as unknown as WardleyModel;
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'wardley') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  const mark = (key: string) => (m: { name?: string; text?: string; x: number; y: number }) => ({
    [key]: (m as Record<string, string>)[key],
    x: n(m.x),
    y: n(m.y),
  });
  return JSON.stringify({
    model: {
      title: text(model.title),
      accTitle: text(model.accTitle),
      accDescr: text(model.accDescr),
      nodes: model.nodes.map((node) => ({
        id: node.id,
        label: node.label,
        x: n(node.x),
        y: n(node.y),
        className: text(node.className),
        labelOffsetX: n(node.labelOffsetX),
        labelOffsetY: n(node.labelOffsetY),
        inPipeline: node.inPipeline === true,
        isPipelineParent: node.isPipelineParent === true,
        inertia: flag(node.inertia),
        sourceStrategy: text(node.sourceStrategy),
      })),
      links: model.links.map((link) => ({
        source: link.source,
        target: link.target,
        dashed: link.dashed === true,
        label: text(link.label),
        flow: text(link.flow),
      })),
      trends: model.trends.map((trend) => ({ nodeId: trend.nodeId, targetX: n(trend.targetX), targetY: n(trend.targetY) })),
      pipelines: model.pipelines.map((pipeline) => ({ nodeId: pipeline.nodeId, componentIds: pipeline.componentIds })),
      annotations: model.annotations.map((annotation) => ({
        number: n(annotation.number),
        coordinates: annotation.coordinates.map((at) => ({ x: n(at.x), y: n(at.y) })),
        text: text(annotation.text),
      })),
      notes: model.notes.map(mark('text')),
      accelerators: model.accelerators.map(mark('name')),
      deaccelerators: model.deaccelerators.map(mark('name')),
      annotationsBox: model.annotationsBox ? { x: n(model.annotationsBox.x), y: n(model.annotationsBox.y) } : null,
      axes: {
        stages: model.axes.stages ?? null,
        stageBoundaries: model.axes.stageBoundaries ? model.axes.stageBoundaries.map(n) : null,
      },
      size: model.size ? { width: n(model.size.width), height: n(model.size.height) } : null,
    },
  });
}

export const wardleyOps: Record<string, (args: string[]) => string> = {
  wardley_front: ([source]) => wardleyFront(JSON.parse(source) as string),
};
