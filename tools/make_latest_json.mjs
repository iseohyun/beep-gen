// 로컬앱 업데이트용 릴리즈 파일을 한 폴더에 모은다.
//
//   node tools/make_latest_json.mjs <nsis 폴더> <버전> <내려받는 주소의 앞부분> <결과 폴더> [설명]
//
// 예: node tools/make_latest_json.mjs target/release/bundle/nsis 0.2.0 \
//       https://github.com/iseohyun/beep-gen/releases/download/v0.2.0 release
//
// <nsis 폴더> 에서 서명된 설치 파일(`*_<버전>_x64-setup.exe` 와 `.sig`)을 찾아
// <결과 폴더> 에 `beep-gen_<버전>_x64-setup.exe` 로 복사하고, 앱이 새 버전을 확인할 때 읽는
// `latest.json` 을 만든다. 이 폴더의 파일을 모두 GitHub 릴리즈에 올리면 된다.
// (이름을 바꾸는 이유: GitHub 는 올린 파일 이름의 공백을 점으로 바꾸므로, 주소가 달라지지 않게
// 공백이 없는 이름을 쓴다.)
import { copyFileSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [bundleDir, version, baseUrl, outDir, notes] = process.argv.slice(2);
if (!bundleDir || !version || !baseUrl || !outDir) {
  console.error("사용법: node tools/make_latest_json.mjs <nsis 폴더> <버전> <주소 앞부분> <결과 폴더> [설명]");
  process.exit(2);
}

const suffix = `_${version}_x64-setup.exe`;
const installers = readdirSync(bundleDir).filter((name) => name.endsWith(suffix));
if (installers.length !== 1) {
  console.error(`${bundleDir} 에서 ${version} 버전의 설치 파일을 하나로 정하지 못했습니다: ${installers.join(", ") || "없음"}`);
  process.exit(1);
}
const installer = join(bundleDir, installers[0]);

let signature;
try {
  signature = readFileSync(`${installer}.sig`, "utf8").trim();
} catch {
  console.error(
    `${installer}.sig 가 없습니다. 서명 키(TAURI_SIGNING_PRIVATE_KEY)를 준 채로 빌드했는지 확인하세요.`,
  );
  process.exit(1);
}

const assetName = `beep-gen${suffix}`;
mkdirSync(outDir, { recursive: true });
copyFileSync(installer, join(outDir, assetName));
copyFileSync(`${installer}.sig`, join(outDir, `${assetName}.sig`));

const latest = {
  version,
  notes: notes ?? "",
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature,
      url: `${baseUrl.replace(/\/$/, "")}/${assetName}`,
    },
  },
};
writeFileSync(join(outDir, "latest.json"), `${JSON.stringify(latest, null, 2)}\n`);
console.log(`${outDir} 에 ${assetName}, ${assetName}.sig, latest.json 을 만들었습니다.`);
