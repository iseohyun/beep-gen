// 화면(UI). 웹과 로컬앱이 이 코드를 그대로 함께 쓴다.
//
// 입력 폼은 코어가 알려주는 파라미터 명세로 만든다. 그래서 코어에 파라미터를
// 추가하면 화면에도 자동으로 나타나고, 이름·기본값·범위가 어긋날 수 없다.

import "./styles/app.css";
import type {
  AppInfo,
  GeneratedTrack,
  IntroOptions,
  MusicReport,
  MusicSource,
  ParamSpec,
  ParamValues,
  Platform,
} from "./platform";
import { APP_VERSION, BUILD_TAG } from "./version";

const STORAGE_KEY = "beep-gen.settings.v1";
const LOCAL_APP_URL = "https://github.com/iseohyun/beep-gen";

type SavedSettings = {
  params: ParamValues;
  format: string;
  introTemplate?: string;
  introVoiceId?: string;
};

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

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
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

/** 배경 음악을 어떻게 맞췄는지와 알림을 보여준다. */
function renderReport(container: HTMLElement, track: GeneratedTrack): void {
  container.replaceChildren();
  const report: MusicReport | undefined = track.musicReport;
  if (report) {
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
  }
  for (const text of [...track.notices, ...(report?.warnings ?? [])]) {
    container.append(el("p", { className: "warning" }, text));
  }
  container.hidden = container.childElementCount === 0;
}

