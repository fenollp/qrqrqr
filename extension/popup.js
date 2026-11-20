import init, { decode_rgba } from "./pkg/qrqrqr.js";

const $ = (id) => document.getElementById(id);
const video = $("video");
const statusEl = $("status");
const cameraSel = $("camera");
const scanSection = $("scan");
const resultSection = $("result");
const textEl = $("text");
const openLink = $("open-link");

const IN_TAB = new URLSearchParams(location.search).has("tab");
if (IN_TAB) document.body.classList.add("tab");

const MAX_SIDE = 800;        // downscale frames before decoding
const FRAME_INTERVAL = 120;  // ms between decode attempts
const PROMPT_TIMEOUT = 4000; // popup can't show the permission prompt; don't wait forever

const canvas = document.createElement("canvas");
const ctx = canvas.getContext("2d", { willReadFrequently: true });

let stream = null;
let timer = null;
let wasmReady = null;

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
  setStatus("Point the camera at a QR code.");
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
    if (text !== undefined) return showResult(text);
  }
  timer = setTimeout(scanLoop, FRAME_INTERVAL);
}

function isHttpUrl(s) {
  try { return ["http:", "https:"].includes(new URL(s.trim()).protocol); } catch { return false; }
}

function showResult(text) {
  stopCamera();
  scanSection.hidden = true;
  resultSection.hidden = false;
  textEl.value = text;
  textEl.focus();
  textEl.select();
  openLink.hidden = !isHttpUrl(text);
  if (!openLink.hidden) openLink.href = text.trim();
  $("copy").textContent = "Copy";
}

$("copy").addEventListener("click", async () => {
  try {
    await navigator.clipboard.writeText(textEl.value);
  } catch {
    textEl.select();
    document.execCommand("copy");
  }
  $("copy").textContent = "Copied ✓";
});

$("again").addEventListener("click", () => {
  resultSection.hidden = true;
  scanSection.hidden = false;
  startCamera();
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

wasmReady = init();
startCamera();
