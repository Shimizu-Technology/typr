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

interface DownloadProgress {
  downloaded: number;
  total: number;
  percent: number;
}

// DOM elements
const statusDot = document.getElementById("status-dot")!;
const statusText = document.getElementById("status-text")!;
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
const languageSelect = document.getElementById("language-select") as HTMLSelectElement;
const textModeRaw = document.getElementById("text-mode-raw")!;
const textModePolished = document.getElementById("text-mode-polished")!;
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
  hotkeyText.textContent = currentSettings.hotkey.replace(
    "CmdOrCtrl",
    navigator.userAgent.includes("Mac") ? "Cmd" : "Ctrl",
  );
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
  updateTransformVisibility();
}

function updateTransformVisibility() {
  const showTransformSettings =
    currentSettings.textMode === "polished" &&
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

async function saveSettings() {
  currentSettings.microphone = micSelect.value;
  currentSettings.whisperModel = modelSelect.value;
  currentSettings.language = languageSelect.value;
  currentSettings.transformProvider = transformProviderSelect.value;
  currentSettings.transformBaseUrl = transformBaseUrl.value;
  currentSettings.transformModel = transformModel.value;
  await invoke("save_settings", { settings: currentSettings });
}

async function storeSecret(kind: "groq" | "transform", input: HTMLInputElement) {
  const value = input.value.trim();
  if (!value) return;

  await invoke("set_provider_secret", { kind, value });
  input.value = "";
  await refreshSecretStatus();
}

async function clearSecret(kind: "groq" | "transform", input: HTMLInputElement) {
  await invoke("clear_provider_secret", { kind });
  input.value = "";
  await refreshSecretStatus();
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
  }
});

// Listen for download progress
listen<DownloadProgress>("download-progress", (event) => {
  const { percent } = event.payload;
  progressFill.style.width = `${percent}%`;
});

// Initialize
loadSettings();
