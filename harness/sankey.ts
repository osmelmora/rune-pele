// Oracle ops for sankey diagrams: Pele's model for a source, or Pele's error with its position,
// as crates/pele-core/tests/sankey_front.rs compares them, and Pele's layout of bare flows, as
// crates/pele-core/tests/sankey_layout.rs holds the port to it. Doubles travel as 16 hex digits
// of their bits.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { SankeyDb, SankeyModel, SankeyNode } from '../vendor/pele/src/diagrams/sankey/db.ts';
import { layoutSankey, type Alignment } from '../vendor/pele/src/diagrams/sankey/layout.ts';

function double(hex: string): number {
  const view = new DataView(new ArrayBuffer(8));
  view.setBigUint64(0, BigInt('0x' + hex));
  return view.getFloat64(0);
}

function bits(value: number): string {
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, value);
  return view.getBigUint64(0).toString(16).padStart(16, '0');
}

function sankeyFront(text: string, options: { config?: Record<string, unknown> }): string {
  let db: SankeyDb;
  try {
    const parsed = parse(text, options);
    if (parsed.type !== 'sankey') return JSON.stringify({ type: parsed.type });
    db = parsed as unknown as SankeyDb;
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'sankey') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  return JSON.stringify({
    model: {
      nodes: db.nodes.map((node) => node.id),
      links: db.links.map((link) => [link.source.index, link.target.index, bits(link.value)]),
      title: db.title ?? null,
    },
  });
}

interface LayoutCall {
  nodes: number;
  links: [number, number, string][];
  options: { width: string; height: string; nodeWidth: string; nodePadding: string; labelHeight: string; alignment: Alignment };
}

// Pele's layout of the flows, every number as its bits, or the message of the error it throws.
function sankeyLayout(call: LayoutCall): string {
  const nodes: SankeyNode[] = Array.from({ length: call.nodes }, (_, index) => ({ id: `n${index}`, index }));
  const model: SankeyModel = {
    type: 'sankey',
    nodes,
    links: call.links.map(([source, target, value]) => ({ source: nodes[source], target: nodes[target], value: double(value) })),
  };
  const { width, height, nodeWidth, nodePadding, labelHeight, alignment } = call.options;
  try {
    const layout = layoutSankey(model, {
      width: double(width),
      height: double(height),
      nodeWidth: double(nodeWidth),
      nodePadding: double(nodePadding),
      labelHeight: double(labelHeight),
      alignment,
    });
    return JSON.stringify({
      nodes: layout.nodes.map((node) => [node.layer, bits(node.value), bits(node.x), bits(node.y), bits(node.height)]),
      links: layout.links.map((link) => [bits(link.value), bits(link.thickness), bits(link.sourceY), bits(link.targetY)]),
    });
  } catch (error) {
    return JSON.stringify({ error: String(error) });
  }
}

export const sankeyOps: Record<string, (args: string[]) => string> = {
  sankey_front: ([source, options]) => sankeyFront(JSON.parse(source) as string, options ? JSON.parse(options) : {}),
  sankey_layout: ([call]) => sankeyLayout(JSON.parse(call) as LayoutCall),
};
