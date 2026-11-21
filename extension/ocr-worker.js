// Runs the OCR off the popup's main thread so the camera preview stays smooth.
// The model file is read from the extension's own models/ directory; nothing is fetched remotely.
import init, { DigitReader } from "./pkg/qrqrqr.js";

let reader = null;

async function load() {
  const res = await fetch(new URL("models/text-recognition.rten", import.meta.url));
  if (!res.ok) throw new Error(`models/text-recognition.rten: HTTP ${res.status}`);
  const model = new Uint8Array(await res.arrayBuffer());
  await init();
  reader = new DigitReader(model);
}

const ready = load().then(
  () => postMessage({ type: "ready" }),
  (e) => postMessage({ type: "error", message: String(e?.message ?? e) })
);

onmessage = async ({ data: { rgba, width, height } }) => {
  await ready;
  if (!reader) return;
  const t0 = performance.now();
  let code;
  try {
    code = reader.read(new Uint8Array(rgba), width, height);
  } catch (e) {
    return postMessage({ type: "error", message: String(e?.message ?? e) });
  }
  postMessage({ type: "result", code: code ?? null, ms: Math.round(performance.now() - t0) });
};
