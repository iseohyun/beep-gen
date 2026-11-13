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
}

export interface FormatInfo {
  /** 파일 확장자이자 식별자 (예: "mp3"). */
  id: string;
  label: string;
  mime: string;
}

/** 코어가 알려주는 화면 구성 정보. */
export interface AppInfo {
  version: string;
  params: ParamSpec[];
  formats: FormatInfo[];
}

/** 파라미터 값. 키는 `ParamSpec.key`. */
export type ParamValues = Record<string, number>;

export interface GeneratedTrack {
  bytes: Uint8Array<ArrayBuffer>;
  fileName: string;
  mime: string;
}

/** 이 환경에서 쓸 수 있는 기능. */
export interface Capabilities {
  /** TTS 인트로 음성을 음원에 넣을 수 있는지. */
  ttsIntro: boolean;
  /** URL 로 음악을 가져올 수 있는지. */
  urlInput: boolean;
}

export interface Platform {
  readonly capabilities: Capabilities;
  /** 화면 구성 정보를 가져온다. */
  describe(): Promise<AppInfo>;
  /** 음원을 만든다. `onProgress` 는 0~100 의 백분율을 받는다. */
  generate(
    params: ParamValues,
    format: string,
    onProgress: (percent: number) => void,
  ): Promise<GeneratedTrack>;
  /** 만든 음원을 사용자의 기기에 저장한다. */
  save(track: GeneratedTrack): Promise<void>;
}
