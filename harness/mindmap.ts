// Oracle ops for mindmaps: Pele's model for a source, or Pele's error with its position, as
// crates/pele-core/tests/mindmap_front.rs compares them. A node's children are written as their
// ids, so a deep map stays shallow JSON.
import type { MindmapModel } from '../vendor/pele/src/diagrams/mindmap/db.ts';
import { front } from './kanban.ts';

function mindmap(parsed: unknown): unknown {
  const model = parsed as MindmapModel;
  return {
    title: model.title ?? null,
    nodes: model.nodes.map((node) => ({
      id: node.id,
      nodeId: node.nodeId,
      level: node.level,
      descr: node.descr,
      type: node.type,
      children: node.children.map((child) => child.id),
      isRoot: node.isRoot,
      section: node.section ?? null,
      class: node.class ?? null,
      icon: node.icon ?? null,
    })),
  };
}

export const mindmapOps: Record<string, (args: string[]) => string> = {
  mindmap_front: ([source]) => front(JSON.parse(source) as string, 'mindmap', mindmap),
};
