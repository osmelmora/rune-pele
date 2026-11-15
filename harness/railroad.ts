// Railroad and info ops: Pele's rule model or info model for a source, or Pele's error with its
// position, so that each front end can be diffed against it before anything is drawn.
import { PeleError, parse } from '../vendor/pele/src/index.ts';

function failure(error: unknown, own: string): string {
  if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
  if (error.type !== null && error.type !== own) return JSON.stringify({ type: error.type });
  const { code, type, mermaidType, line, column, snippet } = error;
  return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
}

// The model in its own field order, with each number of a repetition as `String(number)`, since
// JSON has no Infinity. Nesting can go deeper than JSON.stringify's stack, so it is written by hand.
function nodeJson(root: unknown): string {
  const out: string[] = [];
  const work: unknown[] = [root];
  while (work.length > 0) {
    const next = work.pop();
    if (typeof next === 'string') {
      out.push(next);
      continue;
    }
    const node = next as Record<string, unknown>;
    switch (node.type) {
      case 'terminal':
        out.push(`{"type":"terminal","value":${JSON.stringify(node.value)}}`);
        break;
      case 'nonterminal':
        out.push(`{"type":"nonterminal","name":${JSON.stringify(node.name)}}`);
        break;
      case 'special':
        out.push(`{"type":"special","text":${JSON.stringify(node.text)}}`);
        break;
      case 'sequence':
      case 'choice': {
        const key = node.type === 'sequence' ? 'elements' : 'alternatives';
        const parts = node[key] as unknown[];
        out.push(`{"type":"${node.type}","${key}":[`);
        work.push(']}');
        for (let i = parts.length - 1; i >= 0; i--) {
          work.push(parts[i]);
          if (i > 0) work.push(',');
        }
        break;
      }
      case 'optional':
        out.push('{"type":"optional","element":');
        work.push('}', node.element);
        break;
      case 'repetition':
        out.push('{"type":"repetition","element":');
        work.push(`,"min":"${String(node.min)}","max":"${String(node.max)}"}`, node.element);
        break;
      default:
        throw new Error(`oracle: a railroad node of type ${String(node.type)}`);
    }
    const keys = Object.keys(node).join(',');
    const expected = { terminal: 'type,value', nonterminal: 'type,name', special: 'type,text', sequence: 'type,elements', choice: 'type,alternatives', optional: 'type,element', repetition: 'type,element,min,max' }[node.type as string];
    if (keys !== expected) throw new Error(`oracle: a ${String(node.type)} node with the fields ${keys}`);
  }
  return out.join('');
}

function railroadFront(text: string): string {
  let model;
  try {
    model = parse(text);
  } catch (error) {
    return failure(error, 'railroad');
  }
  if (model.type !== 'railroad') return JSON.stringify({ type: model.type });
  const rules = model.rules.map((rule) => `{"name":${JSON.stringify(rule.name)},"definition":${nodeJson(rule.definition)}}`);
  const head = JSON.stringify({
    type: model.type,
    mermaidType: (model as { mermaidType?: string }).mermaidType,
    notation: model.notation,
    title: model.title ?? null,
    accTitle: model.accTitle ?? null,
    accDescr: model.accDescr ?? null,
  });
  return `${head.slice(0, -1)},"rules":[${rules.join(',')}]}`;
}

function infoFront(text: string): string {
  let model;
  try {
    model = parse(text);
  } catch (error) {
    return failure(error, 'info');
  }
  if (model.type !== 'info') return JSON.stringify({ type: model.type });
  return JSON.stringify({ type: model.type, title: model.title ?? null, accTitle: model.accTitle ?? null, accDescr: model.accDescr ?? null });
}

export const railroadOps: Record<string, (args: string[]) => string> = {
  railroad_front: ([source]) => railroadFront(JSON.parse(source) as string),
  info_front: ([source]) => infoFront(JSON.parse(source) as string),
};
