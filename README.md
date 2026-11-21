# qrqrqr
> Firefox extension on your laptop that reads QR codes off your phone to eg. copy WiFi passwords

Click the toolbar button and point a code at your webcam. Scans pile up in a list under the live camera view, newest first, each with a copy and a delete button. The list is kept in the extension's local storage (never synced).

- **Offline.** The decoder is [`rqrr`](https://crates.io/crates/rqrr), compiled from Rust to WebAssembly and bundled with the extension. Nothing is downloaded or sent anywhere.
- **No extension permissions.** The camera is requested with the standard `getUserMedia` prompt.
- **Reads 6-digit 2FA codes** off a phone screen (OCR, experimental): when no QR code is in view for a second, the centre of the frame is read with [`ocrs`](https://github.com/robertknight/ocrs)' text-recognition model (pure Rust, in the same wasm) A cheap image-processing step finds rows of digit-sized glyphs (bright-on-dark or dark-on-light, tilted phones are fine) and only those are sent to the recognition model, so a read takes ~tens of milliseconds. A code is accepted only if exactly six digits are read twice in a row. It runs in a Web Worker; turn it off in the add-on's options.
- Reads inverted (light-on-dark) codes, lets you pick between several cameras, and shows an *Open link* button for http(s) URLs.

## First run: granting the camera

Firefox can't show its camera permission prompt inside a toolbar popup. The first time, `qrqrqr` shows an **Open scanner in a tab** button. In that tab, allow the camera and tick **Remember this decision**. After that, the popup can use the camera directly.

## Build

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.100   # must match Cargo.toml
make test
make models                                          # OCR model -> extension/models/ (checksummed)
make                                                 # -> extension/pkg/
```

Try it in Firefox: open `about:debugging#/runtime/this-firefox`, choose **Load Temporary Add-on…**, and pick `extension/manifest.json`.
Or run `npx web-ext run --source-dir extension`.

Tests: `make test` (Rust unit tests, plus OCR on the real phone captures in `tests/fixtures/`), `make smoke` (the built wasm in Node) and `make e2e` (the extension in headless Firefox with a still image as the camera).

## Publishing to addons.mozilla.org

`.github/workflows/release.yml` runs when you push a `v*` tag. It:

1. checks that the tag matches the `version` in `extension/manifest.json` and `Cargo.toml`
2. runs the tests, builds the wasm, and lints with `web-ext`
3. submits to AMO on the **listed** channel with `web-ext sign`, attaching the source zip that reviewers need because of the compiled `.wasm`, plus the listing metadata from `amo-metadata.json`
4. creates a GitHub release containing the unsigned build and the source archive

One-time setup:

1. Create API credentials at <https://addons.mozilla.org/developers/addon/api/key/>.
2. Add them as repository secrets: `AMO_JWT_ISSUER` (the JWT issuer) and `AMO_JWT_SECRET`.

To release:

```sh
# bump "version" in extension/manifest.json and Cargo.toml, then
git commit -am "v0.1.1" && git tag v0.1.1 && git push --follow-tags
```

The workflow doesn't wait for review (`--approval-timeout 0`). Mozilla emails you when the version is approved.

## Layout

```
src/lib.rs                 Rust: QR decoder (RGBA frame -> Option<String>, + unit tests)
src/finder.rs, src/ocr.rs  Rust: digit-row finder and 6-digit OCR on top of ocrs' recognizer
extension/ocr-worker.js    runs the OCR off the UI thread, loads the model from extension/models/
extension/                 the add-on (manifest, popup UI); pkg/ is generated
Makefile                   cargo build + wasm-bindgen (+ wasm-opt if installed); make test/smoke/lint/package
scripts/wasm-smoke-test.mjs  loads the built wasm in Node and decodes a generated code
amo-metadata.json          AMO listing (summary, category, license, reviewer notes)
.github/workflows/         ci.yml (every push) and release.yml (tags -> AMO)
```

## License

The code is MIT. The OCR model in `extension/models/` (fetched by `make models`, not stored in git) is a pretrained model of [ocrs-models](https://github.com/robertknight/ocrs-models), licensed [CC-BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/) (trained on HierText, CC-BY-SA 4.0, and synthetic data); it is bundled unmodified.
