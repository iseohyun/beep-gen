// 백그라운드 스레드: 코어(WASM)를 불러 음원을 만든다.
// 화면이 멈추지 않도록 무거운 계산은 전부 여기서 한다. 계산 자체는 코어가 한다.

import init, { build_track, default_file_name, describe } from "./wasm/beep_wasm.js";
// 번들러가 WASM 파일에 내용 해시가 붙은 주소를 만들어 준다(캐시 버스팅).
import wasmUrl from "./wasm/beep_wasm_bg.wasm?url";
import type { WorkerRequest, WorkerResponse } from "./worker-protocol";

const ready = init({ module_or_path: wasmUrl });

function reply(message: WorkerResponse, transfer: Transferable[] = []): void {
  (self as unknown as Worker).postMessage(message, transfer);
}

self.onmessage = async (event: MessageEvent<WorkerRequest>) => {
  const request = event.data;
  try {
    await ready;
    if (request.type === "describe") {
      reply({ id: request.id, type: "described", infoJson: describe() });
      return;
    }
    const bytes = build_track(request.paramsJson, request.format, (percent: number) => {
      reply({ id: request.id, type: "progress", percent });
    }) as Uint8Array<ArrayBuffer>;
    const fileName = default_file_name(request.paramsJson, request.format);
    reply({ id: request.id, type: "generated", bytes, fileName }, [bytes.buffer]);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    reply({ id: request.id, type: "error", message });
  }
};
