// 화면(UI). 웹과 로컬앱이 이 코드를 그대로 함께 쓴다.
//
// 입력 폼은 코어가 알려주는 파라미터 명세로 만든다. 그래서 코어에 파라미터를
// 추가하면 화면에도 자동으로 나타나고, 이름·기본값·범위가 어긋날 수 없다.

import "./styles/app.css";
import type {
  AppInfo,
  GeneratedTrack,
  MusicReport,
  ParamSpec,
  ParamValues,
  Platform,
} from "./platform";
import { WebPlatform } from "./platform-web";
import { APP_VERSION, BUILD_TAG } from "./version";

const STORAGE_KEY = "beep-gen.settings.v1";
const LOCAL_APP_URL = "https://github.com/iseohyun/beep-gen";

type SavedSettings = { params: ParamValues; format: string };

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Partial<HTMLElementTagNameMap[K]> = {},
  ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children);
  return node;
}

function byId<T extends HTMLElement>(id: string): T {
  const node = document.getElementById(id);
  if (!node) throw new Error(`#${id} 요소가 없습니다.`);
  return node as T;
}

function loadSettings(): Partial<SavedSettings> {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}") as Partial<SavedSettings>;
  } catch {
    return {};
  }
}

function saveSettings(settings: SavedSettings): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // 저장소를 쓸 수 없어도(사생활 보호 모드 등) 동작에는 지장이 없다.
  }
}

