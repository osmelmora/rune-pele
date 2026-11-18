// Answers one Math function, as the Node running this script computes it, for every input on
// stdin. Inputs and answers are little-endian doubles, one or two to an input as the function
// takes them. `crates/pele-core/tests/intel_trig.rs` runs it under each Node it compares.
// usage: node scripts/intel-trig.mjs <function>
import { once } from 'node:events';

const functions = {
  sin: [1, (x) => Math.sin(x)],
  cos: [1, (x) => Math.cos(x)],
  acos: [1, (x) => Math.acos(x)],
  asin: [1, (x) => Math.asin(x)],
  log10: [1, (x) => Math.log10(x)],
  log2: [1, (x) => Math.log2(x)],
  sqrt: [1, (x) => Math.sqrt(x)],
  atan2: [2, (y, x) => Math.atan2(y, x)],
  pow: [2, (x, y) => Math.pow(x, y)],
  power: [2, (x, y) => x ** y],
  hypot: [2, (x, y) => Math.hypot(x, y)],
};

const name = process.argv[2];
if (!Object.hasOwn(functions, name)) {
  process.stderr.write(`intel-trig.mjs: no function ${name}; one of ${Object.keys(functions).join(', ')}\n`);
  process.exit(64);
}
const [arity, f] = functions[name];
const width = 8 * arity;

let rest = Buffer.alloc(0);
for await (const chunk of process.stdin) {
  const data = rest.length ? Buffer.concat([rest, chunk]) : chunk;
  const whole = data.length - (data.length % width);
  const input = new Float64Array(data.buffer.slice(data.byteOffset, data.byteOffset + whole));
  const out = new Float64Array(whole / width);
  if (arity === 1) {
    for (let i = 0; i < out.length; i++) out[i] = f(input[i]);
  } else {
    for (let i = 0; i < out.length; i++) out[i] = f(input[2 * i], input[2 * i + 1]);
  }
  if (!process.stdout.write(Buffer.from(out.buffer))) await once(process.stdout, 'drain');
  rest = data.subarray(whole);
}
if (rest.length) {
  process.stderr.write(`intel-trig.mjs: ${rest.length} bytes left over, not a whole input\n`);
  process.exit(65);
}
