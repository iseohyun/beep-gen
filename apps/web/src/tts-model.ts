// 웹용 음성 합성 모델(Supertonic 2)의 출처·파일·사용 조건.
//
// 모델은 용량이 커서 앱에 넣지 않는다. 사용자가 원할 때만 Hugging Face 에서 받아
// 브라우저에 보관한다. (정책: .agents/rules/project_architecture.md 4.1절)

/** 받을 파일의 저장소 버전. 내용이 바뀌어도 같은 파일을 받도록 고정한다. */
const REVISION = "75e6727618a02f323c720cba9478152d4bc16ca4";

/** 모델 파일의 기준 주소. 개발 중 시험할 때는 VITE_TTS_MODEL_BASE 로 바꿀 수 있다. */
const BASE_URL: string =
  import.meta.env.VITE_TTS_MODEL_BASE ??
  `https://huggingface.co/Supertone/supertonic-2/resolve/${REVISION}/`;

export interface ModelFile {
  path: string;
  /** 파일 크기(바이트). 진행률 계산과 받은 파일의 검증에 쓴다. */
  bytes: number;
}

export const TTS_MODEL = {
  name: "Supertonic 2",
  /** 브라우저 저장소(Cache Storage)의 이름. 앱 캐시(beep-gen-…)와 겹치지 않게 한다. */
  cacheName: `tts-supertonic-2-${REVISION.slice(0, 8)}`,
  baseUrl: BASE_URL,
  source: "Hugging Face (huggingface.co/Supertone/supertonic-2)",
  files: [
    { path: "onnx/duration_predictor.onnx", bytes: 1_521_526 },
    { path: "onnx/text_encoder.onnx", bytes: 27_431_318 },
    { path: "onnx/vector_estimator.onnx", bytes: 132_471_364 },
    { path: "onnx/vocoder.onnx", bytes: 101_405_066 },
    { path: "onnx/tts.json", bytes: 8_699 },
    { path: "onnx/unicode_indexer.json", bytes: 262_196 },
    { path: "voice_styles/F1.json", bytes: 420_050 },
    { path: "voice_styles/M1.json", bytes: 420_510 },
  ] satisfies ModelFile[],
  /** 고를 수 있는 목소리. id 는 voice_styles/<id>.json 의 이름이다. */
  voices: [
    { id: "F1", name: "여성 목소리", language: "ko, en" },
    { id: "M1", name: "남성 목소리", language: "ko, en" },
  ],
  /** 모델이 지원하는 언어 코드. */
  languages: ["en", "ko", "es", "pt", "fr"],
  licenseName: "OpenRAIL-M",
  licenseUrl: "https://huggingface.co/Supertone/supertonic-2/blob/main/LICENSE",
  /** 사용자에게 알려야 하는 사용 조건의 요약. 전문은 licenseUrl 에 있다. */
  terms: [
    "법을 어기거나 남을 해치는 용도, 허위 정보를 퍼뜨리는 용도로 쓸 수 없습니다.",
    "동의 없이 다른 사람을 사칭하는 음성을 만들 수 없습니다.",
    "만든 음성을 공개할 때는 기계가 만든 음성임을 밝혀야 합니다.",
  ],
} as const;

export function totalModelBytes(): number {
  return TTS_MODEL.files.reduce((sum, file) => sum + file.bytes, 0);
}

export function modelUrl(path: string): string {
  return TTS_MODEL.baseUrl + path;
}
