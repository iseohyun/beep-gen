// 음성 합성 모델의 내려받기와 보관.
//
// 브라우저의 Cache Storage 에 둔다. 한 번 받으면 인터넷 없이도 쓸 수 있고,
// 화면 스레드와 워커 양쪽에서 읽을 수 있다.

import { TTS_MODEL, modelUrl, totalModelBytes } from "./tts-model";

/** 모델 파일이 모두 보관돼 있는지. */
export async function isInstalled(): Promise<boolean> {
  if (!("caches" in globalThis)) return false;
  if (!(await caches.has(TTS_MODEL.cacheName))) return false;
  const cache = await caches.open(TTS_MODEL.cacheName);
  for (const file of TTS_MODEL.files) {
    if (!(await cache.match(modelUrl(file.path)))) return false;
  }
  return true;
}

/**
 * 모델 파일을 받아 보관한다. 이미 받은 파일은 건너뛰므로, 중간에 끊겨도 이어서 받을 수 있다.
 * `onProgress` 는 0~100 의 백분율을 받는다.
 */
export async function install(onProgress: (percent: number) => void): Promise<void> {
  const cache = await caches.open(TTS_MODEL.cacheName);
  const total = totalModelBytes();
  let done = 0;
  const report = (): void => onProgress(Math.min(100, Math.floor((done / total) * 100)));

  for (const file of TTS_MODEL.files) {
    const url = modelUrl(file.path);
    if (await cache.match(url)) {
      done += file.bytes;
      report();
      continue;
    }
    const response = await fetch(url);
    if (!response.ok || !response.body) {
      throw new Error(`모델 파일을 받지 못했습니다 (${file.path}: ${response.status})`);
    }

    // 받는 만큼 진행률을 알리면서 모은다.
    const reader = response.body.getReader();
    const chunks: Uint8Array<ArrayBuffer>[] = [];
    let received = 0;
    for (;;) {
      const { done: finished, value } = await reader.read();
      if (finished) break;
      chunks.push(value as Uint8Array<ArrayBuffer>);
      received += value.length;
      done += value.length;
      report();
    }
    // 중간에 끊긴 파일을 보관하면 나중에 알 수 없는 오류가 나므로 크기를 확인한다.
    if (received !== file.bytes) {
      throw new Error(
        `모델 파일이 온전히 받아지지 않았습니다 (${file.path}: ${received}/${file.bytes})`,
      );
    }
    await cache.put(
      url,
      new Response(new Blob(chunks), {
        headers: { "Content-Length": String(received), "Content-Type": "application/octet-stream" },
      }),
    );
  }
  onProgress(100);
  // 브라우저가 공간이 부족할 때 마음대로 지우지 않도록 요청한다(거절돼도 동작에는 지장이 없다).
  await navigator.storage?.persist?.().catch(() => false);
}

/** 보관한 모델을 지운다. */
export async function remove(): Promise<void> {
  if ("caches" in globalThis) await caches.delete(TTS_MODEL.cacheName);
}

/** 보관한 파일 하나를 읽는다. */
export async function read(path: string): Promise<ArrayBuffer> {
  const cache = await caches.open(TTS_MODEL.cacheName);
  const response = await cache.match(modelUrl(path));
  if (!response) throw new Error(`음성 모델 파일이 없습니다: ${path}`);
  return response.arrayBuffer();
}
