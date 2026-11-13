// 백그라운드 스레드: 브라우저 안에서 문구를 음성으로 합성한다(Supertonic 2, ONNX).
//
// 이것은 "음성 합성" 어댑터다. 로컬앱이 OS 의 음성 합성기를 부르듯, 웹에서는 이 워커가
// 음성 파일(WAV)을 만들어 코어에 넘긴다. 음원을 조립하는 계산은 여전히 코어가 한다.
// 처리 절차는 모델 제작사의 공식 예제(supertone-inc/supertonic, MIT)를 참고해 다시 썼다.

import * as ort from "onnxruntime-web/wasm";
import ortWasmUrl from "onnxruntime-web/ort-wasm-simd-threaded.wasm?url";

import { TTS_MODEL } from "./tts-model";
import { read } from "./tts-store";

export type TtsRequest = { id: number; text: string; voiceId: string };
export type TtsResponse =
  | { id: number; type: "done"; wav: ArrayBuffer }
  | { id: number; type: "error"; message: string };

/** 잡음에서 음성을 다듬어 가는 반복 횟수. 많을수록 음질이 좋고 느리다(공식 예제의 기본값). */
const DENOISE_STEPS = 8;
/** 말하는 속도(공식 예제의 기본값). */
const SPEED = 1.05;
const SESSION_NAMES = ["duration_predictor", "text_encoder", "vector_estimator", "vocoder"] as const;

type SessionName = (typeof SESSION_NAMES)[number];
interface ModelConfig {
  ae: { sample_rate: number; base_chunk_size: number };
  ttl: { chunk_compress_factor: number; latent_dim: number };
}
interface StyleJson {
  style_ttl: { dims: number[]; data: unknown[] };
  style_dp: { dims: number[]; data: unknown[] };
}
interface Model {
  sessions: Record<SessionName, ort.InferenceSession>;
  config: ModelConfig;
  indexer: number[];
}

ort.env.wasm.wasmPaths = { wasm: ortWasmUrl };
// 여러 스레드는 특수한 응답 헤더가 있는 사이트에서만 쓸 수 있다(GitHub Pages 는 해당 없음).
ort.env.wasm.numThreads = self.crossOriginIsolated
  ? Math.min(4, navigator.hardwareConcurrency || 1)
  : 1;

let modelPromise: Promise<Model> | null = null;

async function readJson<T>(path: string): Promise<T> {
  return JSON.parse(new TextDecoder().decode(await read(path))) as T;
}

function loadModel(): Promise<Model> {
  modelPromise ??= (async () => {
    const sessions = {} as Record<SessionName, ort.InferenceSession>;
    for (const name of SESSION_NAMES) {
      sessions[name] = await ort.InferenceSession.create(
        new Uint8Array(await read(`onnx/${name}.onnx`)),
        { executionProviders: ["wasm"], graphOptimizationLevel: "all" },
      );
    }
    return {
      sessions,
      config: await readJson<ModelConfig>("onnx/tts.json"),
      indexer: await readJson<number[]>("onnx/unicode_indexer.json"),
    };
  })();
  // 실패했으면 다음 요청에서 다시 시도할 수 있게 한다.
  modelPromise.catch(() => (modelPromise = null));
  return modelPromise;
}

/** 문구에 한글이 있으면 한국어, 없으면 영어로 읽는다. */
function detectLanguage(text: string): string {
  return /[ᄀ-ᇿ㄰-㆏가-힣]/.test(text) ? "ko" : "en";
}

