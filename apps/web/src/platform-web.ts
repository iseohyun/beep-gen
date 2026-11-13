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
  Voice,
} from "./platform";
import type { WorkerRequest, WorkerResponse } from "./worker-protocol";

type Pending = {
  resolve: (response: WorkerResponse) => void;
  reject: (error: Error) => void;
  onProgress?: (progress: ProgressInfo) => void;
};

/** `WorkerRequest` 에서 id 를 뺀 형태(유니언의 각 갈래를 유지한다). */
type RequestBody = WorkerRequest extends infer R ? (R extends unknown ? Omit<R, "id"> : never) : never;

export class WebPlatform implements Platform {
  readonly capabilities: Capabilities = {
    musicFile: true,
    // 다른 사이트의 파일은 브라우저 보안 정책(CORS) 때문에 대부분 가져올 수 없다.
    musicUrl: false,
    // 브라우저는 내장 음성을 파일로 받을 수 없다. 내려받는 TTS 모델은 이후 단계에서 붙인다.
    ttsIntro: false,
    offlineCache: true,
  };

  private readonly worker: Worker;
  private readonly pending = new Map<number, Pending>();
  private nextId = 1;
  private info?: AppInfo;

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

  async listVoices(): Promise<Voice[]> {
    return [];
  }

  async generate(
    request: GenerateRequest,
    onProgress: (progress: ProgressInfo) => void,
  ): Promise<GeneratedTrack> {
    if (request.music?.kind === "url") {
      throw new Error("이 환경에서는 주소로 음악을 가져올 수 없습니다.");
    }
    const info = await this.describe();
    // 파일 내용은 만들 때마다 새로 읽어 워커로 넘긴다(복사 없이 소유권 이전).
    const musicBuffer = request.music ? await request.music.file.arrayBuffer() : null;
    const response = await this.request(
      {
        type: "generate",
        paramsJson: JSON.stringify(request.params),
        format: request.format,
        music: musicBuffer,
      },
      musicBuffer ? [musicBuffer] : [],
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
      notices: [],
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
