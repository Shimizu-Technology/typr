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

interface SetupCheck {
  title: string;
  ok: boolean;
  detail: string;
}

interface SetupStatus {
  platform: string;
  summary: string;
  steps: string[];
  checks: SetupCheck[];
}

interface AppFeedback {
  level: "success" | "error" | "warning";
  title: string;
  detail: string;
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
const feedbackBanner = document.getElementById("feedback-banner")!;
const feedbackTitle = document.getElementById("feedback-title")!;
const feedbackDetail = document.getElementById("feedback-detail")!;
const feedbackDismissBtn = document.getElementById(
  "feedback-dismiss-btn",
) as HTMLButtonElement;
const readinessSummary = document.getElementById("readiness-summary")!;
const readinessList = document.getElementById("readiness-list")!;
const readinessRefreshBtn = document.getElementById(
  "readiness-refresh-btn",
) as HTMLButtonElement;
const setupSummary = document.getElementById("setup-summary")!;
const setupCheckList = document.getElementById("setup-check-list")!;
const setupStepsList = document.getElementById("setup-steps-list")!;
const setupRefreshBtn = document.getElementById(
  "setup-refresh-btn",
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

function showFeedback(feedback: AppFeedback) {
  feedbackBanner.classList.remove("hidden");
  feedbackBanner.classList.remove(
    "feedback-banner-success",
    "feedback-banner-error",
    "feedback-banner-warning",
  );
  feedbackBanner.classList.add(`feedback-banner-${feedback.level}`);
  feedbackTitle.textContent = feedback.title;
  feedbackDetail.textContent = feedback.detail;
}

function hideFeedback() {
  feedbackBanner.classList.add("hidden");
}

function toErrorDetail(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function runAsyncAction(
  action: () => Promise<void>,
  failureTitle: string,
  failureDetail?: string,
) {
  void action().catch((error) => {
    showFeedback({
      level: "error",
      title: failureTitle,
      detail: failureDetail ?? toErrorDetail(error),
    });
  });
}

function buildStatusItem(
  title: string,
  detail: string,
  className: string,
): HTMLDivElement {
  const item = document.createElement("div");
  item.className = className;

  const titleElement = document.createElement("div");
  titleElement.className = "readiness-item-title";
  titleElement.textContent = title;

  const detailElement = document.createElement("div");
  detailElement.className = "readiness-item-detail";
  detailElement.textContent = detail;

  item.append(titleElement, detailElement);
  return item;
}

async function loadSettings() {
  try {
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
    await refreshSetupStatus();
    await loadHistoryEntries();
  } catch (error) {
    showFeedback({
      level: "error",
      title: "Typr could not load settings",
      detail: toErrorDetail(error),
    });
  }
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
    readinessList.appendChild(
      buildStatusItem(
        "All core checks passed",
        "Hotkey, engine, and active mode configuration look good.",
        "readiness-item readiness-item-ok",
      ),
    );
    return;
  }

  readiness.issues.forEach((issue) => {
    readinessList.appendChild(
      buildStatusItem(
        issue.title,
        issue.detail,
        `readiness-item readiness-item-${issue.level}`,
      ),
    );
  });
}

async function refreshSetupStatus() {
  const setup = await invoke<SetupStatus>("get_setup_status");
  setupSummary.textContent = setup.summary;
  setupSummary.className = setup.checks.every((check) => check.ok)
    ? "readiness-summary ready"
    : "readiness-summary blocked";

  setupCheckList.innerHTML = "";
  setup.checks.forEach((check) => {
    setupCheckList.appendChild(
      buildStatusItem(
        check.title,
        check.detail,
        `readiness-item ${check.ok ? "readiness-item-ok" : "readiness-item-warning"}`,
      ),
    );
  });

  setupStepsList.innerHTML = "";
  setup.steps.forEach((step) => {
    const item = document.createElement("li");
    item.textContent = step;
    setupStepsList.appendChild(item);
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

  try {
    await invoke("set_provider_secret", { kind, value });
    input.value = "";
    await refreshSecretStatus();
    await refreshReadinessStatus();
    showFeedback({
      level: "success",
      title: "Secret stored securely",
      detail: "Typr saved that provider credential to OS keychain storage.",
    });
  } catch (error) {
    showFeedback({
      level: "error",
      title: "Could not store secret",
      detail: toErrorDetail(error),
    });
  }
}

async function clearSecret(kind: "groq" | "transform", input: HTMLInputElement) {
  try {
    await invoke("clear_provider_secret", { kind });
    input.value = "";
    await refreshSecretStatus();
    await refreshReadinessStatus();
    showFeedback({
      level: "success",
      title: "Secret removed",
      detail: "Typr removed that provider credential from OS keychain storage.",
    });
  } catch (error) {
    showFeedback({
      level: "error",
      title: "Could not clear secret",
      detail: toErrorDetail(error),
    });
  }
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
    const title = document.createElement("div");
    title.className = "history-empty-title";
    title.textContent = "No transcripts yet";
    const detail = document.createElement("div");
    detail.className = "history-empty-detail";
    detail.textContent =
      "Your recent dictation results will show up here after the first successful transcription.";
    empty.append(title, detail);
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

    const header = document.createElement("div");
    header.className = "history-item-header";

    const headerInfo = document.createElement("div");
    const title = document.createElement("div");
    title.className = "history-item-title";
    title.textContent = createdAt;
    const meta = document.createElement("div");
    meta.className = "history-meta";
    [entry.engine, entry.textMode, entry.language || "auto", entry.transformProvider || "none"]
      .forEach((value) => {
        const chip = document.createElement("span");
        chip.textContent = value;
        meta.appendChild(chip);
      });
    headerInfo.append(title, meta);

    const actions = document.createElement("div");
    actions.className = "history-actions";
    [
      { action: "paste", label: "Paste" },
      { action: "reprocess", label: "Reprocess" },
      { action: "delete", label: "Delete" },
    ].forEach(({ action, label }) => {
      const button = document.createElement("button");
      button.className = "btn-secondary";
      button.type = "button";
      button.dataset.action = action;
      button.dataset.id = entry.id;
      button.textContent = label;
      actions.appendChild(button);
    });

    header.append(headerInfo, actions);
    item.appendChild(header);

    item.appendChild(
      buildHistoryPreviewBlock("Latest Output", entry.finalText, false),
    );
    item.appendChild(
      buildHistoryPreviewBlock("Raw Transcript", entry.rawText, true),
    );

    if (entry.selectedText && entry.selectedText.trim().length > 0) {
      item.appendChild(
        buildHistoryPreviewBlock("Selected Text Snapshot", entry.selectedText, true),
      );
    }

    historyList.appendChild(item);
  });
}

function buildHistoryPreviewBlock(
  label: string,
  value: string,
  secondary: boolean,
) {
  const block = document.createElement("div");
  block.className = "history-preview-block";

  const blockLabel = document.createElement("div");
  blockLabel.className = "history-preview-label";
  blockLabel.textContent = label;

  const preview = document.createElement("pre");
  preview.className = secondary
    ? "history-preview history-preview-secondary"
    : "history-preview";
  preview.textContent = value;

  block.append(blockLabel, preview);
  return block;
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
  runAsyncAction(saveSettings, "Could not save settings");
});

engineCloud.addEventListener("click", () => {
  setEngine("cloud");
  runAsyncAction(saveSettings, "Could not save settings");
});

micSelect.addEventListener("change", () => {
  runAsyncAction(saveSettings, "Could not save settings");
});
languageSelect.addEventListener("change", () => {
  runAsyncAction(saveSettings, "Could not save settings");
});

modelSelect.addEventListener("change", async () => {
  runAsyncAction(async () => {
    await checkModelStatus();
    await saveSettings();
  }, "Could not update model settings");
});

downloadBtn.addEventListener("click", async () => {
  (downloadBtn as HTMLButtonElement).disabled = true;
  downloadProgress.classList.remove("hidden");
  progressFill.style.width = "0%";

  try {
    await invoke("download_model", { modelSize: modelSelect.value });
    downloadBtn.textContent = "\u2713";
    await refreshReadinessStatus();
    showFeedback({
      level: "success",
      title: "Model downloaded",
      detail: "The local Whisper model is ready for transcription.",
    });
  } catch (e) {
    downloadBtn.textContent = "Retry";
    (downloadBtn as HTMLButtonElement).disabled = false;
    showFeedback({
      level: "error",
      title: "Model download failed",
      detail: toErrorDetail(e),
    });
  }
  downloadProgress.classList.add("hidden");
});

groqStoreBtn.addEventListener("click", () => storeSecret("groq", groqKey));
groqClearBtn.addEventListener("click", () => clearSecret("groq", groqKey));

modeToggle.addEventListener("click", () => {
  setRecordingMode("toggle");
  runAsyncAction(saveSettings, "Could not save settings");
});

modePtt.addEventListener("click", () => {
  setRecordingMode("push-to-talk");
  runAsyncAction(saveSettings, "Could not save settings");
});

textModeRaw.addEventListener("click", () => {
  setTextMode("raw");
  runAsyncAction(saveSettings, "Could not save settings");
});

textModePolished.addEventListener("click", () => {
  setTextMode("polished");
  runAsyncAction(saveSettings, "Could not save settings");
});

textModeCommand.addEventListener("click", () => {
  setTextMode("command");
  runAsyncAction(saveSettings, "Could not save settings");
});

transformProviderSelect.addEventListener("change", () => {
  currentSettings.transformProvider = transformProviderSelect.value;
  updateTransformVisibility();
  runAsyncAction(saveSettings, "Could not save settings");
});

transformBaseUrl.addEventListener("change", () => {
  runAsyncAction(saveSettings, "Could not save settings");
});
transformModel.addEventListener("change", () => {
  runAsyncAction(saveSettings, "Could not save settings");
});
transformStoreBtn.addEventListener("click", () =>
  storeSecret("transform", transformApiKey),
);
transformClearBtn.addEventListener("click", () =>
  clearSecret("transform", transformApiKey),
);
readinessRefreshBtn.addEventListener("click", () => {
  runAsyncAction(refreshReadinessStatus, "Could not refresh readiness status");
});
setupRefreshBtn.addEventListener("click", () => {
  runAsyncAction(refreshSetupStatus, "Could not refresh setup status");
});
historyClearBtn.addEventListener("click", async () => {
  try {
    await invoke("clear_history_entries");
    await loadHistoryEntries();
    showFeedback({
      level: "success",
      title: "History cleared",
      detail: "Local transcript history was removed from this device.",
    });
  } catch (error) {
    showFeedback({
      level: "error",
      title: "Could not clear history",
      detail: toErrorDetail(error),
    });
  }
});
feedbackDismissBtn.addEventListener("click", () => hideFeedback());
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
      showFeedback({
        level: "success",
        title: "History entry pasted",
        detail: "Typr copied the saved output and attempted to paste it into the active app.",
      });
    } else if (action === "reprocess") {
      await invoke("reprocess_history_entry", { id });
      await loadHistoryEntries();
      showFeedback({
        level: "success",
        title: "History entry reprocessed",
        detail: "Typr regenerated that transcript using your current mode and provider settings.",
      });
    } else if (action === "delete") {
      await invoke("delete_history_entry", { id });
      await loadHistoryEntries();
      showFeedback({
        level: "success",
        title: "History entry deleted",
        detail: "That saved transcript was removed from local history.",
      });
    }
  } catch (error) {
    showFeedback({
      level: "error",
      title: "History action failed",
      detail: toErrorDetail(error),
    });
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
    runAsyncAction(loadHistoryEntries, "Could not refresh history");
  }
});

listen<AppFeedback>("app-feedback", (event) => {
  showFeedback(event.payload);
  runAsyncAction(refreshReadinessStatus, "Could not refresh readiness status");
  runAsyncAction(refreshSetupStatus, "Could not refresh setup status");
});

// Listen for download progress
listen<DownloadProgress>("download-progress", (event) => {
  const { percent } = event.payload;
  progressFill.style.width = `${percent}%`;
});

// Initialize
void loadSettings();
