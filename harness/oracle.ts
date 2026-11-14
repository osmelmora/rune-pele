// Answers numeric questions the way Pele 1.13.0 does, so the Rust port can be diffed against it.
// Protocol: one request per stdin line, `<op>\t<arg>...`, one answer per stdout line.
// Doubles travel as 16 hex digits of their IEEE-754 bits so no decimal parse sits between the sides.
// `render` takes a JSON string and answers one JSON object. `render_with` also takes a JSON object
// of Pele render options, laid over the pinned ones. `detect`, `preprocess`, `model` and `label`
// expose the stages before drawing, so a port can be diffed one stage at a time.
import { createInterface } from 'node:readline';
import { detectDiagram } from '../vendor/pele/src/detect.ts';
import { detectType, parse, render } from '../vendor/pele/src/index.ts';
import { encodeEntities, preprocess } from '../vendor/pele/src/preprocess.ts';
import { num } from '../vendor/pele/src/svg/builder.ts';
import { layoutLabel } from '../vendor/pele/src/text/label.ts';
import { metricsMeasurer } from '../vendor/pele/src/text/measurer.ts';

// The same pinned inputs Pele's own render corpus test uses, so output depends only on the source.
const PARITY = { measurer: metricsMeasurer, now: Date.UTC(2026, 0, 15, 12) };

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

function rendered(text: string, options: object): string {
  try {
    const { type, svg, width, height } = render(text, { ...PARITY, ...options });
    return JSON.stringify({ type, svg, width, height });
  } catch (error) {
    let type: string | null = null;
    try {
      type = detectType(text);
    } catch {
      // Front matter that is not YAML fails detection as well.
    }
    return JSON.stringify({ type, error: String(error) });
  }
}

// JSON has no NaN or Infinity, and a config may hold either.
function plain(value: unknown): unknown {
  if (typeof value === 'number' && !Number.isFinite(value)) return { $number: String(value) };
  if (Array.isArray(value)) return value.map(plain);
  if (typeof value === 'object' && value !== null) {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, plain(item)]));
  }
  return value;
}

const ops: Record<string, (args: string[]) => string> = {
  round: ([x]) => {
    const r = Math.round(double(x));
    return Number.isNaN(r) ? 'NaN' : bits(r);
  },
  num: ([x]) => num(double(x)),
  string: ([x]) => String(double(x)),
  fixed: ([x, digits]) => double(x).toFixed(Number(digits)),
  sin: ([x]) => bits(Math.sin(double(x))),
  cos: ([x]) => bits(Math.cos(double(x))),
  pow: ([x, y]) => bits(Math.pow(double(x), double(y))),
  render: ([source]) => rendered(JSON.parse(source) as string, {}),
  render_with: ([source, options]) => rendered(JSON.parse(source) as string, JSON.parse(options) as object),
  detect: ([source]) => JSON.stringify(detectDiagram(JSON.parse(source) as string)),
  preprocess: ([source]) => {
    try {
      const { text, title, config } = preprocess(JSON.parse(source) as string);
      return JSON.stringify({ text, title: title ?? null, config: plain(config), parsed: encodeEntities(text) });
    } catch (error) {
      return JSON.stringify({ error: String(error) });
    }
  },
  model: ([source]) => {
    try {
      const model = parse(JSON.parse(source) as string);
      if (model.type !== 'pie') return JSON.stringify({ type: model.type });
      const { type, title, accTitle, accDescr, showData, sections } = model;
      return JSON.stringify({
        type,
        title: title ?? null,
        accTitle: accTitle ?? null,
        accDescr: accDescr ?? null,
        showData,
        sections: [...sections].map(([label, value]) => [label, bits(value)]),
      });
    } catch (error) {
      return JSON.stringify({ error: String(error) });
    }
  },
  label: ([raw, size, maxWidth, base]) => {
    const label = layoutLabel(JSON.parse(raw) as string, false, metricsMeasurer, double(size), double(maxWidth), Number(base));
    return JSON.stringify({
      lines: label.lines.map((line) => line.map((span) => ({ text: span.text, style: span.style, width: bits(span.width) }))),
      widths: label.widths.map(bits),
      width: bits(label.width),
      height: bits(label.height),
      lineHeight: bits(label.lineHeight),
    });
  },
};

// flowchart front end
import { PeleError } from '../vendor/pele/src/index.ts';
import type { FlowDb } from '../vendor/pele/src/diagrams/flowchart/db.ts';
import { buildFlowGraph } from '../vendor/pele/src/diagrams/flowchart/graph.ts';
import { canonicalShape, isValidShape } from '../vendor/pele/src/diagrams/flowchart/shapes.ts';
import { safeUrl } from '../vendor/pele/src/util/url.ts';