/** 모델이 기대하는 형태로 문구를 다듬는다. */
function prepareText(text: string): string {
  let prepared = text.normalize("NFKD").replace(/\s+/g, " ").trim();
  if (!/[.!?"')\]]$/.test(prepared)) prepared += ".";
  const language = detectLanguage(text);
  return `<${language}>${prepared}</${language}>`;
}

/** 표준 정규 분포의 난수(음성 합성의 출발점이 되는 잡음). */
function gaussianNoise(count: number): Float32Array {
  const out = new Float32Array(count);
  for (let i = 0; i < count; i += 2) {
    const radius = Math.sqrt(-2 * Math.log(Math.max(Math.random(), 1e-4)));
    const angle = 2 * Math.PI * Math.random();
    out[i] = radius * Math.cos(angle);
    if (i + 1 < count) out[i + 1] = radius * Math.sin(angle);
  }
  return out;
}

function styleTensor(style: { dims: number[]; data: unknown[] }): ort.Tensor {
  const flat = (style.data as number[][]).flat(Infinity as 1) as number[];
  return new ort.Tensor("float32", Float32Array.from(flat), style.dims);
}

async function synthesize(text: string, voiceId: string): Promise<Float32Array> {
  const { sessions, config, indexer } = await loadModel();
  const style = await readJson<StyleJson>(`voice_styles/${voiceId}.json`);
  const styleTtl = styleTensor(style.style_ttl);
  const styleDp = styleTensor(style.style_dp);

  // 글자를 모델의 번호로 바꾼다. 모델이 모르는 글자가 있으면 엉뚱한 소리가 나므로 미리 거절한다.
  const prepared = prepareText(text);
  const ids: bigint[] = [];
  // 글자 단위로 돈다(이모지처럼 두 칸을 차지하는 글자가 반으로 쪼개지지 않게).
  for (const character of prepared) {
    const code = character.codePointAt(0) ?? 0;
    const id = code < indexer.length ? indexer[code] : -1;
    if (id < 0) throw new Error(`읽을 수 없는 글자가 있습니다: "${character}"`);
    ids.push(BigInt(id));
  }
  const length = ids.length;
  const textIds = new ort.Tensor("int64", BigInt64Array.from(ids), [1, length]);
  const textMask = new ort.Tensor("float32", new Float32Array(length).fill(1), [1, 1, length]);

  // 1) 말의 길이 예측  2) 문구 해석
  const predicted = await sessions.duration_predictor.run({
    text_ids: textIds,
    style_dp: styleDp,
    text_mask: textMask,
  });
  const seconds = Number((predicted.duration.data as Float32Array)[0]) / SPEED;
  const encoded = await sessions.text_encoder.run({
    text_ids: textIds,
    style_ttl: styleTtl,
    text_mask: textMask,
  });

  // 3) 잡음에서 출발해 여러 번에 걸쳐 음성의 밑그림(잠재 표현)으로 다듬는다.
  const sampleRate = config.ae.sample_rate;
  const frameSize = config.ae.base_chunk_size * config.ttl.chunk_compress_factor;
  const channels = config.ttl.latent_dim * config.ttl.chunk_compress_factor;
  const frames = Math.ceil(Math.floor(seconds * sampleRate) / frameSize);
  const dims = [1, channels, frames];
  const latentMask = new ort.Tensor("float32", new Float32Array(frames).fill(1), [1, 1, frames]);
  let latent = new ort.Tensor("float32", gaussianNoise(channels * frames), dims);
  for (let step = 0; step < DENOISE_STEPS; step++) {
    const out = await sessions.vector_estimator.run({
      noisy_latent: latent,
      text_emb: encoded.text_emb,
      style_ttl: styleTtl,
      latent_mask: latentMask,
      text_mask: textMask,
      current_step: new ort.Tensor("float32", Float32Array.from([step]), [1]),
      total_step: new ort.Tensor("float32", Float32Array.from([DENOISE_STEPS]), [1]),
    });
    latent = new ort.Tensor("float32", out.denoised_latent.data as Float32Array, dims);
  }

  // 4) 밑그림을 실제 소리로 바꾼다.
  const vocoded = await sessions.vocoder.run({ latent });
  const wave = vocoded.wav_tts.data as Float32Array;
  return wave.slice(0, Math.min(wave.length, Math.floor(seconds * sampleRate)));
}

/** 샘플을 16비트 모노 WAV 파일로 포장한다(코어가 파일 형태로 받는다). */
function toWav(samples: Float32Array, sampleRate: number): ArrayBuffer {
  const buffer = new ArrayBuffer(44 + samples.length * 2);
  const view = new DataView(buffer);
  const writeText = (offset: number, text: string): void => {
    for (let i = 0; i < text.length; i++) view.setUint8(offset + i, text.charCodeAt(i));
  };
  writeText(0, "RIFF");
  view.setUint32(4, 36 + samples.length * 2, true);
  writeText(8, "WAVEfmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true); // PCM
  view.setUint16(22, 1, true); // 모노
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  writeText(36, "data");
  view.setUint32(40, samples.length * 2, true);
  for (let i = 0; i < samples.length; i++) {
    const clamped = Math.max(-1, Math.min(1, samples[i]));
    view.setInt16(44 + i * 2, Math.round(clamped * 32767), true);
  }
  return buffer;
}

self.onmessage = async (event: MessageEvent<TtsRequest>) => {
  const { id, text, voiceId } = event.data;
  const post = (message: TtsResponse, transfer: Transferable[] = []): void =>
    (self as unknown as Worker).postMessage(message, transfer);
  try {
    const voice = TTS_MODEL.voices.some((v) => v.id === voiceId) ? voiceId : TTS_MODEL.voices[0].id;
    const samples = await synthesize(text, voice);
    const { config } = await loadModel();
    const wav = toWav(samples, config.ae.sample_rate);
    post({ id, type: "done", wav }, [wav]);
  } catch (error) {
    post({ id, type: "error", message: error instanceof Error ? error.message : String(error) });
  }
};
