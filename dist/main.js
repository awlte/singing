const { invoke } = window.__TAURI__.core;

const canvas = document.getElementById("wave");
const ctx = canvas.getContext("2d");
const selectionEl = document.getElementById("selection");
const cursorEl = document.getElementById("cursor");
const statusEl = document.getElementById("status");
const playBtn = document.getElementById("play");
const saveBtn = document.getElementById("save");
const folderBtn = document.getElementById("folder");
const optionsBtn = document.getElementById("options");
const optionsPanel = document.getElementById("options-panel");
const deviceSelect = document.getElementById("device-select");
const folderInput = document.getElementById("folder-input");
const folderPickBtn = document.getElementById("folder-pick");
const bufferSelect = document.getElementById("buffer-select");
const playSelect = document.getElementById("play-select");
const player = document.getElementById("player");
const waveWrap = canvas.parentElement;

let config = null; // populated on first options open / startup

let dpr = window.devicePixelRatio || 1;
let info = { sample_rate: 16000, capacity_samples: 0, capacity_ms: 0, current_samples: 0, current_ms: 0 };
let peaks = [];
let selection = null; // { startSample, endSample }
let dragStart = null;
let pollHandle = null;
let blobUrl = null;

function resize() {
  const r = canvas.getBoundingClientRect();
  dpr = window.devicePixelRatio || 1;
  canvas.width = Math.max(1, Math.floor(r.width * dpr));
  canvas.height = Math.max(1, Math.floor(r.height * dpr));
  draw();
}

window.addEventListener("resize", resize);

async function refresh() {
  info = await invoke("buffer_info");
  const widthPx = canvas.width;
  if (widthPx === 0) return;
  peaks = await invoke("get_peaks", { width: widthPx });
  draw();
  updateStatus();
}

function updateStatus() {
  const cur = formatMs(info.current_ms);
  const cap = formatMs(info.capacity_ms);
  let sel = "no selection";
  if (selection && selection.endSample > selection.startSample) {
    const ms = ((selection.endSample - selection.startSample) * 1000) / info.sample_rate;
    sel = `selection ${formatMs(Math.round(ms))}`;
  }
  statusEl.textContent = `buffer ${cur} / ${cap} · ${sel}`;
  saveBtn.disabled = info.current_samples === 0;
  saveBtn.textContent = selection && selection.endSample > selection.startSample
    ? "⬇ Save selection"
    : "⬇ Save all";
}

function formatMs(ms) {
  const s = Math.round(ms / 1000);
  const m = Math.floor(s / 60);
  const r = s % 60;
  return `${m}:${String(r).padStart(2, "0")}`;
}

function draw() {
  const w = canvas.width;
  const h = canvas.height;
  ctx.fillStyle = "#1d2024";
  ctx.fillRect(0, 0, w, h);

  if (peaks.length === 0) return;
  const mid = h / 2;
  const max = 32768;
  ctx.fillStyle = "#6a7080";
  for (let x = 0; x < peaks.length; x++) {
    const v = Math.abs(peaks[x]) / max;
    const barH = Math.max(1, v * (h - 4));
    ctx.fillRect(x, mid - barH / 2, 1, barH);
  }

  // Active region: portion of canvas covered by current buffer fill.
  if (info.capacity_samples > 0) {
    const filled = (info.current_samples / info.capacity_samples) * w;
    // We draw waveform across the whole width; the "filled" portion is on the right
    // because new samples sit at the end of the snapshot. Until the buffer is full,
    // older samples don't exist — gray out the empty left side.
    if (filled < w) {
      ctx.fillStyle = "rgba(0,0,0,0.45)";
      ctx.fillRect(0, 0, w - filled, h);
    }
  }
}

function xToSample(xPx) {
  const r = canvas.getBoundingClientRect();
  const xCanvas = (xPx - r.left) * dpr;
  const frac = Math.max(0, Math.min(1, xCanvas / canvas.width));
  // peaks span the entire snapshot which has `current_samples` elements.
  return Math.round(frac * info.current_samples);
}

function sampleToFracX(sample) {
  if (info.current_samples === 0) return 0;
  return sample / info.current_samples;
}

function renderSelection() {
  if (!selection || selection.endSample <= selection.startSample) {
    selectionEl.classList.add("hidden");
    return;
  }
  const r = waveWrap.getBoundingClientRect();
  const lFrac = sampleToFracX(selection.startSample);
  const rFrac = sampleToFracX(selection.endSample);
  selectionEl.style.left = `${lFrac * r.width}px`;
  selectionEl.style.width = `${(rFrac - lFrac) * r.width}px`;
  selectionEl.classList.remove("hidden");
}

waveWrap.addEventListener("pointerdown", (e) => {
  if (e.button !== 0) return;
  waveWrap.setPointerCapture(e.pointerId);
  const s = xToSample(e.clientX);
  dragStart = s;
  selection = { startSample: s, endSample: s };
  renderSelection();
  updateStatus();
});

waveWrap.addEventListener("pointermove", (e) => {
  if (dragStart == null) return;
  const s = xToSample(e.clientX);
  selection = {
    startSample: Math.min(dragStart, s),
    endSample: Math.max(dragStart, s),
  };
  renderSelection();
  updateStatus();
});

waveWrap.addEventListener("pointerup", (e) => {
  if (dragStart == null) return;
  waveWrap.releasePointerCapture(e.pointerId);
  const s = xToSample(e.clientX);
  if (Math.abs(s - dragStart) < info.sample_rate * 0.05) {
    // treated as click → clear selection
    selection = null;
    renderSelection();
  }
  dragStart = null;
  updateStatus();
});

