// Oracle ops for packet diagrams: Pele's model for a source, or Pele's error with its position,
// as crates/pele-core/tests/packet_front.rs compares them. Each number travels as JavaScript
// writes it, since a field can be wider than JSON's numbers reach.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { PacketModel } from '../vendor/pele/src/diagrams/packet/model.ts';

// The fields of Pele's `PacketBlock` interface, in its order.
const BLOCK = ['start', 'end', 'bits', 'label'];

function packetFront(text: string, options: { config?: Record<string, unknown> }): string {
  let model: PacketModel;
  try {
    const parsed = parse(text, options);
    if (parsed.type !== 'packet') return JSON.stringify({ type: parsed.type });
    model = parsed as unknown as PacketModel;
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'packet') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  const rows = model.rows.map((row) =>
    row.map((block) => {
      for (const key of Object.keys(block)) {
        if (!BLOCK.includes(key)) throw new Error(`oracle: a packet block has the unlisted field ${key}`);
      }
      return [String(block.start), String(block.end), String(block.bits), block.label];
    })
  );
  return JSON.stringify({
    model: {
      title: model.title ?? null,
      accTitle: model.accTitle ?? null,
      accDescr: model.accDescr ?? null,
      bitsPerRow: String(model.bitsPerRow),
      rows,
    },
  });
}

export const packetOps: Record<string, (args: string[]) => string> = {
  packet_front: ([source, options]) => packetFront(JSON.parse(source) as string, options ? JSON.parse(options) : {}),
};
