// 플랫폼 인터페이스: 화면(UI)이 아는 유일한 창구.
//
// 화면은 여기 정의된 것만 호출하고, 웹인지 로컬앱인지는 묻지 않는다.
// 기능이 있는지는 `capabilities` 로만 판단한다(환경 이름으로 분기 금지).

/** 파라미터 하나의 명세. 코어(Rust)의 `ParamSpec` 과 같은 모양이다. */
export interface ParamSpec {
  key: string;
  label: string;
  description: string;
  unit: string;
  kind: "number" | "integer" | "choice";
  default: number;
  min: number;
  max: number;
  step: number;
  choices: number[];
  /** 고를 수 있는 값마다의 이름(`choices` 와 같은 순서). 비어 있으면 값을 그대로 보여준다. */
  choice_labels: string[];
  advanced: boolean;
  /** 이 항목이 쓰이는 경우. 쓰이지 않을 때는 화면에서 숨긴다. */
  when: "always" | "with-music" | "without-music";
  /**
   * 소리 크기(%) 항목이면 dB 슬라이더로 보여줄 때의 눈금.
   * `dB = top_db + 20 × log10(값 / 100)`, `floor_db` 이하는 0%(끔).
   */
  level: { top_db: number; floor_db: number; ceil_db: number } | null;
}

/** 슬라이더가 달라붙는 지점 하나. */
export interface Magnet {
  key: string;
  label: string;
  description: string;
  /** 음악 음량이 100% 일 때의 크기(dB). 음악 음량을 낮추면 그만큼 함께 내려간다. */
  db: number;
}

/** 곡의 소리 크기에서 구한 음량 안내. 코어(Rust)의 `LevelGuide` 와 같은 모양이다. */
export interface LevelGuide {
  levels: { source_peak_db: number; mean_db: number; mode_db: number };
  /** 비프 음량 슬라이더가 달라붙는 지점들. */
  magnets: Magnet[];
  /** 처음에 맞춰 둘 비프 크기(음악 음량 100% 기준 dB). */
  default_beep_db: number;
  /** 곡의 박자 계열과 템포. 박자를 찾지 못한 곡은 `null`. */
  tempo: TempoInfo | null;
}

/** 곡의 박자 계열과 템포. 코어(Rust)의 `TempoInfo` 와 같은 모양이다. */
export interface TempoInfo {
  /** 2 = 2박 계열(2/4, 4/4), 3 = 3박 계열(3/4, 6/8). */
  meter: number;
  /** 템포(BPM). 3박 계열은 강세(맥박 세 개)의 빠르기. */
  bpm: number;
  /** 맥박의 빠르기(BPM). 템포는 이 값을 계열(2 또는 3)로 나눈 것이다. */
  pulse_bpm: number;
  /** 템포가 끝까지 일정한 곡인지. */
  steady: boolean;
  confidence: number;
}

export interface FormatInfo {
  /** 파일 확장자이자 식별자 (예: "mp3"). */
  id: string;
  label: string;
  mime: string;
}

/** 시작 안내 음성에 대한 정보. */
export interface IntroInfo {
  default_template: string;
  /** 고를 수 있는 안내 문구. 첫 번째가 기본값이다. */
  templates: string[];
  hint: string;
}

/** 코어가 알려주는 화면 구성 정보. 코어(Rust)의 `AppInfo` 와 같은 모양이다. */
export interface AppInfo {
  version: string;
  params: ParamSpec[];
  formats: FormatInfo[];
  intro: IntroInfo;
  /** 미리듣기의 기본 길이와 시작 위치. */
  preview: PreviewOptions;
  /** 고를 수 있는 결과 파일 이름 형식. 첫 번째가 기본값이다. */
  name_formats: string[];
  /** 이름 형식에 쓸 수 있는 자리표시자 설명. */
  name_format_hint: string;
}

/** 미리듣기: 전체 대신 일부 구간만 만든다. 코어(Rust)의 `Excerpt` 와 같은 모양이다. */
export interface PreviewOptions {
  /** 길이(초). */
  seconds: number;
  /** 시작 위치(전체 길이의 %). 가장 가까운 강한 비프에서 시작한다. */
  from_percent: number;
}

/** 파라미터 값. 키는 `ParamSpec.key`. */
export type ParamValues = Record<string, number>;

/** 배경 음악을 어떻게 맞췄는지에 대한 보고. 코어(Rust)의 `MusicReport` 와 같은 모양이다. */
export interface MusicReport {
  detected_bpm: number;
  beat_confidence: number;
  target_beat_sec: number;
  tempo_rate: number;
  tempo_change_pct: number;
  correction: number;
  /** 곡 안에서 템포가 평균에서 벗어난 최대 폭(%). */
  tempo_drift_pct: number;
  /** 템포가 끝까지 일정한 곡으로 보고 고정 격자에 맞췄는지(아니면 박마다 따라갔다). */
  steady_tempo: boolean;
  /** 3박 계열(6/8 등)로 보고 맥박 세 개를 한 박으로 잡았는지. */
  triple_meter: boolean;
  source_seconds: number;
  output_seconds: number;
  anchored_on_downbeat: boolean;
  warnings: string[];
}

