// 웹(브라우저)용 플랫폼 구현.
//
// 여기는 어댑터다: 워커와 메시지를 주고받고 브라우저 다운로드를 띄우는 일만 한다.
// 오디오 샘플을 건드리는 코드는 두지 않는다(코어에 둔다).

import type {
  AppInfo,
  Capabilities,
  GenerateRequest,
  GeneratedTrack,
  MusicReport,
  Platform,
  ProgressInfo,
  TtsSetup,
  Voice,
} from "./platform";
import { TTS_MODEL, totalModelBytes } from "./tts-model";
import * as ttsStore from "./tts-store";
import type { TtsRequest, TtsResponse } from "./tts-worker";
import type { WorkerRequest, WorkerResponse } from "./worker-protocol";

type Pending = {
  resolve: (response: WorkerResponse) => void;
  reject: (error: Error) => void;
  onProgress?: (progress: ProgressInfo) => void;
};

/** `WorkerRequest` 에서 id 를 뺀 형태(유니언의 각 갈래를 유지한다). */
type RequestBody = WorkerRequest extends infer R ? (R extends unknown ? Omit<R, "id"> : never) : never;

/** 음성 합성 모델을 돌리기에 무리가 없는 최소 메모리(GB). */
const TTS_MIN_DEVICE_MEMORY_GB = 4;
const TTS_STAGE = "안내 음성을 만드는 중";

/**
 * 이 기기에서 브라우저 안 음성 합성을 권할 수 있는지.
 * 모델(약 263MB)을 메모리에 올려야 해서, 휴대폰과 메모리가 적은 기기는 제외한다.
 */
function canRunTts(): boolean {
  if (!("caches" in window) || typeof Worker === "undefined") return false;
  if (/Android|iPhone|iPad|iPod|Mobile/i.test(navigator.userAgent)) return false;
  const memory = (navigator as Navigator & { deviceMemory?: number }).deviceMemory;
  return memory === undefined || memory >= TTS_MIN_DEVICE_MEMORY_GB;
}

export class WebPlatform implements Platform {
  readonly capabilities: Capabilities = {
    musicFile: true,
    // 다른 사이트의 파일은 브라우저 보안 정책(CORS) 때문에 대부분 가져올 수 없다.
    musicUrl: false,
    // 브라우저는 내장 음성을 파일로 받을 수 없어, 음성 모델을 내려받아 직접 합성한다.
    ttsIntro: canRunTts(),
    offlineCache: true,
  };

  private readonly worker: Worker;
  private readonly pending = new Map<number, Pending>();
  private nextId = 1;
  private info?: AppInfo;

  private ttsWorker?: Worker;
  private readonly ttsPending = new Map<
    number,
    { resolve: (wav: ArrayBuffer) => void; reject: (error: Error) => void }
  >();

  constructor() {
    this.worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
    this.worker.onmessage = (event: MessageEvent<WorkerResponse>) => this.handle(event.data);
    this.worker.onerror = (event) => {
      const error = new Error(event.message || "작업 스레드에서 오류가 났습니다.");
      for (const entry of this.pending.values()) entry.reject(error);
      this.pending.clear();
    };
  }

  private handle(response: WorkerResponse): void {
    const entry = this.pending.get(response.id);
    if (!entry) return;
    if (response.type === "progress") {
      entry.onProgress?.({ percent: response.percent, stage: response.stage });
      return;
    }
    this.pending.delete(response.id);
    if (response.type === "error") entry.reject(new Error(response.message));
    else entry.resolve(response);
  }

