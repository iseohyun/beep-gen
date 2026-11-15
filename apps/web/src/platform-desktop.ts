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
  LevelGuide,
  MusicReport,
  MusicSource,
  Platform,
  ProgressInfo,
  SaveTarget,
  UpdateInfo,
  SongDetails,
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
    songTags: true,
    saveFolders: true,
    updates: true,
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
    // 파일 고르기 창으로 고른 파일은 앱이 이미 읽어 두었다.
    if (music.kind === "local") return { kind: "upload", name: music.name };
    if (this.uploaded !== music.file) {
      this.uploaded = undefined;
      await call("set_music", new Uint8Array(await music.file.arrayBuffer()));
      this.uploaded = music.file;
    }
    return { kind: "upload", name: music.name };
  }

  async pickMusic(): Promise<MusicSource | null> {
    const picked = await call<{ name: string; path: string } | null>("pick_music");
    if (!picked) return null;
    // 앱이 가진 음악이 바뀌었으므로, 예전에 보낸 파일은 다시 보내야 한다.
    this.uploaded = undefined;
    return { kind: "local", ...picked };
  }

  async saveSongInfo(info: SongDetails, rename: boolean): Promise<MusicSource> {
    const saved = await call<{ name: string; path: string }>("save_song_info", {
      info: {
        title: info.title || null,
        artist: info.artist || null,
        composer: info.composer || null,
        source_url: info.sourceUrl || null,
        ai_generated: info.aiGenerated,
        meter: info.meter,
        bpm: info.bpm,
      },
      rename,
    });
    return { kind: "local", ...saved };
  }

  // 파일의 태그 → 파일 이름 → 로컬 AI(Ollama) 순서로 찾는다.
  async identifyMusic(music: MusicSource): Promise<SongGuess> {
    return call<SongGuess>("identify_music", { source: await this.toAppSource(music) });
  }

  // 주소로 넣은 음악은 아직 내려받기 전이라 잴 수 없다.
  async musicLevels(music: MusicSource): Promise<LevelGuide | null> {
    if (music.kind === "url") return null;
    await this.toAppSource(music);
    return call<LevelGuide>("music_levels");
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
        preview: request.preview ?? null,
        nameFormat: request.nameFormat ?? null,
      },
      onProgress: channel,
    });

    const bytes = new Uint8Array(
      await call<ArrayBuffer>("result_bytes", { preview: request.preview !== undefined }),
    );
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
    this.savedPath = path;
    return true;
  }

  setTitle(title: string): void {
    document.title = title;
    // 제목을 바꾸지 못해도 앱을 쓰는 데는 지장이 없다.
    void call("set_title", { title }).catch(() => {});
  }

  pickFolder(): Promise<string | null> {
    return call<string | null>("pick_folder");
  }

  async saveTo(track: GeneratedTrack, target: SaveTarget): Promise<string | null> {
    const path = await call<string | null>("save_result_to", { target, fileName: track.fileName });
    if (path) this.savedPath = path;
    return path;
  }

  checkUpdate(): Promise<UpdateInfo | null> {
    return call<UpdateInfo | null>("check_update");
  }

  async applyUpdate(update: UpdateInfo): Promise<void> {
    await call("apply_update", { source: update.source });
  }

  /** 마지막으로 저장한 파일의 위치. */
  private savedPath: string | null = null;

  async showSavedFolder(): Promise<void> {
    if (!this.savedPath) throw new Error("아직 저장한 파일이 없습니다.");
    await call("show_in_folder", { path: this.savedPath });
  }
}
