// Oracle ops for gantt charts: Pele's date helpers one call at a time, as
// crates/pele-core/tests/gantt_dates.rs compares them, and Pele's parsed and scheduled model, as
// crates/pele-core/tests/gantt_front.rs compares it.
//
// `gantt_date` takes one JSON object whose `fn` names the call and answers one JSON value. Doubles
// travel as 16 hex digits of their bits, and a NaN answers as "NaN".
import { autoTicks, intervalTicks, timeFormat } from '../vendor/pele/src/diagrams/gantt/axis.ts';
import { add, format, isoDate, parseStrict, type Unit } from '../vendor/pele/src/diagrams/gantt/dates.ts';
import { parseDuration } from '../vendor/pele/src/diagrams/gantt/db.ts';

type Json = Record<string, unknown>;

function double(hex: string): number {
  if (hex === 'NaN') return NaN;
  const view = new DataView(new ArrayBuffer(8));
  view.setBigUint64(0, BigInt('0x' + hex));
  return view.getFloat64(0);
}

function bits(value: number): string {
  if (Number.isNaN(value)) return 'NaN';
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, value);
  return view.getBigUint64(0).toString(16).padStart(16, '0');
}

const time = (date: Date): string => bits(date.getTime());

// The setters Pele calls, with the arguments given and no more.
function setter(date: Date, method: string, args: number[]): void {
  switch (method) {
    case 'setTime':
      date.setTime(args[0]);
      break;
    case 'setDate':
      date.setDate(args[0]);
      break;
    case 'setMonth':
      if (args.length > 1) date.setMonth(args[0], args[1]);
      else date.setMonth(args[0]);
      break;
    case 'setFullYear':
      if (args.length > 2) date.setFullYear(args[0], args[1], args[2]);
      else if (args.length > 1) date.setFullYear(args[0], args[1]);
      else date.setFullYear(args[0]);
      break;
    case 'setHours':
      if (args.length > 3) date.setHours(args[0], args[1], args[2], args[3]);
      else if (args.length > 2) date.setHours(args[0], args[1], args[2]);
      else if (args.length > 1) date.setHours(args[0], args[1]);
      else date.setHours(args[0]);
      break;
    default:
      throw new Error(`oracle: ${method} is not a setter Pele uses`);
  }
}

function dateCall(call: Json): unknown {
  const n = (key: string): number => double(call[key] as string);
  const list = (key: string): number[] => (call[key] as string[]).map(double);
  switch (call.fn) {
    case 'new':
      return time(new Date(call.text as string));
    case 'local': {
      const [y, m, d, h, mi, s, ms] = list('args');
      return time(new Date(y, m, d, h, mi, s, ms));
    }
    case 'utc': {
      const [y, m, d, h, mi, s, ms] = list('args');
      return bits(Date.UTC(y, m, d, h, mi, s, ms));
    }
    case 'set': {
      const date = new Date(n('time'));
      setter(date, call.method as string, list('args'));
      return time(date);
    }
    case 'fields': {
      const d = new Date(n('time'));
      return [d.getFullYear(), d.getMonth(), d.getDate(), d.getDay(), d.getHours(), d.getMinutes(), d.getSeconds(), d.getMilliseconds(), d.getTimezoneOffset()].map(bits);
    }
    case 'parse': {
      const date = parseStrict(call.input as string, call.template as string, new Date(n('now')));
      return date === undefined ? null : time(date);
    }
    case 'format':
      return format(new Date(n('time')), call.template as string) ?? null;
    case 'add':
      return time(add(new Date(n('time')), n('amount'), call.unit as Unit));
    case 'iso':
      return isoDate(new Date(n('time')));
    case 'duration': {
      const [amount, unit] = parseDuration(call.text as string);
      return [bits(amount), unit];
    }
    case 'auto':
      return autoTicks(n('start'), n('stop'), n('count')).map(bits);
    case 'interval': {
      const ticks = intervalTicks(call.interval as string, call.weekday as string, n('start'), n('stop'), n('limit'));
      return ticks === undefined ? null : ticks.map(bits);
    }
    case 'timeFormat':
      return timeFormat(call.specifier as string, new Date(n('time')));
    default:
      throw new Error(`oracle: no gantt date call ${String(call.fn)}`);
  }
}

export const ganttOps: Record<string, (args: string[]) => string> = {
  gantt_date: ([call]) => JSON.stringify(dateCall(JSON.parse(call) as Json)),
};
