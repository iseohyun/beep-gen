// 화면(UI). 웹과 로컬앱이 이 코드를 그대로 함께 쓴다.
//
// 입력 폼은 코어가 알려주는 파라미터 명세로 만든다. 그래서 코어에 파라미터를
// 추가하면 화면에도 자동으로 나타나고, 이름·기본값·범위가 어긋날 수 없다.

import "./styles/app.css";
import type {
  AppInfo,
  GeneratedTrack,
  IntroOptions,
  LevelGuide,
  MusicReport,
  MusicSource,
  ParamSpec,
  ParamValues,
  Platform,
  SaveTarget,
  SaveWhere,
  SongGuess,
  UpdateInfo,
} from "./platform";
import { APP_VERSION, BUILD_TAG, FULL_VERSION } from "./version";

const STORAGE_KEY = "beep-gen.settings.v2";
/** 이전 저장 형식. 음량 기본값이 바뀌기 전의 값이 들어 있다. */
const OLD_STORAGE_KEY = "beep-gen.settings.v1";
/** 기본값이 바뀌어, 이전에 저장된 값을 이어받지 않는 항목. */
const RESET_ON_UPGRADE = ["beep-volume", "music-volume"];
const LOCAL_APP_URL = "https://github.com/iseohyun/beep-gen";