async function start(platform: Platform): Promise<void> {
  const form = byId<HTMLFormElement>("settings");
  const basicFields = byId("basic-fields");
  const advancedFields = byId("advanced-fields");
  const formatSelect = byId<HTMLSelectElement>("format");
  const musicSection = byId("music-section");
  const musicInput = byId<HTMLInputElement>("music");
  const musicClear = byId<HTMLButtonElement>("music-clear");
  const musicUrlRow = byId("music-url-row");
  const musicUrl = byId<HTMLInputElement>("music-url");
  const introSection = byId("intro-section");
  const introTemplate = byId<HTMLInputElement>("intro-template");
  const introVoice = byId<HTMLSelectElement>("intro-voice");
  const introHint = byId("intro-hint");
  const introSetup = byId("intro-setup");
  const introSetupText = byId("intro-setup-text");
  const introSetupTerms = byId("intro-setup-terms");
  const introSetupLicense = byId("intro-setup-license");
  const introInstall = byId<HTMLButtonElement>("intro-install");
  const introInstallProgress = byId<HTMLProgressElement>("intro-install-progress");
  const introInstallStatus = byId("intro-install-status");
  const introControls = byId("intro-controls");
  const introRemove = byId<HTMLButtonElement>("intro-remove");
  const generateButton = byId<HTMLButtonElement>("generate");
  const resetButton = byId<HTMLButtonElement>("reset");
  const progress = byId<HTMLProgressElement>("progress");
  const status = byId("status");
  const result = byId("result");
  const player = byId<HTMLAudioElement>("player");
  const resultName = byId("result-name");
  const reportBox = byId("music-report");
  const downloadButton = byId<HTMLButtonElement>("download");
  const ttsNote = byId("tts-note");
  const capabilities = platform.capabilities;

  byId("version").textContent = `v${APP_VERSION} · 빌드 ${BUILD_TAG}`;

  let info: AppInfo;
  try {
    info = await platform.describe();
  } catch (error) {
    status.textContent = `시작하지 못했습니다: ${errorMessage(error)}`;
    status.className = "status error";
    return;
  }

  /** 지금 고른 배경 음악. 파일이 주소보다 우선한다. */
  const selectedMusic = (): MusicSource | null => {
    const file = musicInput.files?.[0];
    if (file) return { kind: "file", file };
    const url = capabilities.musicUrl ? musicUrl.value.trim() : "";
    return url ? { kind: "url", url } : null;
  };
  const refreshMusicState = (): void => {
    const hasFile = Boolean(musicInput.files?.[0]);
    musicClear.hidden = !hasFile;
    // 파일을 골랐으면 주소 칸은 쓰이지 않으므로 숨긴다.
    musicUrlRow.hidden = !capabilities.musicUrl || hasFile;
    applyMusicState(form, selectedMusic() !== null);
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
  musicSection.hidden = !capabilities.musicFile;
  introSection.hidden = !capabilities.ttsIntro;
  /** 안내 음성을 지금 바로 만들 수 있는지(필요한 것을 받아 두었는지). */
  let introReady = false;
  if (capabilities.ttsIntro) {
    introTemplate.value = saved.introTemplate ?? info.intro.default_template;
    introHint.textContent = info.intro.hint;
    try {
      for (const voice of await platform.listVoices()) {
        introVoice.append(el("option", { value: voice.id }, `${voice.name} (${voice.language})`));
      }
      if (saved.introVoiceId) introVoice.value = saved.introVoiceId;
      // 저장해 둔 음성이 지금은 없으면 자동 선택으로 되돌린다.
      if (introVoice.selectedIndex < 0) introVoice.value = "";
    } catch (error) {
      introHint.textContent = `음성 목록을 읽지 못했습니다: ${errorMessage(error)}`;
    }

    // 음성 모델을 먼저 받아야 하는 환경이면, 받기 전에는 안내와 내려받기 버튼을 보여준다.
    const setup = platform.ttsSetup();
    const showIntroState = (): void => {
      introSetup.hidden = introReady;
      introControls.hidden = !introReady;
      introRemove.hidden = !introReady || setup === null;
    };
    introReady = await platform.ttsInstalled().catch(() => false);
    if (setup) {
      introSetupText.textContent =
        `음성 모델 ${setup.modelName}(약 ${setup.sizeMB}MB)을 ${setup.source}에서 한 번 내려받으면 ` +
        "이 브라우저 안에서 안내 음성을 만들 수 있습니다. 내려받을 때 이 기기의 접속 정보가 그 사이트에 전달됩니다. " +
        "안내 문구와 만든 음성은 기기 밖으로 나가지 않습니다.";
      introSetupTerms.replaceChildren(...setup.terms.map((term) => el("li", {}, term)));
      introSetupLicense.replaceChildren(
        `모델에는 ${setup.licenseName} 사용 조건이 적용됩니다. 내려받으면 이 조건에 동의하는 것으로 봅니다. `,
        el("a", { href: setup.licenseUrl, target: "_blank", rel: "noopener" }, "조건 전문 보기"),
      );
      introInstall.textContent = `음성 모델 내려받기 (약 ${setup.sizeMB}MB)`;

      introInstall.addEventListener("click", async () => {
        introInstall.disabled = true;
        introInstallProgress.hidden = false;
        introInstallProgress.value = 0;
        introInstallStatus.className = "status";
        introInstallStatus.textContent = "내려받는 중… 0%";
        try {
          await platform.installTts((percent) => {
            introInstallProgress.value = percent;
            introInstallStatus.textContent = `내려받는 중… ${percent}%`;
          });
          introReady = true;
          introInstallStatus.textContent = "";
          showIntroState();
        } catch (error) {
          introInstallStatus.className = "status error";
          introInstallStatus.textContent =
            `${errorMessage(error)} 다시 누르면 받은 데까지는 건너뛰고 이어서 받습니다.`;
        } finally {
          introInstall.disabled = false;
          introInstallProgress.hidden = true;
        }
      });

      introRemove.addEventListener("click", async () => {
        introRemove.disabled = true;
        try {
          await platform.removeTts();
          introReady = false;
          showIntroState();
        } finally {
          introRemove.disabled = false;
        }
      });
    }
    showIntroState();
  } else {
    ttsNote.hidden = false;
    ttsNote.replaceChildren(
      "시작 안내 음성(TTS)은 이 환경에서 넣을 수 없습니다. ",
      el("a", { href: LOCAL_APP_URL, target: "_blank", rel: "noopener" }, "로컬앱"),
      "에서 지원합니다.",
    );
  }

  const selectedIntro = (): IntroOptions | null => {
    if (!capabilities.ttsIntro || !introReady) return null;
    const template = introTemplate.value.trim();
    if (!template) return null;
    return { template, voiceId: introVoice.value || undefined };
  };

  musicInput.addEventListener("change", refreshMusicState);
  musicUrl.addEventListener("input", refreshMusicState);
  musicClear.addEventListener("click", () => {
    musicInput.value = "";
    refreshMusicState();
  });

  let track: GeneratedTrack | null = null;
  let playerUrl: string | null = null;

  const setBusy = (busy: boolean): void => {
    for (const control of [generateButton, resetButton, musicInput, musicClear, musicUrl]) {
      control.disabled = busy;
    }
    progress.hidden = !busy;
    form.setAttribute("aria-busy", String(busy));
  };

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const params = readParams(form, info);
    if (!params) return;
    const format = formatSelect.value;
    saveSettings({
      params,
      format,
      introTemplate: capabilities.ttsIntro ? introTemplate.value : saved.introTemplate,
      introVoiceId: capabilities.ttsIntro ? introVoice.value : saved.introVoiceId,
    });

    setBusy(true);
    progress.value = 0;
    status.className = "status";
    status.textContent = "음원을 만드는 중입니다…";
    const startedAt = performance.now();
    try {
      track = await platform.generate(
        { params, format, music: selectedMusic(), intro: selectedIntro() },
        (update) => {
          progress.value = update.percent;
          status.textContent = `${update.stage}… ${update.percent}%`;
        },
      );
      if (playerUrl) URL.revokeObjectURL(playerUrl);
      playerUrl = URL.createObjectURL(new Blob([track.bytes], { type: track.mime }));
      player.src = playerUrl;
      resultName.textContent = `${track.fileName} (${formatBytes(track.bytes.length)})`;
      renderReport(reportBox, track);
      result.hidden = false;
      const seconds = ((performance.now() - startedAt) / 1000).toFixed(1);
      status.textContent = `완료했습니다 (${seconds}초).`;
    } catch (error) {
      status.className = "status error";
      status.textContent = errorMessage(error);
    } finally {
      setBusy(false);
    }
  });

  downloadButton.addEventListener("click", async () => {
    if (!track) return;
    try {
      if (await platform.save(track)) {
        status.className = "status";
        status.textContent = "저장했습니다.";
      }
    } catch (error) {
      status.className = "status error";
      status.textContent = errorMessage(error);
    }
  });

  resetButton.addEventListener("click", () => {
    render({}, "");
    if (capabilities.ttsIntro) {
      introTemplate.value = info.intro.default_template;
      introVoice.value = "";
    }
    status.className = "status";
    status.textContent = "기본값으로 되돌렸습니다.";
  });

  generateButton.disabled = false;
}

