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
  advanced: boolean;
  /** 이 항목이 쓰이는 경우. 쓰이지 않을 때는 화면에서 숨긴다. */
  when: "always" | "with-music" | "without-music";
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
  hint: string;
}

/** 코어가 알려주는 화면 구성 정보. 코어(Rust)의 `AppInfo` 와 같은 모양이다. */
export interface AppInfo {
  version: string;
  params: ParamSpec[];
  formats: FormatInfo[];
  intro: IntroInfo;
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

/** 배경 음악을 어디서 가져올지. */
export type MusicSource = { kind: "file"; file: Blob } | { kind: "url"; url: string };

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
  intro: IntroOptions | null;
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
  /** 음원을 만든다. */
  generate(
    request: GenerateRequest,
    onProgress: (progress: ProgressInfo) => void,
  ): Promise<GeneratedTrack>;
  /** 만든 음원을 사용자의 기기에 저장한다. 사용자가 취소하면 `false`. */
  save(track: GeneratedTrack): Promise<boolean>;
}
