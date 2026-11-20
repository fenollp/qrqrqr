import init, { decode_rgba } from "./pkg/qrqrqr.js";

const $ = (id) => document.getElementById(id);
const video = $("video");
const statusEl = $("status");
const cameraSel = $("camera");
const viewport = document.querySelector(".viewport");
const historySection = $("history");
const listEl = $("list");

const IN_TAB = new URLSearchParams(location.search).has("tab");
if (IN_TAB) document.body.classList.add("tab");

const MAX_SIDE = 800;        // downscale frames before decoding
const FRAME_INTERVAL = 120;  // ms between decode attempts
const SAME_CODE_GAP = 1500;  // a code unseen this long (ms) may be added again
const MAX_ITEMS = 50;        // stored history cap (the list scrolls)
const STORE_KEY = "scans";
const PROMPT_TIMEOUT = 4000; // popup can't show the permission prompt; don't wait forever

const canvas = document.createElement("canvas");
const ctx = canvas.getContext("2d", { willReadFrequently: true });

let stream = null;
let timer = null;
let wasmReady = null;
let items = [];       // newest first
let lastText = null;  // code currently in front of the camera
let lastSeen = 0;

function setStatus(msg, isError = false) {
  statusEl.textContent = msg;
  statusEl.classList.toggle("error", isError);
}

function stopCamera() {
  clearTimeout(timer);
  timer = null;
  if (stream) stream.getTracks().forEach((t) => t.stop());
  stream = null;
  video.srcObject = null;
}

function withTimeout(promise, ms) {
  return Promise.race([
    promise,
    new Promise((_, reject) =>
      setTimeout(() => reject(Object.assign(new Error("timeout"), { name: "PromptTimeout" })), ms)
    ),
  ]);
}

async function listCameras(activeId) {
  const cams = (await navigator.mediaDevices.enumerateDevices()).filter((d) => d.kind === "videoinput");
  cameraSel.replaceChildren(
    ...cams.map((c, i) => new Option(c.label || `Camera ${i + 1}`, c.deviceId, false, c.deviceId === activeId))
  );
  cameraSel.hidden = cams.length < 2;
}

async function startCamera() {
  stopCamera();
  setStatus("Starting camera…");
  $("needs-permission").hidden = true;

  let saved = null;
  try { saved = localStorage.getItem("cameraId"); } catch {}
  const video_constraints = saved
    ? { deviceId: { exact: saved } }
    : { facingMode: "environment", width: { ideal: 1280 }, height: { ideal: 720 } };

  try {
    const req = navigator.mediaDevices.getUserMedia({ video: video_constraints, audio: false })
      .catch((e) => {
        // A remembered camera that's been unplugged: fall back to any camera.
        if (saved && e.name === "OverconstrainedError") {
          try { localStorage.removeItem("cameraId"); } catch {}
          return navigator.mediaDevices.getUserMedia({ video: true, audio: false });
        }
        throw e;
      });
    stream = IN_TAB ? await req : await withTimeout(req, PROMPT_TIMEOUT);
  } catch (e) {
    console.debug("getUserMedia failed:", e);
    if (e.name === "NotFoundError" || e.name === "OverconstrainedError") {
      setStatus("No camera found.", true);
    } else if (IN_TAB) {
      setStatus("Camera access was denied. Allow it from the camera icon in the address bar, then reload.", true);
    } else {
      setStatus("Camera permission needed.", true);
      $("needs-permission").hidden = false;
    }
    return;
  }

  video.srcObject = stream;
  await video.play().catch(() => {});
  const activeId = stream.getVideoTracks()[0]?.getSettings().deviceId;
  listCameras(activeId).catch(() => {});
  setStatus("Point a code at the camera.");
  await wasmReady;
  scanLoop();
}

function scanLoop() {
  if (!stream) return;
  const vw = video.videoWidth, vh = video.videoHeight;
  if (vw && vh) {
    const scale = Math.min(1, MAX_SIDE / Math.max(vw, vh));
    const w = Math.round(vw * scale), h = Math.round(vh * scale);
    if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
    ctx.drawImage(video, 0, 0, w, h);
    const { data } = ctx.getImageData(0, 0, w, h);
    const text = decode_rgba(new Uint8Array(data.buffer, data.byteOffset, data.byteLength), w, h);
    if (text !== undefined) onDecoded(text);
  }
  timer = setTimeout(scanLoop, FRAME_INTERVAL);
}

function onDecoded(text) {
  const now = performance.now();
  const repeat = text === lastText && now - lastSeen < SAME_CODE_GAP;
  lastText = text;
  lastSeen = now;
  if (repeat) return;
  items.unshift({ text, at: Date.now() });
  items.length = Math.min(items.length, MAX_ITEMS);
  save();
  render();
  listEl.scrollTop = 0;
  viewport.classList.remove("hit");
  void viewport.offsetWidth; // restart the flash animation
  viewport.classList.add("hit");
}

// Kept in localStorage: stays on this machine and needs no extra permission.
// (storage.sync would upload scans, which can be Wi-Fi passwords, and caps items at 8 KB.)
function load() {
  try { items = JSON.parse(localStorage.getItem(STORE_KEY)) ?? []; } catch { items = []; }
  if (!Array.isArray(items)) items = [];
}

function save() {
  try { localStorage.setItem(STORE_KEY, JSON.stringify(items)); } catch {}
}

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const ta = document.createElement("textarea");
    ta.value = text;
    document.body.append(ta);
    ta.select();
    document.execCommand("copy");
    ta.remove();
  }
}

function render() {
  historySection.hidden = items.length === 0;
  listEl.replaceChildren(
    ...items.map((item) => {
      const li = document.createElement("li");

      const copy = document.createElement("button");
      copy.type = "button";
      copy.className = "icon";
      copy.title = "Copy";
      copy.setAttribute("aria-label", "Copy");
      copy.textContent = "⧉";
      copy.addEventListener("click", async () => {
        await copyText(item.text);
        copy.textContent = "✓";
        setTimeout(() => { copy.textContent = "⧉"; }, 1200);
      });

      let label;
      if (isHttpUrl(item.text)) {
        label = document.createElement("a");
        label.href = item.text.trim();
        label.target = "_blank";
        label.rel = "noopener noreferrer";
      } else {
        label = document.createElement("span");
      }
      label.className = "text";
      label.textContent = item.text;

      const del = document.createElement("button");
      del.type = "button";
      del.className = "icon";
      del.title = "Delete";
      del.setAttribute("aria-label", "Delete");
      del.textContent = "✕";
      del.addEventListener("click", () => {
        items = items.filter((i) => i !== item);
        save();
        render();
      });

      li.append(copy, label, del);
      return li;
    })
  );
}

function isHttpUrl(s) {
  try { return ["http:", "https:"].includes(new URL(s.trim()).protocol); } catch { return false; }
}

$("clear").addEventListener("click", () => {
  items = [];
  save();
  render();
});

cameraSel.addEventListener("change", () => {
  try { localStorage.setItem("cameraId", cameraSel.value); } catch {}
  startCamera();
});

$("open-tab").addEventListener("click", async () => {
  const url = browser.runtime.getURL("popup.html?tab=1");
  await browser.tabs.create({ url });
  window.close();
});

window.addEventListener("pagehide", stopCamera);

load();
render();
wasmReady = init();
startCamera();