function formatBytes(bytes: number): string {
  return bytes >= 1024 * 1024
    ? `${(bytes / 1024 / 1024).toFixed(1)} MB`
    : `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

function formatDuration(seconds: number): string {
  const whole = Math.round(seconds);
  return `${Math.floor(whole / 60)}분 ${String(whole % 60).padStart(2, "0")}초`;
}

/** 파라미터 하나의 입력 칸을 만든다. */
function createField(spec: ParamSpec, initial: number): HTMLElement {
  const inputId = `param-${spec.key}`;
  const hintId = `${inputId}-hint`;
  let control: HTMLInputElement | HTMLSelectElement;

  if (spec.kind === "choice") {
    control = el("select", { id: inputId });
    for (const choice of spec.choices) {
      control.append(el("option", { value: String(choice) }, choice.toLocaleString("ko-KR")));
    }
    control.value = String(spec.choices.includes(initial) ? initial : spec.default);
  } else {
    control = el("input", {
      id: inputId,
      type: "number",
      inputMode: spec.kind === "integer" ? "numeric" : "decimal",
      min: String(spec.min),
      max: String(spec.max),
      // 소수 항목은 증감 단위와 무관하게 어떤 값이든 받는다(예: 1.4, 1.25).
      step: spec.kind === "integer" ? "1" : "any",
      required: true,
      value: String(initial),
    });
  }
  control.dataset.key = spec.key;
  control.setAttribute("aria-describedby", hintId);

  const field = el(
    "div",
    { className: "field" },
    el("label", { htmlFor: inputId }, spec.label),
    el("div", { className: "field-control" }, control, el("span", { className: "unit" }, spec.unit)),
    el("p", { id: hintId, className: "hint" }, spec.description),
  );
  field.dataset.when = spec.when;
  return field;
}

/**
 * 배경 음악 유무에 따라 쓰이지 않는 항목을 숨긴다.
 * 숨긴 칸은 비활성화해서, 보이지 않는 칸의 값 때문에 제출이 막히는 일이 없게 한다.
 */
function applyMusicState(form: HTMLFormElement, hasMusic: boolean): void {
  for (const field of form.querySelectorAll<HTMLElement>("[data-when]")) {
    const when = field.dataset.when;
    const used = when === "always" || (when === "with-music") === hasMusic;
    field.hidden = !used;
    const control = field.querySelector<HTMLInputElement | HTMLSelectElement>("[data-key]");
    if (control) control.disabled = !used;
  }
}

/** 폼에서 현재 값을 읽는다. 범위를 벗어난 칸이 있으면 그 칸을 알려주고 null 을 돌려준다. */
function readParams(form: HTMLFormElement, info: AppInfo): ParamValues | null {
  if (!form.reportValidity()) return null;
  const values: ParamValues = {};
  for (const spec of info.params) {
    const control = form.querySelector<HTMLInputElement | HTMLSelectElement>(
      `[data-key="${spec.key}"]`,
    );
    if (!control) continue;
    // 숨겨져 쓰이지 않는 항목도 값은 그대로 보낸다(코어가 범위를 검사한다).
    const value = Number(control.value);
    values[spec.key] = Number.isFinite(value) && control.value !== "" ? value : spec.default;
  }
  return values;
}

/** 배경 음악을 어떻게 맞췄는지 보여준다. */
function renderMusicReport(container: HTMLElement, report: MusicReport | undefined): void {
  container.replaceChildren();
  container.hidden = !report;
  if (!report) return;

  const change = report.tempo_change_pct;
  const rows: [string, string][] = [
    [
      "검출한 템포",
      `${report.detected_bpm.toFixed(1)} BPM (신뢰도 ${Math.round(report.beat_confidence * 100)}%)`,
    ],
    ["템포 변화", `${change >= 0 ? "+" : ""}${change.toFixed(1)}%`],
    ["한 박의 길이", `${report.target_beat_sec.toFixed(3)}초`],
    ["길이", `${formatDuration(report.source_seconds)} → ${formatDuration(report.output_seconds)}`],
  ];
  const list = el("dl", { className: "report" });
  for (const [term, value] of rows) {
    list.append(el("dt", {}, term), el("dd", {}, value));
  }
  container.append(list);
  for (const warning of report.warnings) {
    container.append(el("p", { className: "warning" }, warning));
  }
}

async function start(platform: Platform): Promise<void> {
  const form = byId<HTMLFormElement>("settings");
  const basicFields = byId("basic-fields");
  const advancedFields = byId("advanced-fields");
  const formatSelect = byId<HTMLSelectElement>("format");
  const musicSection = byId("music-section");
  const musicInput = byId<HTMLInputElement>("music");
  const musicClear = byId<HTMLButtonElement>("music-clear");
  const generateButton = byId<HTMLButtonElement>("generate");
  const resetButton = byId<HTMLButtonElement>("reset");
  const progress = byId<HTMLProgressElement>("progress");
  const status = byId("status");
  const result = byId("result");
  const player = byId<HTMLAudioElement>("player");
  const resultName = byId("result-name");
  const musicReport = byId("music-report");
  const downloadButton = byId<HTMLButtonElement>("download");
  const ttsNote = byId("tts-note");

  byId("version").textContent = `v${APP_VERSION} · 빌드 ${BUILD_TAG}`;

  let info: AppInfo;
  try {
    info = await platform.describe();
  } catch (error) {
    status.textContent = `시작하지 못했습니다: ${error instanceof Error ? error.message : error}`;
    status.className = "status error";
    return;
  }

  const selectedMusic = (): File | null => musicInput.files?.[0] ?? null;
  const refreshMusicState = (): void => {
    const hasMusic = selectedMusic() !== null;
    musicClear.hidden = !hasMusic;
    applyMusicState(form, hasMusic);
  };

  const saved = loadSettings();
  const render = (values: ParamValues, format: string): void => {
    basicFields.replaceChildren();
    advancedFields.replaceChildren();
    for (const spec of info.params) {
      const field = createField(spec, values[spec.key] ?? spec.default);
      (spec.advanced ? advancedFields : basicFields).append(field);
    }
    formatSelect.replaceChildren(
      ...info.formats.map((f) => el("option", { value: f.id }, f.label)),
    );
    formatSelect.value = info.formats.some((f) => f.id === format) ? format : info.formats[0].id;
    refreshMusicState();
  };
  render(saved.params ?? {}, saved.format ?? "");

  // 기능이 있는 환경에서만 해당 입력을 보여주고, 없으면 그 사실과 대안을 알려준다.
  musicSection.hidden = !platform.capabilities.musicFile;
  if (!platform.capabilities.ttsIntro) {
    ttsNote.hidden = false;
    ttsNote.replaceChildren(
      "시작 안내 음성(TTS)은 이 환경에서 넣을 수 없습니다. ",
      el("a", { href: LOCAL_APP_URL, target: "_blank", rel: "noopener" }, "로컬앱"),
      "에서 지원할 예정입니다.",
    );
  }

  musicInput.addEventListener("change", refreshMusicState);
  musicClear.addEventListener("click", () => {
    musicInput.value = "";
    refreshMusicState();
  });

  let track: GeneratedTrack | null = null;
  let playerUrl: string | null = null;

  const setBusy = (busy: boolean): void => {
    generateButton.disabled = busy;
    resetButton.disabled = busy;
    musicInput.disabled = busy;
    musicClear.disabled = busy;
    progress.hidden = !busy;
    form.setAttribute("aria-busy", String(busy));
  };

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const params = readParams(form, info);
    if (!params) return;
    const format = formatSelect.value;
    saveSettings({ params, format });

    setBusy(true);
    progress.value = 0;
    status.className = "status";
    status.textContent = "음원을 만드는 중입니다…";
    const startedAt = performance.now();
    try {
      track = await platform.generate(params, format, selectedMusic(), (update) => {
        progress.value = update.percent;
        status.textContent = `${update.stage}… ${update.percent}%`;
      });
      if (playerUrl) URL.revokeObjectURL(playerUrl);
      playerUrl = URL.createObjectURL(new Blob([track.bytes], { type: track.mime }));
      player.src = playerUrl;
      resultName.textContent = `${track.fileName} (${formatBytes(track.bytes.length)})`;
      renderMusicReport(musicReport, track.musicReport);
      result.hidden = false;
      const seconds = ((performance.now() - startedAt) / 1000).toFixed(1);
      status.textContent = `완료했습니다 (${seconds}초).`;
    } catch (error) {
      status.className = "status error";
      status.textContent = error instanceof Error ? error.message : String(error);
    } finally {
      setBusy(false);
    }
  });

  downloadButton.addEventListener("click", () => {
    if (track) void platform.save(track);
  });

  resetButton.addEventListener("click", () => {
    render({}, "");
    status.className = "status";
    status.textContent = "기본값으로 되돌렸습니다.";
  });

  generateButton.disabled = false;
}

function registerServiceWorker(): void {
  // 개발 서버에서는 캐시가 수정 내용을 가리지 않도록 등록하지 않는다.
  if (!import.meta.env.PROD || !("serviceWorker" in navigator)) return;
  window.addEventListener("load", () => {
    // 빌드 태그가 바뀌면 주소가 달라져 브라우저가 새 서비스 워커를 설치한다.
    navigator.serviceWorker.register(`./sw.js?v=${BUILD_TAG}`).catch(() => {
      // 오프라인 캐시는 부가 기능이라 실패해도 앱은 그대로 동작한다.
    });
  });
}

void start(new WebPlatform());
registerServiceWorker();
