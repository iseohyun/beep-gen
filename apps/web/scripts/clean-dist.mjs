// 저장소 루트 dist/ 에서 이전 웹 빌드 결과물만 지운다.
// dist/ 에는 PyInstaller 가 만든 BeepGen.exe 가 함께 있을 수 있어 폴더 전체를 비우지 않는다.
import { readdirSync, rmSync } from "node:fs";
import { fileURLToPath } from "node:url";

const dist = fileURLToPath(new URL("../../../dist/", import.meta.url));
const publicDir = fileURLToPath(new URL("../public/", import.meta.url));

// 웹 빌드가 만드는 것: assets/ 폴더, index.html, public/ 에서 복사되는 파일들.
const targets = ["assets", "index.html", "precache.json", ...readdirSync(publicDir)];
for (const name of targets) {
  rmSync(dist + name, { recursive: true, force: true });
}
