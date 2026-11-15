// Treemap ops: Pele's model for a source, or Pele's error with its position; Pele's squarified
// layout of a tree; and Pele's value format. crates/pele-core/tests/treemap_front.rs and
// treemap_layout.rs compare them. Numbers travel as the bits of their doubles.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import { formatter } from '../vendor/pele/src/diagrams/treemap/format.ts';
import { layoutTreemap } from '../vendor/pele/src/diagrams/treemap/layout.ts';
import type { TreemapModel, TreemapNode } from '../vendor/pele/src/diagrams/treemap/model.ts';

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

interface NodeJson {
  name: string;
  children?: NodeJson[];
  value?: string;
  classSelector?: string;
  cssCompiledStyles?: string[];
}

const written = (node: TreemapNode): NodeJson => ({
  name: node.name,
  children: node.children?.map(written),
  value: node.value === undefined ? undefined : bits(node.value),
  classSelector: node.classSelector,
  cssCompiledStyles: node.cssCompiledStyles,
});

const read = (node: NodeJson): TreemapNode => ({
  name: node.name,
  children: node.children?.map(read),
  value: node.value === undefined ? undefined : double(node.value),
});

function treemapFront(text: string, limit?: string): string {
  let model: TreemapModel;
  try {
    const parsed = parse(text, limit === undefined ? {} : { limit: Number(limit) });
    if (parsed.type !== 'treemap') return JSON.stringify({ type: parsed.type });
    model = parsed as unknown as TreemapModel;
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'treemap') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  return JSON.stringify({
    model: {
      title: model.title,
      accTitle: model.accTitle,
      accDescr: model.accDescr,
      roots: model.roots.map(written),
      classes: [...model.classes],
    },
  });
}

function treemapLayout(input: string): string {
  const { roots, width, height, gap, header, short, side } = JSON.parse(input) as {
    roots: NodeJson[];
    width: string;
    height: string;
    gap: string;
    header: string;
    short: string;
    side: string;
  };
  const boxes = layoutTreemap(roots.map(read), double(width), double(height), {
    gap: double(gap),
    header: double(header),
    short: double(short),
    side: double(side),
  });
  const index = new Map(boxes.map((box, i) => [box, i]));
  return JSON.stringify(
    boxes.map((box) => ({
      name: box.node.name,
      parent: box.parent === undefined ? -1 : (index.get(box.parent) ?? -2),
      series: box.series,
      value: bits(box.value),
      headed: box.headed,
      header: bits(box.header),
      x: bits(box.x),
      y: bits(box.y),
      w: bits(box.w),
      h: bits(box.h),
    }))
  );
}

export const treemapOps: Record<string, (args: string[]) => string> = {
  treemap_front: ([source, limit]) => treemapFront(JSON.parse(source) as string, limit),
  treemap_layout: ([input]) => treemapLayout(input),
  treemap_format: ([spec, value]) => formatter(JSON.parse(spec) as string)(double(value)),
};
