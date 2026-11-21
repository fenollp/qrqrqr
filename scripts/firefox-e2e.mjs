// End-to-end test in a real Firefox: loads the built extension, replaces the camera by a still
// image, and checks the scan list. Usage: `make e2e` (needs Firefox + Node; Selenium fetches
// geckodriver; `npm i --no-save selenium-webdriver pngjs`). FIREFOX_BIN overrides the binary.
//
// Test-only changes are made to a temporary *copy* of extension/: a camera shim, web-accessible
// resources (Marionette cannot navigate to moz-extension:// directly, so we go through a page
// on http://127.0.0.1), and a lowered strict_min_version when the installed Firefox is older.
import { Builder } from "selenium-webdriver";
import firefox from "selenium-webdriver/firefox.js";
import { PNG } from "pngjs";
import http from "node:http";
import { cpSync, mkdtempSync, readFileSync, writeFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const tmp = mkdtempSync(join(tmpdir(), "qrqrqr-e2e-"));
const ext = join(tmp, "extension");
cpSync(join(root, "extension"), ext, { recursive: true });

// Frames the fake camera can show: the real captures plus a blank one.
const frames = {};
for (const f of readdirSync(join(root, "tests/fixtures")).filter((f) => f.endsWith("-frame.png"))) {
  cpSync(join(root, "tests/fixtures", f), join(ext, f));
  frames[f] = "729592";
}
const blank = new PNG({ width: 640, height: 480 });
blank.data.fill(128);
writeFileSync(join(ext, "blank.png"), PNG.sync.write(blank));
frames["blank.png"] = null;

writeFileSync(join(ext, "shim.js"), `
// TEST ONLY: the camera is a still image (?frame=NAME) painted on a canvas.
const name = new URLSearchParams(location.search).get("frame");
navigator.mediaDevices.enumerateDevices = async () => [];
navigator.mediaDevices.getUserMedia = async () => {
  const img = new Image();
  await new Promise((r) => { img.onload = r; img.src = name; });
  const c = document.createElement("canvas");
  c.width = img.naturalWidth; c.height = img.naturalHeight;
  const g = c.getContext("2d");
  g.drawImage(img, 0, 0);
  setInterval(() => g.drawImage(img, 0, 0), 50); // keep the stream producing frames
  return c.captureStream(15);
};`);
const html = readFileSync(join(ext, "popup.html"), "utf8");
writeFileSync(join(ext, "popup.html"), html.replace('<script type="module" src="popup.js">', '<script src="shim.js"></script>\n  <script type="module" src="popup.js">'));

const uuid = "5b8a1c3e-0f6d-4d27-9a51-7c2e8f4b9d10";
const server = http.createServer((_, r) => r.setHeader("content-type", "text/html").end("<title>host</title>")).listen(0, "127.0.0.1");
await new Promise((r) => server.once("listening", r));
const hostUrl = `http://127.0.0.1:${server.address().port}/`;

const opts = new firefox.Options().addArguments("-headless");
if (process.env.FIREFOX_BIN) opts.setBinary(process.env.FIREFOX_BIN);
opts.setPreference("extensions.webextensions.uuids", JSON.stringify({ "qrqrqr@fenollp.github.io": uuid }));
opts.setPreference("xpinstall.signatures.required", false);

const driver = await new Builder().forBrowser("firefox").setFirefoxOptions(opts).build();
let failed = 0;
try {
  const version = (await driver.getCapabilities()).get("browserVersion");
  const manifestPath = join(ext, "manifest.json");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  const minVersion = parseInt(manifest.browser_specific_settings.gecko.strict_min_version);
  // The shipped manifest requires a recent Firefox; test on an older one by relaxing the minimum.
  if (parseInt(version) < minVersion) {
    console.log(`Firefox ${version} < required ${minVersion}: lowering strict_min_version in the test copy`);
    manifest.browser_specific_settings.gecko.strict_min_version = `${parseInt(version)}.0`;
    delete manifest.browser_specific_settings.gecko_android;
  }
  manifest.web_accessible_resources = [{ resources: ["*", "pkg/*", "models/*"], matches: ["http://127.0.0.1/*"] }];
  writeFileSync(manifestPath, JSON.stringify(manifest, null, 2));
  await driver.installAddon(ext, true);
  console.log(`Firefox ${version}, extension installed`);

  for (const [frame, expected] of Object.entries(frames)) {
    await driver.get(hostUrl);
    await driver.executeScript(`location.href = "moz-extension://${uuid}/popup.html?tab=1&frame=${frame}"`);
    await driver.wait(async () => (await driver.executeScript("return document.readyState")) === "complete", 5000);
    await driver.executeScript("localStorage.clear()");
    await driver.navigate().refresh();
    const t0 = Date.now();
    const timeout = expected ? 20000 : 6000;
    let items = [];
    while (Date.now() - t0 < timeout) {
      items = await driver.executeScript("return [...document.querySelectorAll('#list .text')].map((e) => e.textContent)");
      if (items.length) break;
      await new Promise((r) => setTimeout(r, 250));
    }
    const ok = expected ? items.length === 1 && items[0] === expected : items.length === 0;
    if (!ok) failed++;
    console.log(`${ok ? "ok  " : "FAIL"} ${frame.padEnd(28)} -> ${JSON.stringify(items)} (${Date.now() - t0} ms)`);
  }
} finally {
  await driver.quit();
  server.close();
  rmSync(tmp, { recursive: true, force: true });
}
process.exit(failed ? 1 : 0);