/** 고를 수 있는 음성 하나. */
export interface Voice {
  id: string;
  name: string;
  language: string;
}

/** 안내 음성을 쓰기 전에 사용자가 내려받아야 하는 것에 대한 안내. */
export interface TtsSetup {
  modelName: string;
  /** 내려받을 크기(MB). */
  sizeMB: number;
  /** 어디서 받는지. */
  source: string;
  licenseName: string;
  licenseUrl: string;
  /** 사용자에게 알려야 하는 사용 조건. */
  terms: readonly string[];
}

/** 배경 음악을 어디서 가져올지. `name` 은 고른 파일의 원래 이름이다. */
export type MusicSource =
  | { kind: "file"; file: Blob; name: string }
  /** 이 기기의 파일 고르기 창으로 고른 파일. 위치를 알기 때문에 곡 정보를 원본에 저장할 수 있다. */
  | { kind: "local"; path: string; name: string }
  | { kind: "url"; url: string };

/** 음악 파일에 적어 둘 곡 정보. 모르는 항목은 빈 문자열 또는 `null`. */
export interface SongDetails {
  artist: string;
  title: string;
  composer: string;
  /** 곡을 받은 곳의 주소. */
  sourceUrl: string;
  /** AI 로 만든 곡인지. 모르면 `null`. */
  aiGenerated: boolean | null;
  /** 박자 계열(2 또는 3)과 템포(BPM). 모르면 `null`. */
  meter: number | null;
  bpm: number | null;
}

/** 결과 파일의 이름에 넣을 가수와 곡 제목. 모르는 쪽은 빈 문자열. */
export interface SongInput {
  artist: string;
  title: string;
}

/** 배경 음악의 가수와 곡 제목에 대한 추정. */
export interface SongGuess {
  artist: string | null;
  title: string | null;
  /** 알아낸 곳: 파일의 태그, 파일 이름, 로컬 AI. 알아내지 못했으면 `null`. */
  by: "tags" | "file-name" | "ai" | null;
  /** 로컬 AI 가 답했을 때 그 모델의 이름. */
  model?: string | null;
  /** 로컬 AI 에 물었지만 실패했을 때의 이유. */
  problem?: string | null;
  /** 음악 파일에 적혀 있던 나머지 정보. */
  composer?: string | null;
  source_url?: string | null;
  ai_generated?: boolean | null;
  /** 지난번에 분석해 음악 파일에 적어 둔 박자 계열과 템포. */
  meter?: number | null;
  bpm?: number | null;
}

/** 시작 안내 음성 설정. */
export interface IntroOptions {
  /** 읽을 문구. `{n}`, `{m}` 은 설정값으로 바뀐다. */
  template: string;
  /** 쓸 음성. 생략하면 문구의 언어에 맞는 기본 음성을 쓴다. */
  voiceId?: string;
}

export interface GenerateRequest {
  params: ParamValues;
  format: string;
  music: MusicSource | null;
  /** 배경 음악의 가수와 곡 제목(사용자가 확인한 값). 결과 파일의 이름에 쓴다. */
  song: SongInput;
  intro: IntroOptions | null;
  /** 미리듣기로 만들 때만 준다. 안내 음성은 넣지 않는다. */
  preview?: PreviewOptions;
  /** 결과 파일의 이름 형식(예: `{곡} {n}초`). 생략하면 기본 형식. */
  nameFormat?: string;
}

export interface GeneratedTrack {
  bytes: Uint8Array<ArrayBuffer>;
  fileName: string;
  mime: string;
  /** 배경 음악을 넣었을 때만 있다. */
  musicReport?: MusicReport;
  /** 음원은 만들었지만 사용자가 알아야 할 일(예: 안내 음성을 넣지 못함). */
  notices: string[];
}

/** 진행 상황. `stage` 는 사람이 읽는 단계 설명이다. */
export interface ProgressInfo {
  percent: number;
  stage: string;
}

