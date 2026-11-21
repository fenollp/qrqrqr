// Loads the built wasm exactly as the extension does and decodes a generated QR code.
// Usage: node scripts/wasm-smoke-test.mjs   (after `make wasm models`; needs `npm i --no-save qrcode pngjs`)
import { readFile } from "node:fs/promises";
import QRCode from "qrcode";
import { PNG } from "pngjs";
import { initSync, decode_rgba, DigitReader } from "../extension/pkg/qrqrqr.js";

initSync({ module: await readFile(new URL("../extension/pkg/qrqrqr_bg.wasm", import.meta.url)) });

const text = "Hello from qrqrqr — ünïcødé ✓ https://example.org/?q=1";
const qr = QRCode.create(text, { errorCorrectionLevel: "M" });
const n = qr.modules.size, scale = 5, quiet = 4, size = (n + 2 * quiet) * scale;
const rgba = new Uint8Array(size * size * 4).fill(255);
for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
  const mx = Math.floor(x / scale) - quiet, my = Math.floor(y / scale) - quiet;
  if (mx >= 0 && my >= 0 && mx < n && my < n && qr.modules.get(my, mx)) {
    const i = (y * size + x) * 4; rgba[i] = rgba[i + 1] = rgba[i + 2] = 0;
  }
}

const t0 = performance.now();
const got = decode_rgba(rgba, size, size);
const ms = (performance.now() - t0).toFixed(1);
if (got !== text) { console.error("FAIL: got", got); process.exit(1); }
if (decode_rgba(new Uint8Array(64 * 64 * 4).fill(255), 64, 64) !== undefined) {
  console.error("FAIL: blank frame decoded"); process.exit(1);
}
console.log(`ok: decoded ${size}x${size} frame in ${ms} ms`);

// The OCR model loads from the extension's models/ directory and finds nothing in a blank frame.
const reader = new DigitReader(await readFile(new URL("../extension/models/text-recognition.rten", import.meta.url)));
if (reader.read(new Uint8Array(320 * 240 * 4).fill(255), 320, 240) !== undefined) {
  console.error("FAIL: OCR read a code in a blank frame"); process.exit(1);
}
console.log("ok: OCR model loaded, blank frame has no code");

// Real webcam captures of a phone showing the code 729 592 (tests/fixtures/).
for (const name of ["phone-frame", "phone-roi", "phone-blurred-frame", "phone-blurred-roi"]) {
  const png = PNG.sync.read(await readFile(new URL(`../tests/fixtures/${name}.png`, import.meta.url)));
  const t = performance.now();
  const code = reader.read(new Uint8Array(png.data.buffer, png.data.byteOffset, png.data.byteLength), png.width, png.height);
  if (code !== "729592") { console.error(`FAIL: ${name}: got`, code); process.exit(1); }
  console.log(`ok: ${name}.png -> ${code} in ${(performance.now() - t).toFixed(0)} ms`);
}
