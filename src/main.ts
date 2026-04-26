import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface Settings {
  microphone: string;
  engine: string;
  whisperModel: string;
  recordingMode: string;
  hotkey: string;
  language: string;
  textMode: string;
  transformProvider: string;
  transformBaseUrl: string;
  transformModel: string;
}

interface MicDevice {
  name: string;
  is_default: boolean;
}

interface SecretStatus {
  hasGroqApiKey: boolean;
  hasTransformApiKey: boolean;
}

interface ReadinessIssue {
  level: "error" | "warning";
  title: string;
  detail: string;
}

interface ReadinessStatus {
  canRecord: boolean;
  summary: string;
  issues: ReadinessIssue[];
}

interface DownloadProgress {
  downloaded: number;
  total: number;
  percent: number;
}

interface HistoryEntry {
  id: string;
  createdAtMs: number;
  engine: string;
  language: string;
  textMode: string;
  transformProvider: string;
  rawText: string;
  finalText: string;
  selectedText?: string | null;
}

// DOM elements
const statusDot = document.getElementById("status-dot")!;
const statusText = document.getElementById("status-text")!;
const readinessSummary = document.getElementById("readiness-summary")!;
const readinessList = document.getElementById("readiness-list")!;
const readinessRefreshBtn = document.getElementById(
  "readiness-refresh-btn",
) as HTMLButtonElement;
const historyList = document.getElementById("history-list")!;
const historyClearBtn = document.getElementById(
  "history-clear-btn",
) as HTMLButtonElement;
const micSelect = document.getElementById("mic-select") as HTMLSelectElement;
const engineLocal = document.getElementById("engine-local")!;
const engineCloud = document.getElementById("engine-cloud")!;
const localSettings = document.getElementById("local-settings")!;
const cloudSettings = document.getElementById("cloud-settings")!;
const modelSelect = document.getElementById("model-select") as HTMLSelectElement;
const downloadBtn = document.getElementById("download-btn")!;
const downloadProgress = document.getElementById("download-progress")!;
const progressFill = document.getElementById("progress-fill")!;
const groqKey = document.getElementById("groq-key") as HTMLInputElement;
const groqStoreBtn = document.getElementById("groq-store-btn") as HTMLButtonElement;
const groqClearBtn = document.getElementById("groq-clear-btn") as HTMLButtonElement;
const groqSecretStatus = document.getElementById("groq-secret-status")!;
const modeToggle = document.getElementById("mode-toggle")!;
const modePtt = document.getElementById("mode-ptt")!;
const hotkeyText = document.getElementById("hotkey-text")!;
const hotkeyRebindBtn = document.getElementById(
  "hotkey-rebind-btn",
) as HTMLButtonElement;
const hotkeyHintText = document.getElementById("hotkey-hint-text")!;
const languageSelect = document.getElementById("language-select") as HTMLSelectElement;
const textModeRaw = document.getElementById("text-mode-raw")!;
const textModePolished = document.getElementById("text-mode-polished")!;
const textModeCommand = document.getElementById("text-mode-command")!;
const transformProviderSelect = document.getElementById(
  "transform-provider-select",
) as HTMLSelectElement;
const transformSettings = document.getElementById("transform-settings")!;
const transformBaseUrl = document.getElementById(
  "transform-base-url",
) as HTMLInputElement;
const transformModel = document.getElementById(
  "transform-model",
) as HTMLInputElement;
const transformApiKey = document.getElementById(
  "transform-api-key",
) as HTMLInputElement;
const transformStoreBtn = document.getElementById(
  "transform-store-btn",
) as HTMLButtonElement;
const transformClearBtn = document.getElementById(
  "transform-clear-btn",
) as HTMLButtonElement;
const transformSecretStatus = document.getElementById(
  "transform-secret-status",
)!;

// Section navigation
const navItems = document.querySelectorAll(".nav-item");
const sections = document.querySelectorAll(".content-section");

