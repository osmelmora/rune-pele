// Agentflow ops for the oracle. `agentflow_front` answers Pele's model, graph and semantic model
// for a source and parse options, or Pele's error with its position.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { AgentflowDb } from '../vendor/pele/src/diagrams/agentflow/db.ts';
import { agentGraph } from '../vendor/pele/src/diagrams/agentflow/render.ts';

// Each record's fields in the order its TypeScript interface declares them. A field that is
// undefined is left out, and a field the list does not name fails the request.
const FIELDS = {
  node: ['id', 'text', 'labelType', 'type', 'styles', 'classes', 'dir', 'props', 'link', 'linkTarget', 'metadata', 'isConnector'],
  edge: ['start', 'end', 'type', 'text', 'labelType', 'stroke', 'edgeSemantic', 'length', 'style', 'classes', 'id', 'isUserDefinedId', 'interpolate', 'animate', 'animation', 'metadata'],
  classDef: ['id', 'styles', 'textStyles'],
  subgraph: ['id', 'nodes', 'title', 'classes', 'dir', 'labelType', 'type', 'metadata'],
  mapping: ['id', 'type', 'position'],
  position: ['startLine', 'startColumn', 'endLine', 'endColumn', 'startIndex', 'endIndex'],
  diagnostic: ['id', 'severity', 'message', 'nodeId', 'edgeId', 'position'],
  graphNode: ['id', 'label', 'labelType', 'shape', 'isGroup', 'parentId', 'cssStyles', 'cssCompiledStyles', 'cssClasses', 'dir', 'link', 'linkTarget', 'tooltip', 'colorIndex', 'kind', 'metadata'],
  graphEdge: ['id', 'isUserDefinedId', 'start', 'end', 'type', 'label', 'labelType', 'thickness', 'pattern', 'minlen', 'arrowTypeStart', 'arrowTypeEnd', 'cssCompiledStyles', 'style', 'classes', 'animate', 'animation', 'curve', 'metadata'],
  semanticVertex: ['id', 'label', 'shape', 'vertexKind', 'metadata'],
  semanticEdge: ['start', 'end', 'id', 'label', 'type', 'stroke', 'edgeSemantic', 'length', 'metadata'],
  semanticSubgraph: ['id', 'type', 'title', 'nodes', 'metadata', 'direction'],
  semanticConnector: ['id', 'title', 'metadata'],
};

type Json = Record<string, unknown>;

// JSON has no NaN or Infinity, and metadata may hold either.
function plain(value: unknown): unknown {
  if (typeof value === 'number' && !Number.isFinite(value)) return { $number: String(value) };
  if (Array.isArray(value)) return value.map(plain);
  if (typeof value === 'object' && value !== null) {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, plain(item)]));
  }
  return value;
}

const YAML = new Set(['metadata', 'animate', 'animation', 'interpolate', 'curve']);

function record(item: object, kind: keyof typeof FIELDS): Json {
  const names = FIELDS[kind];
  for (const key of Object.keys(item)) {
    if (!names.includes(key)) throw new Error(`oracle: ${kind} has the unlisted field ${key}`);
  }
  const out: Json = {};
  for (const name of names) {
    const value = (item as Json)[name];
    if (value === undefined) continue;
    out[name] = name === 'position' ? record(value as object, 'position') : YAML.has(name) ? plain(value) : value;
  }
  return out;
}

function agentflowFront(text: string, options: { config?: Json; maxEdges?: number }): string {
  let db: AgentflowDb & { mermaidType: string };
  try {
    const model = parse(text, options);
    if (model.type !== 'agentflow') return JSON.stringify({ type: model.type });
    db = model as unknown as AgentflowDb & { mermaidType: string };
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'agentflow') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  // The graph was built when the source was parsed, and is kept per model.
  const graph = agentGraph(db, {});
  const semantic = db.semanticModel();
  return JSON.stringify({
    model: {
      type: db.type,
      mermaidType: db.mermaidType,
      direction: db.direction,
      nodes: [...db.nodes.values()].map((node) => record(node, 'node')),
      edges: db.edges.map((edge) => record(edge, 'edge')),
      subgraphs: db.subgraphs.map((subgraph) => record(subgraph, 'subgraph')),
      connectors: [...db.connectors.keys()],
      classes: [...db.classes.values()].map((classDef) => record(classDef, 'classDef')),
      tooltips: [...db.tooltips],
      defaultEdgeStyle: db.defaultEdgeStyle,
      defaultInterpolate: db.defaultInterpolate,
      mappings: db.mappings.map((mapping) => record(mapping, 'mapping')),
      diagnostics: db.diagnostics.map((diagnostic) => record(diagnostic, 'diagnostic')),
      title: db.title,
      accTitle: db.accTitle,
      accDescr: db.accDescr,
    },
    graph: {
      nodes: graph.nodes.map((node) => record(node, 'graphNode')),
      edges: graph.edges.map((edge) => record(edge, 'graphEdge')),
    },
    semantic: {
      direction: semantic.direction,
      vertices: semantic.vertices.map((vertex) => record(vertex, 'semanticVertex')),
      edges: semantic.edges.map((edge) => record(edge, 'semanticEdge')),
      subGraphs: semantic.subGraphs.map((subgraph) => record(subgraph, 'semanticSubgraph')),
      connectors: semantic.connectors.map((connector) => record(connector, 'semanticConnector')),
    },
  });
}

export const agentflowOps: Record<string, (args: string[]) => string> = {
  agentflow_front: ([source, options]) => agentflowFront(JSON.parse(source) as string, options ? JSON.parse(options) : {}),
};
