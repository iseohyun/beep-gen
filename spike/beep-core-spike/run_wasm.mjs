// 웹과 같은 엔진(V8)에서 WASM 빌드를 실행: node run_wasm.mjs <input.mp3> <output.mp3> [n] [m] [weak]
import { readFileSync, writeFileSync } from "node:fs";
import { performance } from "node:perf_hooks";

const [input, output, n = "1.4", m = "2", weak = "0.35"] = process.argv.slice(2);
const wasmPath = new URL(
  "./target/wasm32-unknown-unknown/release/beep_core_spike.wasm",
  import.meta.url,
);

const { instance } = await WebAssembly.instantiate(readFileSync(wasmPath), {
  env: { js_now: () => performance.now() },
});
const x = instance.exports;

const bytes = readFileSync(input);
const ptr = x.alloc(bytes.length);
new Uint8Array(x.memory.buffer, ptr, bytes.length).set(bytes);

const ok = x.run(ptr, bytes.length, Number(n), Number(m), Number(weak));
const report = Buffer.from(
  new Uint8Array(x.memory.buffer, x.report_ptr(), x.report_len()),
).toString("utf8");

if (!ok) {
  console.error("error:", report);
  process.exit(1);
}
writeFileSync(output, new Uint8Array(x.memory.buffer, x.output_ptr(), x.output_len()));
console.log(report);