navItems.forEach((item) => {
  item.addEventListener("click", () => {
    const target = item.getAttribute("data-section");
    navItems.forEach((n) => n.classList.remove("active"));
    sections.forEach((s) => s.classList.remove("active"));
    item.classList.add("active");
    document.getElementById(`section-${target}`)?.classList.add("active");
  });
});

// Window drag — titlebar and sidebar empty space
const titlebar = document.getElementById("titlebar")!;
const sidebar = document.getElementById("sidebar")!;
const appWindow = getCurrentWindow();

titlebar.addEventListener("mousedown", (e) => {
  if ((e.target as HTMLElement).closest("button, select, input, a, .nav-item")) return;
  appWindow.startDragging();
});

sidebar.addEventListener("mousedown", (e) => {
  if ((e.target as HTMLElement).closest("button, select, input, a, .nav-item")) return;
  appWindow.startDragging();
});

let currentSettings: Settings;
let isCapturingHotkey = false;

async function loadSettings() {
  currentSettings = await invoke<Settings>("get_settings");

  // Populate mic dropdown
  const mics = await invoke<MicDevice[]>("list_microphones");
  micSelect.innerHTML = "";
  mics.forEach((mic) => {
    const option = document.createElement("option");
    option.value = mic.name;
    option.textContent = mic.name + (mic.is_default ? " (default)" : "");
    micSelect.appendChild(option);
  });
  micSelect.value = currentSettings.microphone;

  // Engine
  setEngine(currentSettings.engine);

  // Model
  modelSelect.value = currentSettings.whisperModel;
  await checkModelStatus();

  // Recording mode
  setRecordingMode(currentSettings.recordingMode);

  // Language and text mode
  languageSelect.value = currentSettings.language;
  setTextMode(currentSettings.textMode);

  // Transform provider
  transformProviderSelect.value = currentSettings.transformProvider;
  transformBaseUrl.value = currentSettings.transformBaseUrl;
  transformModel.value = currentSettings.transformModel;
  updateTransformVisibility();
  await refreshSecretStatus();

  // Hotkey
  renderHotkey(currentSettings.hotkey);
  await refreshReadinessStatus();
  await loadHistoryEntries();
}

function setEngine(engine: string) {
  currentSettings.engine = engine;
  engineLocal.classList.toggle("active", engine === "local");
  engineCloud.classList.toggle("active", engine === "cloud");
  localSettings.classList.toggle("hidden", engine !== "local");
  cloudSettings.classList.toggle("hidden", engine !== "cloud");
}

function setRecordingMode(mode: string) {
  currentSettings.recordingMode = mode;
  modeToggle.classList.toggle("active", mode === "toggle");
  modePtt.classList.toggle("active", mode === "push-to-talk");
}

function setTextMode(mode: string) {
  currentSettings.textMode = mode;
  textModeRaw.classList.toggle("active", mode === "raw");
  textModePolished.classList.toggle("active", mode === "polished");
  textModeCommand.classList.toggle("active", mode === "command");
  updateTransformVisibility();
}

function updateTransformVisibility() {
  const showTransformSettings =
    (currentSettings.textMode === "polished" ||
      currentSettings.textMode === "command") &&
    currentSettings.transformProvider !== "none";
  transformSettings.classList.toggle("hidden", !showTransformSettings);
}

async function checkModelStatus() {
  const downloaded = await invoke<boolean>("check_model_downloaded", {
    modelSize: modelSelect.value,
  });
  downloadBtn.textContent = downloaded ? "\u2713" : "Download";
  (downloadBtn as HTMLButtonElement).disabled = downloaded;
}

async function refreshSecretStatus() {
  const status = await invoke<SecretStatus>("get_secret_status");

  groqSecretStatus.textContent = status.hasGroqApiKey
    ? "Stored securely in OS keychain"
    : "Not stored";
  groqClearBtn.disabled = !status.hasGroqApiKey;

  transformSecretStatus.textContent = status.hasTransformApiKey
    ? "Stored securely in OS keychain"
    : "Not stored";
  transformClearBtn.disabled = !status.hasTransformApiKey;
}

