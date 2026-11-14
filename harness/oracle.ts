// Answers numeric questions the way Pele 1.13.0 does, so the Rust port can be diffed against it.
// Protocol: one request per stdin line, `<op>\t<arg>...`, one answer per stdout line.
// Doubles travel as 16 hex digits of their IEEE-754 bits so no decimal parse sits between the sides.
// `render` takes a JSON string and answers one JSON object.
import { createInterface } from 'node:readline';
import { detectType, render } from '../vendor/pele/src/index.ts';
import { num } from '../vendor/pele/src/svg/builder.ts';
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

const ops: Record<string, (args: string[]) => string> = {
  round: ([x]) => {
    const r = Math.round(double(x));
    return Number.isNaN(r) ? 'NaN' : bits(r);
  },
  num: ([x]) => num(double(x)),
  string: ([x]) => String(double(x)),
  fixed: ([x, digits]) => double(x).toFixed(Number(digits)),
  render: ([source]) => {
    const text = JSON.parse(source) as string;
    try {
      const { type, svg, width, height } = render(text, PARITY);
      return JSON.stringify({ type, svg, width, height });
    } catch (error) {
      return JSON.stringify({ type: detectType(text), error: String(error) });
    }
  },
};

const out: string[] = [];
for await (const line of createInterface({ input: process.stdin, crlfDelay: Infinity })) {
  const [op, ...args] = line.split('\t');
  const run = ops[op];
  if (!run) throw new Error(`unknown op ${op}`);
  out.push(run(args));
  if (out.length >= 4096) process.stdout.write(out.splice(0).join('\n') + '\n');
}
if (out.length > 0) process.stdout.write(out.join('\n') + '\n');