function registerServiceWorker(platform: Platform): void {
  // 개발 서버에서는 캐시가 수정 내용을 가리지 않도록 등록하지 않는다.
  if (!platform.capabilities.offlineCache || !import.meta.env.PROD) return;
  if (!("serviceWorker" in navigator)) return;
  const register = (): void => {
    // 빌드 태그가 바뀌면 주소가 달라져 브라우저가 새 서비스 워커를 설치한다.
    navigator.serviceWorker.register(`./sw.js?v=${BUILD_TAG}`).catch(() => {
      // 오프라인 캐시는 부가 기능이라 실패해도 앱은 그대로 동작한다.
    });
  };
  // 플랫폼을 고르는 동안 페이지 로딩이 이미 끝났을 수 있다. 그때는 바로 등록한다.
  if (document.readyState === "complete") register();
  else window.addEventListener("load", register, { once: true });
}

/**
 * 실행 환경에 맞는 플랫폼 구현을 고른다.
 * 환경을 직접 확인하는 곳은 여기 한 군데뿐이다. 화면 코드는 `capabilities` 만 본다.
 */
async function createPlatform(): Promise<Platform> {
  if ("__TAURI_INTERNALS__" in window) {
    const { DesktopPlatform } = await import("./platform-desktop");
    return new DesktopPlatform();
  }
  const { WebPlatform } = await import("./platform-web");
  return new WebPlatform();
}

void createPlatform().then((platform) => {
  registerServiceWorker(platform);
  return start(platform);
});
