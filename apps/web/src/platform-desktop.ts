// 데스크톱 앱(Tauri)용 플랫폼 구현.
//
// 여기는 어댑터다: 앱의 Rust 쪽 명령을 부르고 저장 위치를 묻는 일만 한다.
// 음원을 만드는 계산은 Rust 쪽의 같은 코어가 네이티브 속도로 한다.

import { Channel, invoke } from "@tauri-apps/api/core";
import { save as askSavePath } from "@tauri-apps/plugin-dialog";

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

interface GenerateResponse {
  fileName: string;
  musicReport: MusicReport | null;
  notices: string[];
}

/** Rust 쪽이 돌려주는 오류(문자열)를 Error 로 바꾼다. */
async function call<T>(command: string, args?: Parameters<typeof invoke>[1]): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw error instanceof Error ? error : new Error(String(error));
  }
}

export class DesktopPlatform implements Platform {
  readonly capabilities: Capabilities = {
    musicFile: true,
    musicUrl: true,
    ttsIntro: true,
    offlineCache: false,
  };

  private info?: AppInfo;

  async describe(): Promise<AppInfo> {
    this.info ??= await call<AppInfo>("describe");
    return this.info;
  }

  listVoices(): Promise<Voice[]> {
    return call<Voice[]>("list_voices");
  }

  // OS 에 내장된 음성을 쓰므로 따로 받을 것이 없다.
  ttsSetup(): TtsSetup | null {
    return null;
  }

  async ttsInstalled(): Promise<boolean> {
    return true;
  }

  async installTts(): Promise<void> {}

  async removeTts(): Promise<void> {}

  async generate(
    request: GenerateRequest,
    onProgress: (progress: ProgressInfo) => void,
  ): Promise<GeneratedTrack> {
    const info = await this.describe();

    // 고른 파일의 내용은 먼저 앱으로 보내 두고, 만들 때는 그것을 쓰라고만 알린다.
    let music: { kind: "upload" } | { kind: "url"; url: string } | null = null;
    if (request.music?.kind === "file") {
      await call("set_music", new Uint8Array(await request.music.file.arrayBuffer()));
      music = { kind: "upload" };
    } else if (request.music?.kind === "url") {
      music = { kind: "url", url: request.music.url };
    }

    const channel = new Channel<ProgressInfo>();
    channel.onmessage = onProgress;
    const response = await call<GenerateResponse>("generate", {
      request: {
        paramsJson: JSON.stringify(request.params),
        format: request.format,
        music,
        intro: request.intro,
      },
      onProgress: channel,
    });

    const bytes = new Uint8Array(await call<ArrayBuffer>("result_bytes"));
    const mime =
      info.formats.find((f) => f.id === request.format)?.mime ?? "application/octet-stream";
    return {
      bytes,
      fileName: response.fileName,
      mime,
      musicReport: response.musicReport ?? undefined,
      notices: response.notices,
    };
  }

  async save(track: GeneratedTrack): Promise<boolean> {
    const extension = track.fileName.split(".").pop() ?? "";
    const path = await askSavePath({
      defaultPath: track.fileName,
      filters: [{ name: extension.toUpperCase(), extensions: [extension] }],
    });
    if (!path) return false;
    await call("save_result", { path });
    return true;
  }
}