  private request(
    body: RequestBody,
    transfer: Transferable[] = [],
    onProgress?: (progress: ProgressInfo) => void,
  ): Promise<WorkerResponse> {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject, onProgress });
      this.worker.postMessage({ ...body, id } as WorkerRequest, transfer);
    });
  }

  async describe(): Promise<AppInfo> {
    if (!this.info) {
      const response = await this.request({ type: "describe" });
      if (response.type !== "described") throw new Error("예상하지 못한 응답입니다.");
      this.info = JSON.parse(response.infoJson) as AppInfo;
    }
    return this.info;
  }

  // ── 안내 음성(TTS) ──────────────────────────────────────

  async listVoices(): Promise<Voice[]> {
    return this.capabilities.ttsIntro ? TTS_MODEL.voices.map((voice) => ({ ...voice })) : [];
  }

  ttsSetup(): TtsSetup | null {
    return {
      modelName: TTS_MODEL.name,
      sizeMB: Math.round(totalModelBytes() / 1_000_000),
      source: TTS_MODEL.source,
      licenseName: TTS_MODEL.licenseName,
      licenseUrl: TTS_MODEL.licenseUrl,
      terms: TTS_MODEL.terms,
    };
  }

  async ttsInstalled(): Promise<boolean> {
    return this.capabilities.ttsIntro && (await ttsStore.isInstalled());
  }

  installTts(onProgress: (percent: number) => void): Promise<void> {
    return ttsStore.install(onProgress);
  }

  async removeTts(): Promise<void> {
    // 모델을 메모리에 올려 둔 워커도 함께 내린다.
    this.ttsWorker?.terminate();
    this.ttsWorker = undefined;
    for (const entry of this.ttsPending.values()) entry.reject(new Error("음성 모델을 지웠습니다."));
    this.ttsPending.clear();
    await ttsStore.remove();
  }

  /** 문구를 읽은 음성(WAV)을 만든다. 음성 합성 워커는 처음 쓸 때 만든다. */
  private synthesize(text: string, voiceId: string): Promise<ArrayBuffer> {
    if (!this.ttsWorker) {
      const worker = new Worker(new URL("./tts-worker.ts", import.meta.url), { type: "module" });
      worker.onmessage = (event: MessageEvent<TtsResponse>) => {
        const entry = this.ttsPending.get(event.data.id);
        if (!entry) return;
        this.ttsPending.delete(event.data.id);
        if (event.data.type === "done") entry.resolve(event.data.wav);
        else entry.reject(new Error(event.data.message));
      };
      worker.onerror = (event) => {
        const error = new Error(event.message || "음성 합성을 시작하지 못했습니다.");
        for (const entry of this.ttsPending.values()) entry.reject(error);
        this.ttsPending.clear();
        // 다음 요청에서 새로 만들도록 버린다.
        worker.terminate();
        if (this.ttsWorker === worker) this.ttsWorker = undefined;
      };
      this.ttsWorker = worker;
    }
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.ttsPending.set(id, { resolve, reject });
      this.ttsWorker?.postMessage({ id, text, voiceId } satisfies TtsRequest);
    });
  }

  /** 요청에 안내 음성이 있으면 만들어 돌려준다. 만들지 못하면 이유를 `notices` 에 남긴다. */
  private async makeIntro(
    request: GenerateRequest,
    paramsJson: string,
    notices: string[],
    onProgress: (progress: ProgressInfo) => void,
  ): Promise<ArrayBuffer | null> {
    if (!request.intro || !(await this.ttsInstalled())) return null;
    // 자리표시자({n} 등)를 채운 최종 문구는 코어가 만든다.
    const response = await this.request({
      type: "introText",
      template: request.intro.template,
      paramsJson,
    });
    if (response.type !== "introText") throw new Error("예상하지 못한 응답입니다.");
    if (!response.text) return null;

    onProgress({ percent: 0, stage: TTS_STAGE });
    try {
      return await this.synthesize(response.text, request.intro.voiceId ?? TTS_MODEL.voices[0].id);
    } catch (error) {
      // 안내 음성을 만들지 못해도 음원은 만든다. 대신 그 사실을 알린다.
      const reason = error instanceof Error ? error.message : String(error);
      notices.push(`안내 음성 없이 만들었습니다. ${reason}`);
      return null;
    }
  }

  // ── 음원 생성 ───────────────────────────────────────────

  async generate(
    request: GenerateRequest,
    onProgress: (progress: ProgressInfo) => void,
  ): Promise<GeneratedTrack> {
    if (request.music?.kind === "url") {
      throw new Error("이 환경에서는 주소로 음악을 가져올 수 없습니다.");
    }
    const info = await this.describe();
    const paramsJson = JSON.stringify(request.params);
    const notices: string[] = [];
    const introBuffer = await this.makeIntro(request, paramsJson, notices, onProgress);

    // 파일 내용은 만들 때마다 새로 읽어 워커로 넘긴다(복사 없이 소유권 이전).
    const musicBuffer = request.music ? await request.music.file.arrayBuffer() : null;
    const transfer = [musicBuffer, introBuffer].filter((b): b is ArrayBuffer => b !== null);
    const response = await this.request(
      {
        type: "generate",
        paramsJson,
        format: request.format,
        music: musicBuffer,
        intro: introBuffer,
      },
      transfer,
      onProgress,
    );
    if (response.type !== "generated") throw new Error("예상하지 못한 응답입니다.");
    const mime =
      info.formats.find((f) => f.id === request.format)?.mime ?? "application/octet-stream";
    return {
      bytes: response.bytes,
      fileName: response.fileName,
      mime,
      musicReport: response.musicReportJson
        ? (JSON.parse(response.musicReportJson) as MusicReport)
        : undefined,
      notices,
    };
  }

  async save(track: GeneratedTrack): Promise<boolean> {
    const url = URL.createObjectURL(new Blob([track.bytes], { type: track.mime }));
    const link = document.createElement("a");
    link.href = url;
    link.download = track.fileName;
    document.body.append(link);
    link.click();
    link.remove();
    // 다운로드가 시작될 시간을 준 뒤 임시 주소를 해제한다.
    setTimeout(() => URL.revokeObjectURL(url), 60_000);
    return true;
  }
}