/** 이 환경에서 쓸 수 있는 기능. */
export interface Capabilities {
  /** 배경 음악 파일을 고를 수 있는지. */
  musicFile: boolean;
  /** 음악을 주소(URL)로 가져올 수 있는지. */
  musicUrl: boolean;
  /** TTS 인트로 음성을 음원에 넣을 수 있는지. */
  ttsIntro: boolean;
  /** 서비스 워커로 오프라인 캐시를 둘 환경인지(웹 브라우저). */
  offlineCache: boolean;
  /**
   * 곡 정보를 음악 파일(원본)에 저장할 수 있는지. 그러려면 파일의 위치를 알아야 하므로,
   * 이 기능이 있는 환경에서는 `pickMusic()` 으로 파일을 고른다.
   */
  songTags: boolean;
  /** 결과를 정해 둔 폴더에 바로 저장할 수 있는지(`saveTo`). 없으면 `save` 로 내려받는다. */
  saveFolders: boolean;
  /** 새 빌드가 있는지 확인하고 그것으로 바꿀 수 있는지(`checkUpdate`, `applyUpdate`). */
  updates: boolean;
}

/** 찾은 새 빌드. */
export interface UpdateInfo {
  /** 새 실행 파일의 위치. */
  path: string;
  /** 새 실행 파일이 만들어진 시각(1970년부터의 밀리초). */
  built_at_ms: number;
}

/** 결과를 저장할 곳. */
export type SaveWhere = "music-output" | "music-folder" | "downloads" | "my-music" | "folder";
export interface SaveTarget {
  where: SaveWhere;
  /** `where` 가 `"folder"` 일 때의 폴더 위치. */
  folder?: string;
}

export interface Platform {
  readonly capabilities: Capabilities;
  /** 화면 구성 정보를 가져온다. */
  describe(): Promise<AppInfo>;
  /** 안내 음성에 쓸 수 있는 음성 목록. TTS 를 지원하지 않으면 빈 목록. */
  listVoices(): Promise<Voice[]>;
  /** 안내 음성을 쓰기 전에 내려받아야 하는 것. 바로 쓸 수 있는 환경이면 `null`. */
  ttsSetup(): TtsSetup | null;
  /** 안내 음성을 지금 바로 만들 수 있는지(필요한 것을 이미 받았는지). */
  ttsInstalled(): Promise<boolean>;
  /** 안내 음성에 필요한 것을 내려받는다. `onProgress` 는 0~100 의 백분율을 받는다. */
  installTts(onProgress: (percent: number) => void): Promise<void>;
  /** 내려받은 것을 지운다. */
  removeTts(): Promise<void>;
  /**
   * 배경 음악의 가수와 곡 제목을 알아낸다. 알아내지 못하면 지어내지 않고 빈 값을 돌려준다.
   * 환경에 따라 찾는 범위가 다르다(로컬앱은 로컬 AI 에도 묻는다).
   */
  identifyMusic(music: MusicSource): Promise<SongGuess>;
  /**
   * 곡의 소리 크기를 재서 음량 안내를 돌려준다. 잴 수 없으면(주소로 넣은 음악 등) `null`.
   */
  musicLevels(music: MusicSource): Promise<LevelGuide | null>;
  /** 이 기기의 파일 고르기 창으로 음악 파일을 고른다. 취소했거나 지원하지 않으면 `null`. */
  pickMusic(): Promise<MusicSource | null>;
  /**
   * 지금 고른 음악 파일(원본)에 곡 정보를 적는다. `rename` 이면 이름도 "가수 - 곡" 으로 바꾼다.
   * 바뀐 파일을 가리키는 `MusicSource` 를 돌려준다.
   */
  saveSongInfo(info: SongDetails, rename: boolean): Promise<MusicSource>;
  /** 음원을 만든다. */
  generate(
    request: GenerateRequest,
    onProgress: (progress: ProgressInfo) => void,
  ): Promise<GeneratedTrack>;
  /** 만든 음원을 사용자의 기기에 저장한다. 사용자가 취소하면 `false`. */
  save(track: GeneratedTrack): Promise<boolean>;
  /** 창(또는 탭)의 제목을 바꾼다. */
  setTitle(title: string): void;
  /** 폴더 고르기 창으로 저장할 폴더를 고른다. 취소했거나 지원하지 않으면 `null`. */
  pickFolder(): Promise<string | null>;
  /**
   * 만든 음원을 `target` 폴더에 바로 저장하고 저장한 위치를 돌려준다.
   * 같은 이름의 파일이 있어 사용자가 덮어쓰기를 거절하면 `null`.
   */
  saveTo(track: GeneratedTrack, target: SaveTarget): Promise<string | null>;
  /** 마지막으로 저장한 파일이 있는 폴더를 파일 탐색기로 연다(`capabilities.saveFolders` 일 때만). */
  showSavedFolder(): Promise<void>;
  /** 지금 실행 중인 것보다 새로운 빌드가 있으면 알려준다. 없거나 지원하지 않으면 `null`. */
  checkUpdate(): Promise<UpdateInfo | null>;
  /** 새 빌드로 바꾸고 앱을 다시 시작한다. */
  applyUpdate(): Promise<void>;
}
