// 백그라운드 스레드: 코어(WASM)를 불러 음원을 만든다.
// 화면이 멈추지 않도록 무거운 계산은 전부 여기서 한다. 계산 자체는 코어가 한다.

import init, { build_track, describe, identify_song, intro_text } from "./wasm/beep_wasm.js";
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
    if (request.type === "introText") {
      const text = intro_text(request.template, request.paramsJson);
      reply({ id: request.id, type: "introText", text });
      return;
    }
    if (request.type === "identify") {
      const bytes = request.music ? new Uint8Array(request.music) : undefined;
      const guessJson = identify_song(bytes, request.sourceName ?? undefined);
      reply({ id: request.id, type: "identified", guessJson });
      return;
    }
    const music = request.music ? new Uint8Array(request.music) : undefined;
    const intro = request.intro ? new Uint8Array(request.intro) : undefined;
    const result = build_track(
      request.paramsJson,
      request.format,
      music,
      request.musicNameJson ?? undefined,
      intro,
      (percent: number, _stageKey: string, stageLabel: string) => {
        reply({ id: request.id, type: "progress", percent, stage: stageLabel });
      },
    );
    try {
      const bytes = result.takeBytes() as Uint8Array<ArrayBuffer>;
      reply(
        {
          id: request.id,
          type: "generated",
          bytes,
          fileName: result.fileName,
          musicReportJson: result.musicReportJson ?? null,
        },
        [bytes.buffer],
      );
    } finally {
      result.free();
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    reply({ id: request.id, type: "error", message });
  }
};