async function refreshReadinessStatus() {
  const readiness = await invoke<ReadinessStatus>("get_readiness_status");
  readinessSummary.textContent = readiness.summary;
  readinessSummary.className = readiness.canRecord
    ? "readiness-summary ready"
    : "readiness-summary blocked";

  readinessList.innerHTML = "";

  if (readiness.issues.length === 0) {
    const item = document.createElement("div");
    item.className = "readiness-item readiness-item-ok";
    item.innerHTML = `
      <div class="readiness-item-title">All core checks passed</div>
      <div class="readiness-item-detail">Hotkey, engine, and active mode configuration look good.</div>
    `;
    readinessList.appendChild(item);
    return;
  }

  readiness.issues.forEach((issue) => {
    const item = document.createElement("div");
    item.className = `readiness-item readiness-item-${issue.level}`;
    item.innerHTML = `
      <div class="readiness-item-title">${issue.title}</div>
      <div class="readiness-item-detail">${issue.detail}</div>
    `;
    readinessList.appendChild(item);
  });
}

async function saveSettings() {
  const nextSettings: Settings = {
    ...currentSettings,
    microphone: micSelect.value,
    whisperModel: modelSelect.value,
    language: languageSelect.value,
    transformProvider: transformProviderSelect.value,
    transformBaseUrl: transformBaseUrl.value,
    transformModel: transformModel.value,
  };

  await invoke("save_settings", { settings: nextSettings });
  currentSettings = nextSettings;
  await refreshReadinessStatus();
}

async function storeSecret(kind: "groq" | "transform", input: HTMLInputElement) {
  const value = input.value.trim();
  if (!value) return;

  await invoke("set_provider_secret", { kind, value });
  input.value = "";
  await refreshSecretStatus();
  await refreshReadinessStatus();
}

async function clearSecret(kind: "groq" | "transform", input: HTMLInputElement) {
  await invoke("clear_provider_secret", { kind });
  input.value = "";
  await refreshSecretStatus();
  await refreshReadinessStatus();
}

async function loadHistoryEntries() {
  const entries = await invoke<HistoryEntry[]>("list_history_entries");
  renderHistoryEntries(entries);
}

function renderHistoryEntries(entries: HistoryEntry[]) {
  historyList.innerHTML = "";

  if (entries.length === 0) {
    const empty = document.createElement("div");
    empty.className = "history-empty";
    empty.innerHTML = `
      <div class="history-empty-title">No transcripts yet</div>
      <div class="history-empty-detail">Your recent dictation results will show up here after the first successful transcription.</div>
    `;
    historyList.appendChild(empty);
    return;
  }

  entries.forEach((entry) => {
    const item = document.createElement("article");
    item.className = "history-item";

    const createdAt = new Date(entry.createdAtMs).toLocaleString([], {
      dateStyle: "medium",
      timeStyle: "short",
    });

    const selectedPreview =
      entry.selectedText && entry.selectedText.trim().length > 0
        ? `<div class="history-preview-block">
            <div class="history-preview-label">Selected Text Snapshot</div>
            <pre class="history-preview history-preview-secondary">${escapeHtml(entry.selectedText)}</pre>
          </div>`
        : "";

    item.innerHTML = `
      <div class="history-item-header">
        <div>
          <div class="history-item-title">${escapeHtml(createdAt)}</div>
          <div class="history-meta">
            <span>${escapeHtml(entry.engine)}</span>
            <span>${escapeHtml(entry.textMode)}</span>
            <span>${escapeHtml(entry.language || "auto")}</span>
            <span>${escapeHtml(entry.transformProvider || "none")}</span>
          </div>
        </div>
        <div class="history-actions">
          <button class="btn-secondary" data-action="paste" data-id="${entry.id}">Paste</button>
          <button class="btn-secondary" data-action="reprocess" data-id="${entry.id}">Reprocess</button>
          <button class="btn-secondary" data-action="delete" data-id="${entry.id}">Delete</button>
        </div>
      </div>
      <div class="history-preview-block">
        <div class="history-preview-label">Latest Output</div>
        <pre class="history-preview">${escapeHtml(entry.finalText)}</pre>
      </div>
      <div class="history-preview-block">
        <div class="history-preview-label">Raw Transcript</div>
        <pre class="history-preview history-preview-secondary">${escapeHtml(entry.rawText)}</pre>
      </div>
      ${selectedPreview}
    `;

    historyList.appendChild(item);
  });
}