playBtn.addEventListener("click", async () => {
  let range;
  if (selection && selection.endSample > selection.startSample) {
    range = selection;
  } else {
    const tailSecs = (config && config.play_tail_secs) || 30;
    const tail = Math.min(info.current_samples, info.sample_rate * tailSecs);
    range = {
      startSample: info.current_samples - tail,
      endSample: info.current_samples,
    };
  }
  if (range.endSample <= range.startSample) return;

  if (blobUrl) {
    URL.revokeObjectURL(blobUrl);
    blobUrl = null;
  }
  const data = await invoke("get_wav", {
    startSample: range.startSample,
    endSample: range.endSample,
  });
  const blob = new Blob([data], { type: "audio/wav" });
  blobUrl = URL.createObjectURL(blob);
  player.src = blobUrl;
  player.currentTime = 0;
  await player.play();
});

player.addEventListener("timeupdate", () => {
  if (!player.duration || !selection) {
    cursorEl.classList.remove("visible");
    return;
  }
  const frac = player.currentTime / player.duration;
  const lFrac = sampleToFracX(selection.startSample);
  const rFrac = sampleToFracX(selection.endSample);
  const cursorFrac = lFrac + (rFrac - lFrac) * frac;
  const r = waveWrap.getBoundingClientRect();
  cursorEl.style.left = `${cursorFrac * r.width}px`;
  cursorEl.classList.add("visible");
});
player.addEventListener("ended", () => cursorEl.classList.remove("visible"));
player.addEventListener("pause", () => {
  if (player.currentTime === 0) cursorEl.classList.remove("visible");
});

saveBtn.addEventListener("click", async () => {
  const range = (selection && selection.endSample > selection.startSample)
    ? selection
    : { startSample: 0, endSample: info.current_samples };
  if (range.endSample <= range.startSample) return;
  try {
    const path = await invoke("save_clip", {
      startSample: range.startSample,
      endSample: range.endSample,
    });
    statusEl.textContent = `saved → ${path.split("/").pop()}`;
  } catch (err) {
    statusEl.textContent = `save failed: ${err}`;
  }
});

folderBtn.addEventListener("click", () => invoke("open_captures_dir"));

async function loadConfig() {
  config = await invoke("get_config");
  folderInput.value = config.save_folder;
  bufferSelect.value = String(config.buffer_secs);
  playSelect.value = String(config.play_tail_secs);
}

async function loadDevices() {
  const [devices, current] = await Promise.all([
    invoke("input_devices"),
    invoke("current_input_device"),
  ]);
  deviceSelect.innerHTML = "";
  const defaultOpt = document.createElement("option");
  defaultOpt.value = "";
  defaultOpt.textContent = "System default";
  deviceSelect.appendChild(defaultOpt);
  for (const d of devices) {
    const opt = document.createElement("option");
    opt.value = d.name;
    opt.textContent = d.is_default ? `${d.name} (default)` : d.name;
    if (current && current === d.name) opt.selected = true;
    deviceSelect.appendChild(opt);
  }
}

deviceSelect.addEventListener("change", async () => {
  const name = deviceSelect.value || null;
  try {
    await invoke("set_input_device", { name });
    statusEl.textContent = `switched to ${name ?? "default"}, buffer cleared`;
  } catch (err) {
    statusEl.textContent = `switch failed: ${err}`;
  }
});

async function commitFolder(path) {
  if (!path) return;
  try {
    await invoke("set_save_folder", { path });
    config.save_folder = path;
    folderInput.value = path;
    statusEl.textContent = `save folder → ${path}`;
  } catch (err) {
    statusEl.textContent = `folder failed: ${err}`;
  }
}

folderInput.addEventListener("change", () => commitFolder(folderInput.value.trim()));

folderPickBtn.addEventListener("click", async (e) => {
  e.stopPropagation();
  const picked = await window.__TAURI__.dialog.open({
    directory: true,
    multiple: false,
    defaultPath: folderInput.value || undefined,
  });
  if (typeof picked === "string") await commitFolder(picked);
});

bufferSelect.addEventListener("change", async () => {
  const secs = parseInt(bufferSelect.value, 10);
  try {
    await invoke("set_buffer_secs", { secs });
    config.buffer_secs = secs;
    statusEl.textContent = `buffer → ${Math.round(secs / 60)} min (cleared)`;
  } catch (err) {
    statusEl.textContent = `buffer failed: ${err}`;
  }
});

playSelect.addEventListener("change", async () => {
  const secs = parseInt(playSelect.value, 10);
  try {
    await invoke("set_play_tail_secs", { secs });
    config.play_tail_secs = secs;
    statusEl.textContent = `default play length → ${secs}s`;
  } catch (err) {
    statusEl.textContent = `play length failed: ${err}`;
  }
});

optionsBtn.addEventListener("click", async (e) => {
  e.stopPropagation();
  const opening = optionsPanel.classList.contains("hidden");
  optionsPanel.classList.toggle("hidden");
  if (opening) {
    await Promise.all([loadConfig(), loadDevices()]);
  }
});

document.addEventListener("click", (e) => {
  if (
    !optionsPanel.classList.contains("hidden") &&
    !optionsPanel.contains(e.target) &&
    e.target !== optionsBtn
  ) {
    optionsPanel.classList.add("hidden");
  }
});

window.addEventListener("keydown", (e) => {
  if (e.key === " " || e.code === "Space") {
    e.preventDefault();
    if (player.paused) playBtn.click();
    else player.pause();
  } else if (e.key === "Escape") {
    selection = null;
    renderSelection();
    updateStatus();
  } else if ((e.metaKey || e.ctrlKey) && e.key === "s") {
    e.preventDefault();
    saveBtn.click();
  }
});

resize();
refresh();
loadConfig().catch(() => {});
pollHandle = setInterval(refresh, 500);
