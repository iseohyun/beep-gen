// 서비스 워커: 한 번 연 뒤에는 인터넷 없이도 앱이 열리게 한다.
//
// 등록 주소의 ?v= 값(빌드 태그)이 캐시 이름이다. 빌드 태그가 바뀌면 브라우저가
// 새 서비스 워커를 설치하고, 활성화될 때 이전 캐시를 모두 지운다.
const BUILD_TAG = new URL(self.location.href).searchParams.get("v") || "dev";
const CACHE_NAME = `beep-gen-${BUILD_TAG}`;
const CACHE_PREFIX = "beep-gen-";
// 서버가 Vary 헤더를 붙이면 요청 헤더가 조금만 달라도 캐시가 빗나간다.
// 여기 캐시는 주소만으로 구분되므로 Vary 는 무시한다.
const MATCH_OPTIONS = { ignoreVary: true };

// 설치할 때 앱을 이루는 파일을 모두 받아 둔다(목록은 빌드가 precache.json 으로 만든다).
// 하나라도 받지 못하면 설치가 실패하고, 브라우저가 다음 방문 때 다시 시도한다.
self.addEventListener("install", (event) => {
  event.waitUntil(
    (async () => {
      const response = await fetch("./precache.json", { cache: "no-cache" });
      const files = await response.json();
      const cache = await caches.open(CACHE_NAME);
      await cache.addAll(["./", ...files.map((name) => `./${name}`)]);
      await self.skipWaiting();
    })(),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const names = await caches.keys();
      await Promise.all(
        names
          .filter((name) => name.startsWith(CACHE_PREFIX) && name !== CACHE_NAME)
          .map((name) => caches.delete(name)),
      );
      await self.clients.claim();
    })(),
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;

  // 이름에 내용 해시가 들어간 파일(assets/)은 바뀌지 않으므로 캐시를 먼저 쓴다.
  // 그 외(index.html 등)는 최신을 먼저 받고, 오프라인일 때만 캐시를 쓴다.
  const immutable = url.pathname.includes("/assets/");
  event.respondWith(immutable ? cacheFirst(request) : networkFirst(request));
});

async function cacheFirst(request) {
  const cache = await caches.open(CACHE_NAME);
  const cached = await cache.match(request, MATCH_OPTIONS);
  if (cached) return cached;
  const response = await fetch(request);
  if (response.ok) cache.put(request, response.clone());
  return response;
}

async function networkFirst(request) {
  const cache = await caches.open(CACHE_NAME);
  try {
    const response = await fetch(request, { cache: "no-cache" });
    if (response.ok) cache.put(request, response.clone());
    return response;
  } catch (error) {
    const cached = await cache.match(request, {
      ...MATCH_OPTIONS,
      ignoreSearch: request.mode === "navigate",
    });
    if (cached) return cached;
    throw error;
  }
}