type SavedSettings = {
  params: ParamValues;
  format: string;
  introTemplate?: string;
  introVoiceId?: string;
  /** 결과 파일의 이름 형식. */
  nameFormat?: string;
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
    const current = localStorage.getItem(STORAGE_KEY);
    if (current) return JSON.parse(current) as Partial<SavedSettings>;
    // 이전 형식으로 저장된 설정은 이어받되, 기본값이 바뀐 음량은 새 기본값을 쓰게 한다.
    // (이전 기본값은 비프가 너무 크고 음악이 작았다. 저장된 값이 대부분 그 기본값이다.)
    const old = JSON.parse(localStorage.getItem(OLD_STORAGE_KEY) ?? "{}") as Partial<SavedSettings>;
    for (const key of RESET_ON_UPGRADE) delete old.params?.[key];
    return old;
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

/** 화면 상태(접어 둔 구역, 저장 위치). 음원 설정과 따로 둔다. */
const UI_KEY = "beep-gen.ui.v1";
/** 새 빌드가 있는지 다시 확인하는 간격(밀리초). */
const UPDATE_CHECK_MS = 30_000;
type UiState = {
  /** 접어 둔 구역의 id. */
  closed?: string[];
  /** 고른 저장 위치 항목. */
  saveWhere?: string;
  /** 지난번에 저장한 폴더. */
  lastFolder?: string;
};

function loadUi(): UiState {
  try {
    return JSON.parse(localStorage.getItem(UI_KEY) ?? "{}") as UiState;
  } catch {
    return {};
  }
}

function saveUi(state: UiState): void {
  try {
    localStorage.setItem(UI_KEY, JSON.stringify(state));
  } catch {
    // 저장소를 쓸 수 없어도 동작에는 지장이 없다.
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

// ── 소리 크기 슬라이더 ──────────────────────────────────
// 소리 크기 항목(%)은 dB 슬라이더와 dB 입력 칸으로 보여준다. 슬라이더의 값은 "맨 위(100%)에서
// 몇 dB 아래인지" 이고, 맨 아래까지 내리면 끔(0%)이다. 코어에는 여전히 % 로 보낸다.

/** 슬라이더의 눈금 간격(dB). 자석 지점과 직접 적은 값을 그대로 담을 수 있게 촘촘히 둔다. */
const LEVEL_STEP_DB = 0.01;
/** 방향키 한 번에 움직이는 크기(dB). */
const LEVEL_KEY_STEP_DB = 0.5;
/** 끌어서 움직일 때 자석 지점에 달라붙는 거리(dB). */
const MAGNET_REACH_DB = 1.0;
/** 이 거리 안이면 자석 지점 위에 있는 것으로 본다(dB). */
const MAGNET_ON_DB = 0.06;

/** 슬라이더가 가리키는 크기(dB). 끔이면 `null`. */
function levelDb(slider: HTMLInputElement): number | null {
  if (slider.value === slider.min) return null;
  return Number(slider.dataset.topDb) + Number(slider.value);
}

/** 슬라이더를 `db` 크기로 옮긴다(범위를 벗어나면 끝으로). */
function setLevelDb(slider: HTMLInputElement, db: number): void {
  slider.value = String(db - Number(slider.dataset.topDb));
  showLevel(slider);
}

/** 슬라이더의 값(dB)을 코어에 보낼 % 로 바꾼다. */
function levelPercent(slider: HTMLInputElement): number {
  return slider.value === slider.min ? 0 : 100 * 10 ** (Number(slider.value) / 20);
}

/** 슬라이더 옆의 dB 입력 칸에 지금 크기를 적는다. */
function showLevel(slider: HTMLInputElement): void {
  const db = levelDb(slider);
  const box = slider.closest(".cell-value")?.querySelector<HTMLInputElement>(".level-number");
  if (box) box.value = db === null ? "" : db.toFixed(1);
  slider.setAttribute("aria-valuetext", db === null ? "끔" : `${db.toFixed(1)} dB`);
}

/** 한 줄: 왼쪽 제목, 가운데 값, 오른쪽 단위. */
function createRow(
  label: string,
  inputId: string,
  description: string,
  value: (Node | string)[],
  unit: string,
): HTMLElement {
  const title = el("label", { htmlFor: inputId, title: description }, label);
  return el(
    "div",
    { className: "row" },
    title,
    el("div", { className: "cell-value" }, ...value),
    el("span", { className: "cell-unit" }, unit),
  );
}

/**
 * 소리 크기 항목의 줄을 만든다: 자석 점 + 슬라이더 + dB 입력 칸, 그리고 그 아래의 설명 줄.
 * 설명 줄은 자석 지점 위에 있을 때만 보인다.
 */
function createLevelRows(spec: ParamSpec, initial: number): HTMLElement[] {
  const level = spec.level!;
  const inputId = `param-${spec.key}`;
  // 맨 아래 눈금이 눈금 간격에 맞도록 범위를 정한다.
  const span = Math.ceil(level.top_db - level.floor_db);
  const slider = el("input", {
    id: inputId,
    type: "range",
    min: String(-span),
    // 0 은 100%(top_db)이고, 그 위로는 더 키우는 범위다.
    max: (level.ceil_db - level.top_db).toFixed(2),
    step: String(LEVEL_STEP_DB),
    title: spec.description,
  });
  slider.dataset.key = spec.key;
  slider.dataset.topDb = String(level.top_db);
  slider.value = String(initial > 0 ? Math.max(-span, 20 * Math.log10(initial / 100)) : -span);

  const box = el("input", {
    type: "text",
    className: "level-number",
    inputMode: "decimal",
    placeholder: "끔",
    autocomplete: "off",
  });
  box.setAttribute("aria-label", `${spec.label} (dB)`);
  /** 입력 칸의 값을 슬라이더에 옮기고, 슬라이더가 바뀐 것처럼 알린다. */
  const commit = (db: number): void => {
    setLevelDb(slider, db);
    slider.dispatchEvent(new Event("input", { bubbles: true }));
  };
  box.addEventListener("change", () => {
    const db = Number(box.value.replace(",", ".").replace(/[−–]/g, "-"));
    if (box.value.trim() === "" || !Number.isFinite(db)) showLevel(slider);
    else commit(db);
  });
  // 방향키로는 0.5 dB 씩 움직인다(슬라이더의 눈금은 그보다 촘촘하다).
  const onArrow = (event: KeyboardEvent): void => {
    const up = event.key === "ArrowUp" || event.key === "ArrowRight";
    const down = event.key === "ArrowDown" || event.key === "ArrowLeft";
    // 글자 칸에서 좌우 방향키는 글자 사이를 옮겨 다니는 데 쓴다.
    const sideways = event.key === "ArrowLeft" || event.key === "ArrowRight";
    if ((!up && !down) || (sideways && event.target === box)) return;
    event.preventDefault();
    const now = levelDb(slider) ?? level.top_db - span;
    const next = Math.round((now + (up ? LEVEL_KEY_STEP_DB : -LEVEL_KEY_STEP_DB)) * 2) / 2;
    commit(next);
  };
  box.addEventListener("keydown", onArrow);
  slider.addEventListener("keydown", onArrow);

  const row = createRow(
    "음량",
    inputId,
    spec.description,
    [el("div", { className: "level-slider" }, el("div", { className: "level-dots" }), slider), box],
    "dB",
  );
  row.classList.add("level-row");
  row.dataset.when = spec.when;
  const note = el("p", { className: "row-note hint level-note", hidden: true });
  note.dataset.noteFor = spec.key;
  showLevel(slider);
  slider.addEventListener("input", () => showLevel(slider));
  return [row, note];
}

/** 파라미터 하나의 줄을 만든다. */
function createRows(spec: ParamSpec, initial: number): HTMLElement[] {
  if (spec.level) return createLevelRows(spec, initial);
  const inputId = `param-${spec.key}`;
  let control: HTMLInputElement | HTMLSelectElement;

  if (spec.kind === "choice") {
    control = el("select", { id: inputId });
    spec.choices.forEach((choice, index) => {
      const label = spec.choice_labels[index] ?? choice.toLocaleString("ko-KR");
      control.append(el("option", { value: String(choice) }, label));
    });
    control.value = String(spec.choices.includes(initial) ? initial : spec.default);
  } else {
    const input = el("input", {
      id: inputId,
      type: "number",
      inputMode: spec.kind === "integer" ? "numeric" : "decimal",
      max: String(spec.max),
      step: String(spec.kind === "integer" ? 1 : spec.step),
      required: true,
      value: String(initial),
    });
    if (spec.kind === "integer") {
      input.min = String(spec.min);
    } else {
      // 소수 항목은 화살표로는 증감 단위(예: 0.1)만큼 움직이되, 직접 적을 때는 어떤 값이든
      // 받는다(예: 1.25). 브라우저는 "기준값 + 단위의 배수" 만 받으므로, 기준값을 늘 지금 값에
      // 맞춘다. 기준값은 최솟값 속성이 있으면 그쪽을 따르기 때문에, 최솟값은 속성으로 두지 않고
      // 제출할 때 직접 검사한다(readParams).
      input.dataset.min = String(spec.min);
      const followValue = (): void => {
        input.setAttribute("value", input.value);
        input.setCustomValidity("");
      };
      followValue();
      input.addEventListener("input", followValue);
    }
    control = input;
  }
  control.dataset.key = spec.key;
  control.title = spec.description;

  const row = createRow(spec.label, inputId, spec.description, [control], spec.unit);
  row.dataset.when = spec.when;
  return [row];
}

/**
 * 배경 음악 유무에 따라 쓰이지 않는 항목을 숨긴다.
 * 숨긴 칸은 비활성화해서, 보이지 않는 칸의 값 때문에 제출이 막히는 일이 없게 한다.
 * 소리 크기 줄은 숨기지 않고 비활성화만 한다(배경음을 넣으면 쓸 수 있다는 것이 보이게).
 */
function applyMusicState(form: HTMLFormElement, hasMusic: boolean): void {
  for (const row of form.querySelectorAll<HTMLElement>("[data-when]")) {
    const when = row.dataset.when;
    const used = when === "always" || (when === "with-music") === hasMusic;
    if (!row.classList.contains("level-row")) row.hidden = !used;
    for (const control of row.querySelectorAll<HTMLInputElement | HTMLSelectElement>(
      "input, select",
    )) {
      control.disabled = !used;
    }
  }
}

/** 폼에서 현재 값을 읽는다. 범위를 벗어난 칸이 있으면 그 칸을 알려주고 null 을 돌려준다. */
function readParams(form: HTMLFormElement, info: AppInfo): ParamValues | null {
  for (const input of form.querySelectorAll<HTMLInputElement>("input[data-min]")) {
    const min = Number(input.dataset.min);
    const tooSmall = !input.disabled && input.value !== "" && Number(input.value) < min;
    input.setCustomValidity(tooSmall ? `값은 ${min} 이상이어야 합니다.` : "");
  }
  if (!form.reportValidity()) return null;
  const values: ParamValues = {};
  for (const spec of info.params) {
    const control = form.querySelector<HTMLInputElement | HTMLSelectElement>(
      `[data-key="${spec.key}"]`,
    );
    if (!control) continue;
    // 숨겨져 쓰이지 않는 항목도 값은 그대로 보낸다(코어가 범위를 검사한다).
    if (control instanceof HTMLInputElement && control.type === "range") {
      values[spec.key] = levelPercent(control);
      continue;
    }
    const value = Number(control.value);
    values[spec.key] = Number.isFinite(value) && control.value !== "" ? value : spec.default;
  }
  return values;
}

/** 가수와 곡을 어디서 알아냈는지 사용자에게 알리는 문구. */
function describeGuess(guess: SongGuess): string {
  const fallback = "비워 두면 원래 파일 이름을 씁니다.";
  switch (guess.by) {
    case "tags":
      return "음악 파일에 적힌 정보에서 찾았습니다. 틀리면 고쳐 주세요.";
    case "file-name":
      return "파일 이름에서 찾았습니다. 틀리면 고쳐 주세요.";
    case "ai":
      return `로컬 AI(${guess.model ?? "Ollama"})가 파일 이름에서 추정했습니다. 틀릴 수 있으니 확인해 주세요.`;
    default:
      return guess.problem
        ? `${guess.problem} 가수와 곡명을 직접 적어 주세요. ${fallback}`
        : `가수와 곡명을 찾지 못했습니다. 직접 적어 주세요. ${fallback}`;
  }
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
      [
        report.triple_meter ? "맥박 하나의 길이" : "한 박의 길이",
        `${report.target_beat_sec.toFixed(3)}초`,
      ],
      [
        "곡의 템포",
        report.steady_tempo
          ? "끝까지 일정 (고정 격자에 맞춤)"
          : `흔들림 최대 ${report.tempo_drift_pct.toFixed(1)}% (박마다 따라가며 맞춤)`,
      ],
      ...(report.triple_meter
        ? ([["박자", "3박 계열. 비프 세 개가 한 마디"]] as [string, string][])
        : []),
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

/** 파일 위치에서 폴더 부분만. */
function folderOf(path: string): string {
  return path.replace(/[\\/][^\\/]*$/, "");
}

/** 파일 이름에서 확장자를 뗀 부분. */
function stemOf(name: string): string {
  return name.replace(/\.[^.]+$/, "");
}

/** 항목(파라미터)을 어느 구역에 둘지. 여기에 없는 항목은 비프 구역(고급 항목은 그 안의 고급)에 둔다. */
const PARAM_PLACE: Record<string, string> = {
  meter: "music-tempo-rows",
  "music-volume": "music-level-rows",
  "sample-rate": "generate-advanced-rows",
};

async function start(platform: Platform): Promise<void> {
  const form = byId<HTMLFormElement>("settings");
  const controls = byId<HTMLFieldSetElement>("controls");
  const formatSelect = byId<HTMLSelectElement>("format");
  const musicSection = byId("sec-music");
  const musicChoose = byId("music-choose");
  const musicRows = byId("music-rows");
  const musicInput = byId<HTMLInputElement>("music");
  const musicPick = byId<HTMLButtonElement>("music-pick");
  const musicUrlOpen = byId<HTMLButtonElement>("music-url-open");
  const musicUrlRow = byId("music-url-row");
  const musicUrl = byId<HTMLInputElement>("music-url");
  const musicUrlOk = byId<HTMLButtonElement>("music-url-ok");
  const musicName = byId<HTMLInputElement>("music-name");
  const musicClear = byId<HTMLButtonElement>("music-clear");
  const songArtist = byId<HTMLInputElement>("song-artist");
  const songTitle = byId<HTMLInputElement>("song-title");
  const songStatus = byId("song-status");
  const songComposer = byId<HTMLInputElement>("song-composer");
  const songAi = byId<HTMLInputElement>("song-ai");
  const songRename = byId<HTMLInputElement>("song-rename");
  const songLink = byId<HTMLInputElement>("song-link");
  const songSave = byId<HTMLButtonElement>("song-save");
  const introSection = byId("sec-intro");
  const introPreset = byId<HTMLSelectElement>("intro-preset");
  const introCustomRow = byId("intro-custom-row");
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
  const nameFormat = byId<HTMLSelectElement>("name-format");
  const nameFormatCustomRow = byId("name-format-custom-row");
  const nameFormatCustom = byId<HTMLInputElement>("name-format-custom");
  const nameFormatHint = byId("name-format-hint");
  const saveWhereRow = byId("save-where-row");
  const saveWhere = byId<HTMLSelectElement>("save-where");
  const previewButton = byId<HTMLButtonElement>("preview");
  const previewFrom = byId<HTMLSelectElement>("preview-from");
  const previewPlayer = byId<HTMLAudioElement>("preview-player");
  const resetButton = byId<HTMLButtonElement>("reset");
  const progress = byId<HTMLProgressElement>("progress");
  const status = byId("status");
  const result = byId("result");
  const player = byId<HTMLAudioElement>("player");
  const resultName = byId("result-name");
  const reportBox = byId("music-report");
  const downloadButton = byId<HTMLButtonElement>("download");
  const showFolderButton = byId<HTMLButtonElement>("show-folder");
  const ttsNote = byId("tts-note");
  const capabilities = platform.capabilities;

  // 지금 보는 화면이 어느 빌드인지: 화면 위쪽과 제목표시줄에 같은 표기를 보여준다.
  byId("version").textContent = FULL_VERSION;
  platform.setTitle(`Beep Gen ${FULL_VERSION}`);

  const showError = (error: unknown): void => {
    status.className = "status error";
    status.textContent = errorMessage(error);
  };

  let info: AppInfo;
  try {
    info = await platform.describe();
  } catch (error) {
    status.textContent = `시작하지 못했습니다: ${errorMessage(error)}`;
    status.className = "status error";
    return;
  }

  // 구역을 접어 둔 상태를 기억한다.
  const ui = loadUi();
  for (const section of document.querySelectorAll<HTMLDetailsElement>("details.section")) {
    if (ui.closed?.includes(section.id)) section.open = false;
    section.addEventListener("toggle", () => {
      ui.closed = [...document.querySelectorAll<HTMLDetailsElement>("details.section")]
        .filter((each) => !each.open)
        .map((each) => each.id);
      saveUi(ui);
    });
  }
  // 글자 칸에서 Enter 를 눌러 음원이 만들어지는 일을 막는다(만들기는 버튼으로만).
  form.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && event.target instanceof HTMLInputElement) event.preventDefault();
  });

  // ── 배경음 고르기 ─────────────────────────────────────────
  /** 파일 고르기 창으로 고른 음악(그 기능이 있는 환경에서만). */
  let pickedMusic: MusicSource | null = null;
  /** 주소로 넣은 음악. */
  let urlMusic = "";
  /** 지금 고른 배경 음악. */
  const selectedMusic = (): MusicSource | null => {
    if (pickedMusic) return pickedMusic;
    const file = musicInput.files?.[0];
    if (file) return { kind: "file", file, name: file.name };
    return urlMusic ? { kind: "url", url: urlMusic } : null;
  };
  /** 음악 파일의 위치를 알면 그 폴더. */
  const musicFolder = (): string | null =>
    pickedMusic?.kind === "local" ? folderOf(pickedMusic.path) : null;

  // ── 박자와 템포 ──────────────────────────────────────────
  // 곡을 불러오면 박자 계열(2박/3박)과 템포를 분석해 보여준다. 분석이 틀렸으면 직접 고친다.
  // 저장 아이콘을 누르면 음악 파일에 적어 두고, 다음에 불러올 때 그 값을 쓴다.
  const tempoNote = byId("tempo-note");
  /** 음악 파일에 적혀 있던 박자 계열과 템포. */
  let tagMeter: number | null = null;
  let tagBpm: number | null = null;
  /** 곡을 분석하는 중인지. */
  let measuring = false;
  const meterSelect = (): HTMLSelectElement | null =>
    form.querySelector<HTMLSelectElement>('select[data-key="meter"]');
  /** 음원을 만들 때 쓰일 박자 계열(2 또는 3). 아직 모르면 `null`. */
  const effectiveMeter = (): number | null => {
    const chosen = Number(meterSelect()?.value ?? 0);
    return chosen || levelGuide?.tempo?.meter || tagMeter;
  };
  /** 그 박자 계열에서의 템포(BPM). */
  const effectiveBpm = (): number | null => {
    const tempo = levelGuide?.tempo;
    const meter = effectiveMeter();
    return tempo && meter ? tempo.pulse_bpm / meter : tagBpm;
  };
  const refreshTempo = (): void => {
    const select = meterSelect();
    const tempo = levelGuide?.tempo ?? null;
    const hasMusic = selectedMusic() !== null;
    const meter = effectiveMeter();
    if (select?.options[0]) {
      const detected = tempo ? ` (${tempo.meter}박 계열)` : measuring ? " (분석 중…)" : "";
      select.options[0].textContent = `자동${detected}`;
    }
    const bpm = effectiveBpm();
    const parts: string[] = [];
    if (measuring) parts.push("박자와 템포를 분석하는 중…");
    else if (bpm !== null) {
      parts.push(`템포 ${bpm.toFixed(1)} BPM`);
      if (tempo && meter === 3) parts.push(`맥박 ${tempo.pulse_bpm.toFixed(0)} BPM`);
      if (tempo) parts.push(tempo.steady ? "템포 일정" : "템포가 흔들리는 곡");
      else parts.push("파일에 적힌 값");
      if (tempo && meter !== tempo.meter) parts.push(`분석은 ${tempo.meter}박 계열`);
    } else if (hasMusic) parts.push("박자를 찾지 못했습니다.");
    tempoNote.textContent = parts.join(" · ");
    tempoNote.hidden = !hasMusic || tempoNote.textContent === "";
    // 3박 패턴은 3박 계열 곡에서만 쓰인다.
    const accent = form.querySelector<HTMLSelectElement>('select[data-key="triple-accent"]');
    const accentRow = accent?.closest<HTMLElement>(".row");
    if (accent && accentRow) {
      const used = hasMusic && meter === 3;
      accentRow.hidden = !used;
      accent.disabled = !used;
    }
  };

  const refreshMusicState = (): void => {
    const music = selectedMusic();
    const hasMusic = music !== null;
    musicChoose.hidden = hasMusic;
    musicRows.hidden = !hasMusic;
    musicName.value = music ? (music.kind === "url" ? music.url : music.name) : "";
    musicName.title = music?.kind === "local" ? music.path : musicName.value;
    // 곡 정보는 위치를 아는 파일에만 저장할 수 있다.
    const canTag = capabilities.songTags && music?.kind === "local";
    for (const row of musicRows.querySelectorAll<HTMLElement>(".tag-only")) row.hidden = !canTag;
    songStatus.hidden = !hasMusic;
    // 비프만 있는 음원은 어디를 들어도 같으므로 위치를 고르지 않는다.
    previewFrom.disabled = !hasMusic;
    applyMusicState(form, hasMusic);
    refreshTempo();
    refreshSamples();
    refreshWhere();
  };

  // 음악을 고르면 가수와 곡명을 찾아 칸에 채운다. 결과 파일의 이름에 쓰인다.
  /** 지금 진행 중인 찾기. 음원을 만들기 전에 끝나기를 기다린다. */
  let identifying: Promise<void> = Promise.resolve();
  /** 음악을 바꿀 때마다 올라간다. 늦게 도착한 이전 음악의 결과를 버리는 데 쓴다. */
  let identifyRun = 0;
  /** 찾는 동안 사용자가 칸을 직접 고쳤는지. 고쳤으면 덮어쓰지 않는다. */
  let songEdited = false;
  /** 음악 파일에 AI 활용 여부가 적혀 있었는지. 적혀 있지 않았고 체크도 안 했으면 적지 않는다. */
  let aiKnown = false;
  const identifySelected = (): void => {
    const music = selectedMusic();
    const run = ++identifyRun;
    songEdited = false;
    aiKnown = false;
    tagMeter = null;
    tagBpm = null;
    // 박자는 곡마다 다르므로 새 곡은 자동에서 시작한다.
    const meter = meterSelect();
    if (meter) meter.value = "0";
    songArtist.value = "";
    songTitle.value = "";
    songComposer.value = "";
    songLink.value = "";
    songAi.checked = false;
    songStatus.className = "row-note hint";
    refreshSamples();
    if (!music) {
      identifying = Promise.resolve();
      return;
    }
    songStatus.textContent = "가수와 곡명을 찾는 중…";
    identifying = platform
      .identifyMusic(music)
      .catch((error: unknown): SongGuess => {
        return { artist: null, title: null, by: null, problem: errorMessage(error) };
      })
      .then((guess) => {
        if (run !== identifyRun) return;
        // 지난번에 적어 둔 박자 계열이 있으면 그것을 쓴다(사용자가 고친 값일 수 있다).
        tagMeter = guess.meter ?? null;
        tagBpm = guess.bpm ?? null;
        const meter = meterSelect();
        if (meter && tagMeter && meter.value === "0") meter.value = String(tagMeter);
        refreshTempo();
        if (songEdited) {
          songStatus.textContent = "비워 두면 원래 파일 이름을 씁니다.";
          return;
        }
        songArtist.value = guess.artist ?? "";
        songTitle.value = guess.title ?? "";
        songComposer.value = guess.composer ?? "";
        // 주소로 받은 음악이면 그 주소가 곧 출처다.
        songLink.value = guess.source_url ?? "";
        aiKnown = guess.ai_generated != null;
        songAi.checked = guess.ai_generated === true;
        songStatus.textContent = describeGuess(guess);
        refreshSamples();
      });
  };
  for (const input of [songArtist, songTitle]) {
    input.addEventListener("input", () => {
      songEdited = true;
      refreshSamples();
    });
  }

  const saved = loadSettings();
  const render = (values: ParamValues, format: string): void => {
    const places = [
      "beep-rows",
      "beep-advanced-rows",
      "music-tempo-rows",
      "music-level-rows",
      "generate-advanced-rows",
    ].map((id) => byId(id));
    for (const place of places) place.replaceChildren();
    for (const spec of info.params) {
      const place = PARAM_PLACE[spec.key] ?? (spec.advanced ? "beep-advanced-rows" : "beep-rows");
      // 박자는 곡마다 다르므로 지난번 값을 이어받지 않는다(곡의 분석 결과나 파일에 적힌 값을 쓴다).
      const initial = spec.key === "meter" ? spec.default : (values[spec.key] ?? spec.default);
      byId(place).append(...createRows(spec, initial));
    }
    meterSelect()?.addEventListener("change", () => refreshTempo());
    // 들어갈 항목이 없는 "고급" 은 보이지 않게 한다.
    for (const sub of document.querySelectorAll<HTMLElement>("details.sub")) {
      sub.hidden = sub.querySelector(".rows")?.childElementCount === 0;
    }
    formatSelect.replaceChildren(
      ...info.formats.map((f) => el("option", { value: f.id }, f.label)),
    );
    formatSelect.value = info.formats.some((f) => f.id === format) ? format : info.formats[0].id;
    // 주기나 분할 수를 바꾸면 그 값이 들어가는 선택지의 표시도 따라 바뀐다.
    for (const key of ["interval", "subdivision"]) {
      form.querySelector(`[data-key="${key}"]`)?.addEventListener("input", () => refreshSamples());
    }
    refreshMusicState();
    bindLevels();
  };

  // ── 음량 슬라이더와 자석 지점 ─────────────────────────────
  // 음악을 고르면 곡의 크기(최대·평균·최빈)를 재서, 비프 음량 슬라이더 위에 그 세 지점을 점으로
  // 표시한다. 점을 누르거나 그 자리로 끌어다 놓으면 아래에 그 지점의 설명이 나온다.
  /** 지금 고른 음악의 음량 안내. 재지 못했거나 음악이 없으면 `null`. */
  let levelGuide: LevelGuide | null = null;
  /** 음악을 고른 뒤 사용자가 음량을 직접 바꿨는지. 바꿨으면 자동으로 맞추지 않는다. */
  let levelsTouched = false;
  const levelSlider = (key: string): HTMLInputElement | null =>
    form.querySelector<HTMLInputElement>(`input[type="range"][data-key="${key}"]`);

  /** 비프 슬라이더가 달라붙을 지점들(dB). 음악 음량을 낮추면 그만큼 함께 내려간다. */
  const beepMagnets = (): { label: string; description: string; db: number }[] => {
    const beep = levelSlider("beep-volume");
    const music = levelSlider("music-volume");
    if (!levelGuide || !beep || !music) return [];
    const ceiling = Number(beep.dataset.topDb) + Number(beep.max);
    const musicOffset = levelDb(music) ?? Number(music.dataset.topDb) + Number(music.min);
    return levelGuide.magnets.map((magnet) => ({
      label: magnet.label,
      description: magnet.description,
      // 슬라이더의 범위를 넘는 지점은 맨 위에 둔다.
      db: Math.min(ceiling, magnet.db + musicOffset),
    }));
  };

  /** 자석 점을 다시 그리고, 지금 자석 지점 위에 있으면 그 설명을 보여준다. */
  const showMagnets = (note = ""): void => {
    const beep = levelSlider("beep-volume");
    const dots = beep?.parentElement?.querySelector<HTMLElement>(".level-dots");
    const noteRow = form.querySelector<HTMLElement>('[data-note-for="beep-volume"]');
    if (!beep || !dots || !noteRow) return;
    const top = Number(beep.dataset.topDb);
    const min = Number(beep.min);
    const now = levelDb(beep);
    let onMagnet = "";
    dots.replaceChildren(
      ...beepMagnets().map((magnet) => {
        const text = `${magnet.label} ${magnet.db.toFixed(1)} dB · ${magnet.description}`;
        const on = now !== null && Math.abs(now - magnet.db) <= MAGNET_ON_DB;
        if (on) onMagnet = text;
        const dot = el("button", {
          type: "button",
          className: on ? "level-dot on" : "level-dot",
          title: text,
        });
        dot.setAttribute("aria-label", text);
        dot.style.left = `${(100 * (magnet.db - top - min)) / (Number(beep.max) - min)}%`;
        dot.addEventListener("click", () => {
          levelsTouched = true;
          setLevelDb(beep, magnet.db);
          showMagnets();
        });
        return dot;
      }),
    );
    noteRow.textContent = note || onMagnet;
    noteRow.hidden = noteRow.textContent === "";
  };

  /** 초기화했을 때: 음악은 최대(0 dB)로, 비프는 곡의 평균 크기로 맞춘다. */
  const applyDefaultLevels = (): void => {
    const beep = levelSlider("beep-volume");
    const music = levelSlider("music-volume");
    if (!levelGuide || !beep || !music || levelsTouched) return;
    setLevelDb(music, Number(music.dataset.topDb));
    setLevelDb(beep, levelGuide.default_beep_db);
  };

  /** 입력 칸을 새로 그린 뒤 슬라이더에 동작을 붙인다. */
  function bindLevels(): void {
    const beep = levelSlider("beep-volume");
    const music = levelSlider("music-volume");
    if (!beep || !music) return;
    // 끌어서 움직일 때만 달라붙는다. 방향키나 숫자 입력으로는 자유롭게 움직인다.
    let dragging = false;
    beep.addEventListener("pointerdown", () => (dragging = true));
    beep.addEventListener("pointerup", () => (dragging = false));
    beep.addEventListener("pointercancel", () => (dragging = false));
    beep.addEventListener("input", () => {
      levelsTouched = true;
      const db = levelDb(beep);
      if (dragging && db !== null) {
        const near = beepMagnets()
          .map((magnet) => magnet.db)
          .sort((a, b) => Math.abs(a - db) - Math.abs(b - db))[0];
        if (near !== undefined && Math.abs(near - db) <= MAGNET_REACH_DB) setLevelDb(beep, near);
      }
      showMagnets();
    });
    music.addEventListener("input", () => {
      levelsTouched = true;
      showMagnets();
    });
    showMagnets();
  }

  let levelRun = 0;
  const measureSelected = (): void => {
    const music = selectedMusic();
    const run = ++levelRun;
    levelGuide = null;
    measuring = music !== null;
    refreshTempo();
    if (!music) {
      showMagnets();
      return;
    }
    showMagnets("곡의 크기를 재는 중…");
    void platform
      .musicLevels(music)
      .catch(() => null)
      .then((guide) => {
        if (run !== levelRun) return;
        levelGuide = guide;
        measuring = false;
        // 음량은 마지막에 쓴 값을 그대로 둔다. 곡에 맞춘 값은 자석 점이나 초기화로 고른다.
        showMagnets();
        refreshTempo();
      });
  };

  // ── 값이 들어간 모습으로 보여주는 선택지(파일명, 시작음 문구) ─────
  const CUSTOM = "custom";
  /** 지금 설정으로 `{n}`, `{m}` 을 채운다. */
  const fillNumbers = (template: string): string => {
    const value = (key: string, fallback: string): string => {
      const text = form.querySelector<HTMLInputElement>(`[data-key="${key}"]`)?.value ?? "";
      return text !== "" && Number.isFinite(Number(text)) ? String(Number(text)) : fallback;
    };
    return template.replaceAll("{n}", value("interval", "n")).replaceAll("{m}", value("subdivision", "m"));
  };
  /** 이름 형식이 지금 설정에서 어떤 이름이 되는지(확장자 제외). 실제 이름은 코어가 짓는다. */
  const sampleName = (template: string): string => {
    const music = selectedMusic();
    let artist = "";
    let title = "beep";
    if (music) {
      title = songTitle.value.trim();
      artist = title ? songArtist.value.trim() : "";
      if (!title) title = music.kind === "url" ? "곡" : stemOf(music.name);
    }
    let text = template.replaceAll("{artist}", "{가수}").replaceAll(/\{title\}|\{곡명\}/g, "{곡}");
    // 가수를 모르면 가수와 그 옆의 구분자도 함께 뺀다.
    if (!artist) text = text.replace(/\{가수\}\s*[-–—_,·]?\s*/g, "").replace(/\s*[-–—_,·]?\s*\{가수\}/g, "");
    return fillNumbers(text).replaceAll("{가수}", artist).replaceAll("{곡}", title).trim();
  };
  function refreshSamples(): void {
    for (const option of nameFormat.options) {
      if (option.value !== CUSTOM) option.textContent = sampleName(option.value);
    }
    for (const option of introPreset.options) {
      if (option.value !== CUSTOM && option.value !== "") option.textContent = fillNumbers(option.value);
    }
  }

  // 결과 파일의 이름 형식: 목록에서 고르거나 직접 적는다. 목록에는 형식 대신 결과물을 보여준다.
  nameFormat.replaceChildren(
    ...info.name_formats.map((template) => el("option", { value: template }, template)),
    el("option", { value: CUSTOM }, "직접 입력…"),
  );
  nameFormatHint.textContent = info.name_format_hint;
  const showNameFormat = (template: string): void => {
    const listed = info.name_formats.includes(template);
    nameFormat.value = listed ? template : CUSTOM;
    nameFormatCustom.value = template;
    nameFormatCustomRow.hidden = listed;
    nameFormatHint.hidden = listed;
  };
  const selectedNameFormat = (): string =>
    (nameFormat.value === CUSTOM ? nameFormatCustom.value.trim() : nameFormat.value) ||
    info.name_formats[0];
  showNameFormat(saved.nameFormat ?? info.name_formats[0]);
  nameFormat.addEventListener("change", () => {
    if (nameFormat.value === CUSTOM) {
      // 직접 적기 시작할 때는 방금 보던 형식에서 출발한다.
      nameFormatCustomRow.hidden = false;
      nameFormatHint.hidden = false;
      nameFormatCustom.focus();
    } else {
      showNameFormat(nameFormat.value);
    }
  });

  // 시작음 문구: 목록에서 고르거나 직접 적는다. "없음" 이면 안내 음성을 넣지 않는다.
  introPreset.replaceChildren(
    ...info.intro.templates.map((template) => el("option", { value: template }, template)),
    el("option", { value: "" }, "없음"),
    el("option", { value: CUSTOM }, "직접 입력…"),
  );
  introHint.textContent = info.intro.hint;
  const showIntroTemplate = (template: string): void => {
    const listed = template === "" || info.intro.templates.includes(template);
    introPreset.value = listed ? template : CUSTOM;
    introTemplate.value = template;
    introCustomRow.hidden = listed;
    introHint.hidden = listed;
  };
  const selectedIntroTemplate = (): string =>
    introPreset.value === CUSTOM ? introTemplate.value.trim() : introPreset.value;
  introPreset.addEventListener("change", () => {
    if (introPreset.value === CUSTOM) {
      introCustomRow.hidden = false;
      introHint.hidden = false;
      introTemplate.focus();
    } else {
      showIntroTemplate(introPreset.value);
    }
  });

  // ── 저장 위치 ─────────────────────────────────────────────
  const PICK_FOLDER = "pick";
  const LAST_FOLDER = "last";
  const DEFAULT_WHERE: SaveWhere = "music-output";
  /** 저장 위치 목록을 지금 상태에 맞게 다시 그린다. */
  function refreshWhere(): void {
    if (!capabilities.saveFolders) return;
    const folder = musicFolder();
    const unknown = " (배경음 위치를 모르면 다운로드)";
    const wanted = ui.saveWhere ?? DEFAULT_WHERE;
    saveWhere.replaceChildren(
      el("option", { value: "music-output" }, folder ? `배경음 위치/output — ${folder}\\output` : `배경음 위치/output${unknown}`),
      el("option", { value: "music-folder" }, folder ? `배경음 위치 — ${folder}` : `배경음 위치${unknown}`),
      ...(ui.lastFolder ? [el("option", { value: LAST_FOLDER }, `지난번 위치 — ${ui.lastFolder}`)] : []),
      el("option", { value: "downloads" }, "다운로드"),
      el("option", { value: "my-music" }, "내 음악"),
      el("option", { value: PICK_FOLDER }, "폴더 고르기…"),
    );
    saveWhere.value = wanted;
    if (saveWhere.selectedIndex < 0) saveWhere.value = DEFAULT_WHERE;
  }
  const saveTarget = (): SaveTarget =>
    saveWhere.value === LAST_FOLDER
      ? { where: "folder", folder: ui.lastFolder }
      : { where: saveWhere.value as SaveWhere };
  saveWhereRow.hidden = !capabilities.saveFolders;
  saveWhere.addEventListener("change", async () => {
    if (saveWhere.value === PICK_FOLDER) {
      try {
        const picked = await platform.pickFolder();
        if (picked) {
          ui.lastFolder = picked;
          ui.saveWhere = LAST_FOLDER;
        }
      } catch (error) {
        showError(error);
      }
    } else {
      ui.saveWhere = saveWhere.value;
    }
    saveUi(ui);
    refreshWhere();
  });
  if (capabilities.saveFolders) downloadButton.textContent = "다른 곳에 저장";

  render(saved.params ?? {}, saved.format ?? "");

  // 기능이 있는 환경에서만 해당 입력을 보여주고, 없으면 그 사실과 대안을 알려준다.
  musicSection.hidden = !capabilities.musicFile;
  musicUrlOpen.hidden = !capabilities.musicUrl;
  introSection.hidden = !capabilities.ttsIntro;
  /** 안내 음성을 지금 바로 만들 수 있는지(필요한 것을 받아 두었는지). */
  let introReady = false;
  if (capabilities.ttsIntro) {
    showIntroTemplate(saved.introTemplate ?? info.intro.default_template);
    refreshSamples();
    try {
      for (const voice of await platform.listVoices()) {
        introVoice.append(el("option", { value: voice.id }, `${voice.name} (${voice.language})`));
      }
      if (saved.introVoiceId) introVoice.value = saved.introVoiceId;
      // 저장해 둔 음성이 지금은 없으면 자동 선택으로 되돌린다.
      if (introVoice.selectedIndex < 0) introVoice.value = "";
    } catch (error) {
      introHint.hidden = false;
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
    const template = selectedIntroTemplate();
    if (!template) return null;
    return { template, voiceId: introVoice.value || undefined };
  };

  /** 배경음이 바뀌었을 때: 화면을 맞추고 곡 정보와 음량을 새로 구한다. */
  const musicChanged = (): void => {
    refreshMusicState();
    identifySelected();
    measureSelected();
  };
  musicInput.addEventListener("change", musicChanged);
  // 곡 정보를 원본에 저장할 수 있는 환경에서는 파일 고르기 창으로 고른다(파일의 위치를 알아야 한다).
  musicPick.addEventListener("click", async () => {
    if (!capabilities.songTags) {
      musicInput.click();
      return;
    }
    try {
      const picked = await platform.pickMusic();
      if (!picked) return;
      pickedMusic = picked;
      musicChanged();
    } catch (error) {
      showError(error);
    }
  });
  musicUrlOpen.addEventListener("click", () => {
    musicUrlRow.hidden = !musicUrlRow.hidden;
    if (!musicUrlRow.hidden) musicUrl.focus();
  });
  const acceptUrl = (): void => {
    const url = musicUrl.value.trim();
    if (!/^https?:\/\/\S+$/i.test(url)) {
      status.className = "status error";
      status.textContent = "http:// 또는 https:// 로 시작하는 오디오 파일 주소를 적어 주세요.";
      musicUrl.focus();
      return;
    }
    status.className = "status";
    status.textContent = "";
    urlMusic = url;
    musicChanged();
    // 주소로 받은 음악은 그 주소가 곧 출처다.
    songLink.value = url;
  };
  musicUrlOk.addEventListener("click", acceptUrl);
  musicUrl.addEventListener("keydown", (event) => {
    if (event.key === "Enter") acceptUrl();
  });
  musicClear.addEventListener("click", () => {
    musicInput.value = "";
    pickedMusic = null;
    urlMusic = "";
    musicUrl.value = "";
    musicUrlRow.hidden = true;
    musicChanged();
  });

  if (capabilities.songTags) {
    songAi.addEventListener("change", () => (aiKnown = true));
    songSave.addEventListener("click", async () => {
      songSave.disabled = true;
      songStatus.className = "row-note hint";
      songStatus.textContent = "음악 파일에 저장하는 중…";
      try {
        // 가수와 곡을 찾는 중이면 끝나기를 기다린다.
        await identifying;
        const title = songTitle.value.trim();
        if (!title) throw new Error("곡명을 적어야 저장할 수 있습니다.");
        const before = pickedMusic?.kind === "local" ? pickedMusic.name : "";
        pickedMusic = await platform.saveSongInfo(
          {
            artist: songArtist.value.trim(),
            title,
            composer: songComposer.value.trim(),
            sourceUrl: songLink.value.trim(),
            aiGenerated: songAi.checked ? true : aiKnown ? false : null,
            meter: effectiveMeter(),
            bpm: effectiveBpm(),
          },
          songRename.checked,
        );
        const after = pickedMusic.kind === "local" ? pickedMusic.name : "";
        refreshMusicState();
        songStatus.textContent =
          before === after
            ? `음악 파일에 저장했습니다: ${after}`
            : `음악 파일에 저장하고 이름을 바꿨습니다: ${before} → ${after}`;
      } catch (error) {
        songStatus.className = "row-note hint status error";
        songStatus.textContent = errorMessage(error);
      } finally {
        songSave.disabled = false;
      }
    });
  }

  let track: GeneratedTrack | null = null;
  let playerUrl: string | null = null;
  let previewUrl: string | null = null;

  byId("preview-label").textContent = `${info.preview.seconds}초`;
  previewButton.title = `지금 설정으로 ${info.preview.seconds}초만 만들어 들려줍니다.`;
  previewFrom.value = String(info.preview.from_percent);
  if (previewFrom.selectedIndex < 0) previewFrom.selectedIndex = 0;

  const setBusy = (busy: boolean): void => {
    // 구역 안의 모든 입력을 한꺼번에 잠근다. 풀면 각 칸은 원래 상태로 돌아간다.
    controls.disabled = busy;
    resetButton.disabled = busy;
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
      introTemplate: capabilities.ttsIntro ? selectedIntroTemplate() : saved.introTemplate,
      introVoiceId: capabilities.ttsIntro ? introVoice.value : saved.introVoiceId,
      nameFormat: selectedNameFormat(),
    });
    // 잠그기 전에 읽어 둔다(잠긴 동안에는 값이 바뀌지 않는다).
    const target = capabilities.saveFolders ? saveTarget() : null;

    setBusy(true);
    progress.value = 0;
    status.className = "status";
    status.textContent = "음원을 만드는 중입니다…";
    const startedAt = performance.now();
    try {
      // 가수와 곡을 찾는 중이면 끝나기를 기다린다(파일 이름에 쓰이므로).
      await identifying;
      const song = { artist: songArtist.value.trim(), title: songTitle.value.trim() };
      track = await platform.generate(
        {
          params,
          format,
          music: selectedMusic(),
          song,
          intro: selectedIntro(),
          nameFormat: selectedNameFormat(),
        },
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
      // 새로 만든 음원은 아직 저장하지 않았다.
      showFolderButton.hidden = true;
      const seconds = ((performance.now() - startedAt) / 1000).toFixed(1);
      if (!target) {
        status.textContent = `완료했습니다 (${seconds}초). 아래에서 듣고 저장하세요.`;
        return;
      }
      // 폴더를 정해 저장할 수 있는 환경에서는 만들자마자 그 폴더에 저장한다.
      const savedPath = await platform.saveTo(track, target);
      if (savedPath) {
        resultName.textContent = `${savedPath} (${formatBytes(track.bytes.length)})`;
        status.textContent = `완료했습니다 (${seconds}초). 저장: ${savedPath}`;
        ui.lastFolder = folderOf(savedPath);
        saveUi(ui);
        refreshWhere();
        showFolderButton.hidden = false;
      } else {
        status.textContent =
          `만들었지만 저장하지 않았습니다 (${seconds}초). 아래 "다른 곳에 저장" 으로 저장할 수 있습니다.`;
      }
    } catch (error) {
      showError(error);
    } finally {
      setBusy(false);
    }
  });

  // 미리듣기: 지금 설정으로 짧은 구간만 만들어 바로 들려준다. 결과 파일과는 따로다.
  previewButton.addEventListener("click", async () => {
    const params = readParams(form, info);
    if (!params) return;
    const fromPercent = Number(previewFrom.value);
    setBusy(true);
    progress.value = 0;
    status.className = "status";
    status.textContent = "미리듣기를 만드는 중입니다…";
    previewPlayer.pause();
    const startedAt = performance.now();
    try {
      const preview = await platform.generate(
        {
          params,
          // 바로 재생할 것이므로 압축하지 않는 형식이 빠르다.
          format: "wav",
          music: selectedMusic(),
          song: { artist: "", title: "" },
          intro: null,
          preview: { seconds: info.preview.seconds, from_percent: fromPercent },
        },
        (update) => {
          progress.value = update.percent;
          status.textContent = `${update.stage}… ${update.percent}%`;
        },
      );
      if (previewUrl) URL.revokeObjectURL(previewUrl);
      previewUrl = URL.createObjectURL(new Blob([preview.bytes], { type: preview.mime }));
      previewPlayer.src = previewUrl;
      previewPlayer.hidden = false;
      const seconds = ((performance.now() - startedAt) / 1000).toFixed(1);
      status.textContent = `미리듣기를 만들었습니다 (${seconds}초).`;
      // 자동 재생이 막힌 환경에서는 사용자가 재생 버튼을 누르면 된다.
      void previewPlayer.play().catch(() => {});
    } catch (error) {
      showError(error);
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
        showFolderButton.hidden = !capabilities.saveFolders;
      }
    } catch (error) {
      showError(error);
    }
  });

  // 업데이트: 새 개발 빌드나 릴리즈의 새 버전이 있으면 버튼이 켜진다. 받아서 바꾸는 것은 눌렀을 때만 한다.
  const updateButton = byId<HTMLButtonElement>("update");
  updateButton.hidden = !capabilities.updates;
  let foundUpdate: UpdateInfo | null = null;
  const describeUpdate = (found: UpdateInfo): string => {
    const when = found.built_at_ms === null ? "" : ` (${new Date(found.built_at_ms).toLocaleString("ko-KR")})`;
    return found.source === "release"
      ? `새 버전 ${found.version ?? ""} 이(가) 있습니다${when}. 지금 버전은 ${APP_VERSION} 입니다.
누르면 GitHub 릴리즈에서 받아 설치하고 다시 시작합니다.`
      : `새 개발 빌드가 있습니다${when}. 누르면 바꾸고 다시 시작합니다.
${found.path ?? ""}`;
  };
  const checkUpdate = async (): Promise<void> => {
    try {
      foundUpdate = await platform.checkUpdate();
      updateButton.title = foundUpdate ? describeUpdate(foundUpdate) : "새 버전이 없습니다.";
    } catch (error) {
      // 확인하지 못하면 버튼을 꺼 두고 이유를 풍선 도움말에 적는다.
      foundUpdate = null;
      updateButton.title = error instanceof Error ? error.message : String(error);
    }
    updateButton.disabled = !foundUpdate;
  };
  if (capabilities.updates) {
    void checkUpdate();
    // 창으로 돌아올 때와, 켜 둔 동안에는 30초마다 다시 본다.
    window.addEventListener("focus", () => void checkUpdate());
    setInterval(() => void checkUpdate(), UPDATE_CHECK_MS);
  }
  updateButton.addEventListener("click", async () => {
    const update = foundUpdate;
    if (!update) return;
    updateButton.disabled = true;
    status.className = "status";
    status.textContent =
      update.source === "release"
        ? `새 버전 ${update.version ?? ""} 을(를) 받아 설치하는 중입니다. 곧 다시 시작합니다…`
        : "새 빌드로 바꾸는 중입니다. 곧 다시 시작합니다…";
    try {
      await platform.applyUpdate(update);
    } catch (error) {
      showError(error);
      void checkUpdate();
    }
  });

  showFolderButton.addEventListener("click", async () => {
    try {
      await platform.showSavedFolder();
    } catch (error) {
      showError(error);
    }
  });

  resetButton.addEventListener("click", () => {
    // 기억해 둔 입력값도 지운다(다음에 켰을 때도 처음 값에서 시작한다).
    try {
      localStorage.removeItem(STORAGE_KEY);
    } catch {
      // 저장소를 쓸 수 없어도 화면의 값은 되돌린다.
    }
    render({}, "");
    // 박자는 파일에 적힌 값이 있으면 그것으로, 없으면 자동으로 되돌린다.
    const meter = meterSelect();
    if (meter && tagMeter) meter.value = String(tagMeter);
    refreshTempo();
    if (capabilities.ttsIntro) {
      showIntroTemplate(info.intro.default_template);
      introVoice.value = "";
    }
    showNameFormat(info.name_formats[0]);
    refreshSamples();
    // 음악이 있으면 음량도 곡에 맞춘 처음 값으로 되돌린다.
    levelsTouched = false;
    applyDefaultLevels();
    showMagnets();
    status.className = "status";
    status.textContent = "입력값을 처음 값으로 되돌렸습니다.";
  });

  generateButton.disabled = false;
  previewButton.disabled = false;
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
