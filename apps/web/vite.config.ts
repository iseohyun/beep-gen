import { readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";

const PRECACHE_FILE = "precache.json";

/**
 * 빌드 결과물의 파일 목록을 precache.json 으로 내보낸다.
 * 서비스 워커가 설치될 때 이 목록을 미리 받아 두어, 첫 방문 직후부터 오프라인으로 열린다.
 */
function precacheManifest(): Plugin {
  return {
    name: "beep-gen-precache-manifest",
    apply: "build",
    generateBundle(_options, bundle) {
      const publicDir = fileURLToPath(new URL("./public/", import.meta.url));
      const files = [...Object.keys(bundle), ...readdirSync(publicDir)]
        // 서비스 워커 자신은 브라우저가 따로 관리한다.
        .filter((name) => name !== "sw.js")
        // 음성 합성 실행기(약 15MB)와 그 워커는 안내 음성을 쓰는 사람만 필요하므로
        // 미리 받지 않는다. 처음 쓸 때 받아지고, 그 뒤에는 서비스 워커가 보관한다.
        .filter((name) => !/ort-wasm|tts-worker/.test(name))
        .sort();
      this.emitFile({
        type: "asset",
        fileName: PRECACHE_FILE,
        source: JSON.stringify(files, null, 2),
      });
    },
  };
}

/** 빌드한 시각(월일-시분). 화면과 제목표시줄에 보여, 지금 보는 것이 어느 빌드인지 알 수 있게 한다. */
function buildStamp(): string {
  const now = new Date();
  const two = (value: number): string => String(value).padStart(2, "0");
  return `${two(now.getMonth() + 1)}${two(now.getDate())}-${two(now.getHours())}${two(now.getMinutes())}`;
}

export default defineConfig({
  define: {
    __BUILD_STAMP__: JSON.stringify(buildStamp()),
  },
  // 상대 경로로 빌드한다. GitHub Pages(/beep-gen/)와 로컬 테스트 서버(/beep-gen/,
  // /beep-gen@브랜치/) 어디에 올려도 같은 결과물이 동작해야 하기 때문이다.
  base: "./",
  build: {
    // 저장소 루트의 dist/ 로 내보낸다(로컬 테스트 서버가 dist/index.html 을 찾는다).
    // 같은 폴더에 PyInstaller 결과물(BeepGen.exe)이 있을 수 있어 통째로 비우지 않고,
    // 웹 결과물만 scripts/clean-dist.mjs 가 미리 지운다.
    outDir: "../../dist",
    emptyOutDir: false,
    target: "es2022",
  },
  worker: {
    format: "es",
  },
  plugins: [precacheManifest()],
});
