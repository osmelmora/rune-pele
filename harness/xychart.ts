// Oracle ops for XY charts: Pele's db for a source, or Pele's error with its position, as
// crates/pele-core/tests/xychart_front.rs compares them, and Pele's tick functions, as
// crates/pele-core/tests/xychart_ticks.rs holds the port to them bit for bit. Each number travels
// as the hex bits of its double.
import { PeleError, parse } from '../vendor/pele/src/index.ts';
import type { XyChartDb } from '../vendor/pele/src/diagrams/xychart/db.ts';
import { tickIndex, tickStep, tickValue, ticksBetween } from '../vendor/pele/src/diagrams/xychart/ticks.ts';

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

// JSON has no Infinity, and -0 prints as 0.
const n = (value: number): unknown =>
  Object.is(value, -0) || !Number.isFinite(value) ? { $number: Object.is(value, -0) ? '-0' : String(value) } : value;

function xychartFront(source: string): string {
  let db: XyChartDb;
  try {
    const parsed = parse(source);
    if (parsed.type !== 'xychart') return JSON.stringify({ type: parsed.type });
    db = parsed as unknown as XyChartDb;
  } catch (error) {
    if (!(error instanceof PeleError)) return JSON.stringify({ error: String(error) });
    if (error.type !== null && error.type !== 'xychart') return JSON.stringify({ type: error.type });
    const { code, type, mermaidType, line, column, snippet } = error;
    return JSON.stringify({ error: String(error), code, type, mermaidType, line, column, snippet });
  }
  const x = db.xAxis;
  return JSON.stringify({
    model: {
      title: db.title ?? null,
      accTitle: db.accTitle ?? null,
      accDescr: db.accDescr ?? null,
      orientation: db.orientation ?? null,
      xAxis:
        x.type === 'band'
          ? { type: 'band', title: x.title, categories: x.categories }
          : { type: 'linear', title: x.title, min: n(x.min), max: n(x.max) },
      yAxis: { type: 'linear', title: db.yAxis.title, min: n(db.yAxis.min), max: n(db.yAxis.max) },
      hasSetXAxis: db.hasSetXAxis,
      hasSetYAxis: db.hasSetYAxis,
      plots: db.plots.map((plot) => ({
        type: plot.type,
        title: plot.title,
        data: plot.data.map(([place, value]) => [place, value === undefined ? null : n(value)]),
        pointLabels: plot.pointLabels ?? null,
      })),
    },
  });
}

// What Pele's `valueMarks` makes of a range before it labels the ticks.
function valueAxis(lo: number, hi: number, length: number, spacing: number, exact: boolean): string {
  const step = tickStep(Math.abs(hi - lo), Math.max(2, Math.min(10, Math.floor(length / spacing))));
  if (!exact) {
    lo = tickValue(step, Math.floor(tickIndex(step, lo) + 1e-9));
    hi = tickValue(step, Math.ceil(tickIndex(step, hi) - 1e-9));
  }
  return [bits(lo), bits(hi), ...ticksBetween(Math.min(lo, hi), Math.max(lo, hi), step).map(bits)].join(',');
}

export const xychartOps: Record<string, (args: string[]) => string> = {
  xychart_front: ([source]) => xychartFront(JSON.parse(source) as string),
  xychart_log10: ([x]) => bits(Math.log10(double(x))),
  xychart_pow10: ([x]) => bits(10 ** double(x)),
  xychart_step: ([span, count]) => bits(tickStep(double(span), double(count))),
  xychart_between: ([lo, hi, step]) => ticksBetween(double(lo), double(hi), double(step)).map(bits).join(','),
  xychart_axis: ([lo, hi, length, spacing, exact]) =>
    valueAxis(double(lo), double(hi), double(length), double(spacing), exact === 'true'),
};