// Each record's fields in the order its TypeScript interface declares them. A field that is
// undefined is left out, and a field the list does not name fails the request.
const FLOW_FIELDS = {
  node: ['id', 'text', 'labelType', 'type', 'styles', 'classes', 'dir', 'props', 'link', 'linkTarget', 'icon', 'form', 'pos', 'img', 'constraint', 'assetWidth', 'assetHeight'],
  edge: ['start', 'end', 'type', 'text', 'labelType', 'stroke', 'length', 'style', 'classes', 'id', 'isUserDefinedId', 'interpolate', 'animate', 'animation'],
  classDef: ['id', 'styles', 'textStyles'],
  subgraph: ['id', 'nodes', 'title', 'classes', 'dir', 'labelType', 'metadata'],
  graphNode: ['id', 'label', 'labelType', 'shape', 'isGroup', 'parentId', 'cssStyles', 'cssCompiledStyles', 'cssClasses', 'dir', 'link', 'linkTarget', 'tooltip', 'icon', 'pos', 'img', 'assetWidth', 'assetHeight', 'constraint', 'colorIndex', 'metadata'],
  graphEdge: ['id', 'isUserDefinedId', 'start', 'end', 'type', 'label', 'labelType', 'thickness', 'pattern', 'minlen', 'arrowTypeStart', 'arrowTypeEnd', 'cssCompiledStyles', 'style', 'classes', 'animate', 'animation', 'curve'],
};

function flowRecord(record: object, kind: keyof typeof FLOW_FIELDS): Record<string, unknown> {
  const names = FLOW_FIELDS[kind];
  for (const key of Object.keys(record)) {
    if (!names.includes(key)) throw new Error(`oracle: ${kind} has the unlisted field ${key}`);
  }
  const out: Record<string, unknown> = {};
  for (const name of names) {
    const value = (record as Record<string, unknown>)[name];
    if (value !== undefined) out[name] = value;
  }
  return out;
}

// Pele's flowchart model and flow graph for a source and parse options, or Pele's error with its
// position. A source of another type answers only that type.
function flowFront(text: string, options: { config?: Record<string, unknown> }): string {
  let db: FlowDb & { mermaidType: string };
  try {
    const model = parse(text, options);
    if (model.type !== 'flowchart') return JSON.stringify({ type: model.type });
    db = model as unknown as FlowDb & { mermaidType: string };
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'flowchart') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  const pre = preprocess(text).config;
  const config = options.config ? { ...options.config, ...pre } : pre;
  const flow = (config.flowchart ?? {}) as Record<string, unknown>;
  const graph = buildFlowGraph(db, typeof flow.curve === 'string' ? flow.curve : undefined);
  const model = {
    type: db.type,
    mermaidType: db.mermaidType,
    direction: db.direction,
    nodes: [...db.nodes.values()].map((node) => flowRecord(node, 'node')),
    edges: db.edges.map((edge) => flowRecord(edge, 'edge')),
    subgraphs: db.subgraphs.map((subgraph) => flowRecord(subgraph, 'subgraph')),
    classes: [...db.classes.values()].map((classDef) => flowRecord(classDef, 'classDef')),
    tooltips: [...db.tooltips],
    defaultEdgeStyle: db.defaultEdgeStyle,
    defaultInterpolate: db.defaultInterpolate,
    title: db.title,
    accTitle: db.accTitle,
    accDescr: db.accDescr,
  };
  return JSON.stringify({
    model,
    graph: {
      nodes: graph.nodes.map((node) => flowRecord(node, 'graphNode')),
      edges: graph.edges.map((edge) => flowRecord(edge, 'graphEdge')),
    },
  });
}

ops.flow_front = ([source, options]) => flowFront(JSON.parse(source) as string, options ? JSON.parse(options) : {});
ops.canonical_shape = ([name]) => {
  const shape = JSON.parse(name) as string;
  return JSON.stringify([canonicalShape(shape), isValidShape(shape)]);
};
ops.safe_url = ([url]) => JSON.stringify(safeUrl(JSON.parse(url) as string));

const out: string[] = [];
for await (const line of createInterface({ input: process.stdin, crlfDelay: Infinity })) {
  const [op, ...args] = line.split('\t');
  const run = ops[op];
  if (!run) throw new Error(`unknown op ${op}`);
  out.push(run(args));
  if (out.length >= 4096) process.stdout.write(out.splice(0).join('\n') + '\n');
}
if (out.length > 0) process.stdout.write(out.join('\n') + '\n');
