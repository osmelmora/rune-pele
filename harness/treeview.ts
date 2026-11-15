// Tree view ops: Pele's model for a source, or Pele's error with its position, so that the front
// end, box drawing included, can be diffed against it before anything is drawn.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { TreeNode, TreeViewModel } from '../vendor/pele/src/diagrams/treeview/model.ts';

function failure(error: unknown): string {
  if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
  if (error.type !== null && error.type !== 'treeView') return JSON.stringify({ type: error.type });
  const { code, type, mermaidType, line, column, snippet } = error;
  return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
}

// A node in its own field order, with each field it leaves out as null. Written with a stack, as
// a tree can nest deeper than JSON.stringify's.
function nodeJson(root: TreeNode): string {
  const out: string[] = [];
  const work: (TreeNode | string)[] = [root];
  while (work.length > 0) {
    const next = work.pop()!;
    if (typeof next === 'string') {
      out.push(next);
      continue;
    }
    const head = JSON.stringify({
      id: next.id,
      level: next.level,
      name: next.name,
      nodeType: next.nodeType,
      icon: next.icon ?? null,
      cssClass: next.cssClass ?? null,
      description: next.description ?? null,
    });
    out.push(`${head.slice(0, -1)},"children":[`);
    work.push(']}');
    for (let i = next.children.length - 1; i >= 0; i--) {
      work.push(next.children[i]);
      if (i > 0) work.push(',');
    }
  }
  return out.join('');
}

function treeviewFront(text: string): string {
  let parsed;
  try {
    parsed = parse(text);
  } catch (error) {
    return failure(error);
  }
  if (parsed.type !== 'treeView') return JSON.stringify({ type: parsed.type });
  const model = parsed as TreeViewModel;
  const head = JSON.stringify({ type: model.type, title: model.title ?? null, accTitle: model.accTitle ?? null, accDescr: model.accDescr ?? null });
  return `${head.slice(0, -1)},"root":${nodeJson(model.root)},"count":${model.count}}`;
}

export const treeviewOps: Record<string, (args: string[]) => string> = {
  treeview_front: ([source]) => treeviewFront(JSON.parse(source) as string),
};
