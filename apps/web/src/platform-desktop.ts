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
  MusicSource,
  Platform,
  ProgressInfo,
  SongGuess,
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

  /** 앱으로 마지막에 보내 둔 음악 파일. 같은 파일을 다시 보내지 않으려고 기억한다. */
  private uploaded?: Blob;

  /**
   * 음악을 앱의 Rust 쪽이 쓸 수 있는 형태로 넘긴다.
   * 고른 파일의 내용은 먼저 앱으로 보내 두고, 그 뒤로는 그것을 쓰라고만 알린다.
   */
  private async toAppSource(
    music: MusicSource,
  ): Promise<{ kind: "upload"; name: string } | { kind: "url"; url: string }> {
    if (music.kind === "url") return music;
    if (this.uploaded !== music.file) {
      this.uploaded = undefined;
      await call("set_music", new Uint8Array(await music.file.arrayBuffer()));
      this.uploaded = music.file;
    }
    return { kind: "upload", name: music.name };
  }

  // 파일의 태그 → 파일 이름 → 로컬 AI(Ollama) 순서로 찾는다.
  async identifyMusic(music: MusicSource): Promise<SongGuess> {
    return call<SongGuess>("identify_music", { source: await this.toAppSource(music) });
  }

  async generate(
    request: GenerateRequest,
    onProgress: (progress: ProgressInfo) => void,
  ): Promise<GeneratedTrack> {
    const info = await this.describe();
    const music = request.music ? await this.toAppSource(request.music) : null;

    const channel = new Channel<ProgressInfo>();
    channel.onmessage = onProgress;
    const response = await call<GenerateResponse>("generate", {
      request: {
        paramsJson: JSON.stringify(request.params),
        format: request.format,
        music,
        song: {
          artist: request.song.artist || null,
          title: request.song.title || null,
        },
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
