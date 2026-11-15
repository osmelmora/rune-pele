// Oracle ops for git graphs: Pele's model for a source, or Pele's error with its position, as
// crates/pele-core/tests/git_front.rs compares them.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { GitCommit, GitModel } from '../vendor/pele/src/diagrams/git/db.ts';

// The fields of Pele's `GitCommit` interface, in its order.
const COMMIT = ['id', 'message', 'seq', 'type', 'tags', 'parents', 'branch', 'customType', 'customId'];

function commit(record: GitCommit): Record<string, unknown> {
  for (const key of Object.keys(record)) {
    if (!COMMIT.includes(key)) throw new Error(`oracle: a commit has the unlisted field ${key}`);
  }
  const { id, message, seq, type, tags, parents, branch } = record;
  return { id, message, seq, type, tags, parents, branch, customType: record.customType ?? null, customId: record.customId ?? null };
}

function gitFront(text: string, options: { config?: Record<string, unknown> }): string {
  let model: GitModel;
  try {
    const parsed = parse(text, options);
    if (parsed.type !== 'gitGraph') return JSON.stringify({ type: parsed.type });
    model = parsed as unknown as GitModel;
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'gitGraph') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  return JSON.stringify({
    model: {
      title: model.title ?? null,
      accTitle: model.accTitle ?? null,
      accDescr: model.accDescr ?? null,
      direction: model.direction,
      commits: [...model.commits.values()].map(commit),
      branches: [...model.branches],
      lanes: model.lanes,
      currentBranch: model.currentBranch,
      head: model.head?.id ?? null,
      warnings: model.warnings,
    },
  });
}

export const gitOps: Record<string, (args: string[]) => string> = {
  git_front: ([source, options]) => gitFront(JSON.parse(source) as string, options ? JSON.parse(options) : {}),
};