function escapeHtml(value: string) {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function renderHotkey(hotkey: string) {
  hotkeyText.textContent = hotkey.replace(
    "CmdOrCtrl",
    navigator.userAgent.includes("Mac") ? "Cmd" : "Ctrl",
  );
}

function setHotkeyCapture(active: boolean) {
  isCapturingHotkey = active;
  hotkeyRebindBtn.textContent = active ? "Cancel" : "Change";
  hotkeyText.classList.toggle("capturing", active);
  hotkeyHintText.textContent = active
    ? "Press the new shortcut now. Use Command/Ctrl plus another key."
    : "Global keyboard shortcut to trigger recording";
  if (active) {
    hotkeyText.textContent = "Listening…";
  } else {
    renderHotkey(currentSettings.hotkey);
  }
}

function mapKeyboardEventToHotkey(event: KeyboardEvent): string | null {
  const modifiers: string[] = [];
  if (event.metaKey || event.ctrlKey) modifiers.push("CmdOrCtrl");
  if (event.altKey) modifiers.push("Alt");
  if (event.shiftKey) modifiers.push("Shift");

  let key: string | null = null;
  const code = event.code;

  if (/^Key[A-Z]$/.test(code)) {
    key = code.replace("Key", "");
  } else if (/^Digit[0-9]$/.test(code)) {
    key = code.replace("Digit", "");
  } else if (/^F([1-9]|1[0-2])$/.test(code)) {
    key = code;
  } else {
    const keyMap: Record<string, string> = {
      Space: "Space",
      Enter: "Enter",
      Tab: "Tab",
      Escape: "Escape",
      Backspace: "Backspace",
      ArrowUp: "Up",
      ArrowDown: "Down",
      ArrowLeft: "Left",
      ArrowRight: "Right",
      Minus: "-",
      Equal: "=",
      BracketLeft: "[",
      BracketRight: "]",
      Backslash: "\\",
      Semicolon: ";",
      Quote: "'",
      Comma: ",",
      Period: ".",
      Slash: "/",
      Backquote: "`",
    };
    key = keyMap[code] ?? null;
  }

  if (!key || modifiers.length === 0) {
    return null;
  }

  return [...modifiers, key].join("+");
}

async function applyHotkey(hotkey: string) {
  const updated = await invoke<Settings>("set_hotkey", { hotkey });
  currentSettings = updated;
  renderHotkey(currentSettings.hotkey);
  await refreshReadinessStatus();
}

// Event listeners
engineLocal.addEventListener("click", () => {
  setEngine("local");
  saveSettings();
});

engineCloud.addEventListener("click", () => {
  setEngine("cloud");
  saveSettings();
});

micSelect.addEventListener("change", () => saveSettings());
languageSelect.addEventListener("change", () => saveSettings());

modelSelect.addEventListener("change", async () => {
  await checkModelStatus();
  saveSettings();
});

downloadBtn.addEventListener("click", async () => {
  (downloadBtn as HTMLButtonElement).disabled = true;
  downloadProgress.classList.remove("hidden");
  progressFill.style.width = "0%";

  try {
    await invoke("download_model", { modelSize: modelSelect.value });
    downloadBtn.textContent = "\u2713";
    await refreshReadinessStatus();
  } catch (e) {
    downloadBtn.textContent = "Retry";
    (downloadBtn as HTMLButtonElement).disabled = false;
    console.error("Download failed:", e);
  }
  downloadProgress.classList.add("hidden");
});

groqStoreBtn.addEventListener("click", () => storeSecret("groq", groqKey));
groqClearBtn.addEventListener("click", () => clearSecret("groq", groqKey));

modeToggle.addEventListener("click", () => {
  setRecordingMode("toggle");
  saveSettings();
});

modePtt.addEventListener("click", () => {
  setRecordingMode("push-to-talk");
  saveSettings();
});

textModeRaw.addEventListener("click", () => {
  setTextMode("raw");
  saveSettings();
});

textModePolished.addEventListener("click", () => {
  setTextMode("polished");
  saveSettings();
});

textModeCommand.addEventListener("click", () => {
  setTextMode("command");
  saveSettings();
});

transformProviderSelect.addEventListener("change", () => {
  currentSettings.transformProvider = transformProviderSelect.value;
  updateTransformVisibility();
  saveSettings();
});

transformBaseUrl.addEventListener("change", () => saveSettings());
transformModel.addEventListener("change", () => saveSettings());
transformStoreBtn.addEventListener("click", () =>
  storeSecret("transform", transformApiKey),
);
transformClearBtn.addEventListener("click", () =>
  clearSecret("transform", transformApiKey),
);
readinessRefreshBtn.addEventListener("click", () => refreshReadinessStatus());
historyClearBtn.addEventListener("click", async () => {
  await invoke("clear_history_entries");
  await loadHistoryEntries();
});
hotkeyRebindBtn.addEventListener("click", () => {
  setHotkeyCapture(!isCapturingHotkey);
});

historyList.addEventListener("click", async (event) => {
  const target = event.target as HTMLElement;
  const button = target.closest<HTMLButtonElement>("[data-action]");
  if (!button) return;

  const action = button.dataset.action;
  const id = button.dataset.id;
  if (!action || !id) return;

  button.disabled = true;

  try {
    if (action === "paste") {
      await invoke("paste_history_entry", { id });
    } else if (action === "reprocess") {
      await invoke("reprocess_history_entry", { id });
      await loadHistoryEntries();
    } else if (action === "delete") {
      await invoke("delete_history_entry", { id });
      await loadHistoryEntries();
    }
  } catch (error) {
    console.error(`History action failed: ${action}`, error);
  } finally {
    button.disabled = false;
  }
});

window.addEventListener(
  "keydown",
  async (event) => {
    if (!isCapturingHotkey) {
      return;
    }

    event.preventDefault();
    event.stopPropagation();

    if (event.key === "Escape") {
      setHotkeyCapture(false);
      return;
    }

    const hotkey = mapKeyboardEventToHotkey(event);
    if (!hotkey) {
      hotkeyHintText.textContent =
        "Use Command/Ctrl plus at least one non-modifier key.";
      return;
    }

    try {
      await applyHotkey(hotkey);
      setHotkeyCapture(false);
      hotkeyHintText.textContent = "Hotkey updated successfully.";
    } catch (error) {
      setHotkeyCapture(false);
      hotkeyHintText.textContent =
        error instanceof Error ? error.message : String(error);
    }
  },
  true,
);

// Listen for recording state changes
listen<string>("recording-state", (event) => {
  const state = event.payload;
  statusDot.className = "";
  if (state === "Recording") {
    statusDot.classList.add("recording");
    statusText.textContent = "Recording...";
  } else if (state === "Transcribing") {
    statusDot.classList.add("transcribing");
    statusText.textContent = "Transcribing...";
  } else {
    statusDot.classList.add("ready");
    statusText.textContent = "Ready";
    void loadHistoryEntries();
  }
});

// Listen for download progress
listen<DownloadProgress>("download-progress", (event) => {
  const { percent } = event.payload;
  progressFill.style.width = `${percent}%`;
});

// Initialize
loadSettings();
