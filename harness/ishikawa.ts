// Oracle ops for Ishikawa diagrams: Pele's model for a source, or Pele's error with its position,
// as crates/pele-core/tests/ishikawa_front.rs compares them.
import type { IshikawaModel } from '../vendor/pele/src/diagrams/ishikawa/types.ts';
import { front } from './kanban.ts';

// The tree in preorder, each node with its depth, which is the tree and nothing else.
function ishikawa(parsed: unknown): unknown {
  const model = parsed as IshikawaModel;
  const nodes: { text: string; depth: number }[] = [];
  const pending = model.root ? [{ node: model.root, depth: 0 }] : [];
  while (pending.length > 0) {
    const { node, depth } = pending.pop()!;
    nodes.push({ text: node.text, depth });
    for (let i = node.children.length - 1; i >= 0; i--) pending.push({ node: node.children[i], depth: depth + 1 });
  }
  return { title: model.title ?? null, nodes };
}

export const ishikawaOps: Record<string, (args: string[]) => string> = {
  ishikawa_front: ([source]) => front(JSON.parse(source) as string, 'ishikawa', ishikawa),
};
