# qrqrqr
> Firefox extension on your laptop that reads QR codes off your phone to eg. copy WiFi passwords

Click the toolbar button and hold a QR code up to your webcam. The decoded text appears in a box you can copy or edit.

- **Offline.** The decoder is [`rqrr`](https://crates.io/crates/rqrr), compiled from Rust to WebAssembly and bundled with the extension. Nothing is downloaded or sent anywhere.
- **No extension permissions.** The camera is requested with the standard `getUserMedia` prompt.
- Reads inverted (light-on-dark) codes, lets you pick between several cameras, and shows an *Open link* button for http(s) URLs.

## First run: granting the camera

Firefox can't show its camera permission prompt inside a toolbar popup. The first time, `qrqrqr` shows an **Open scanner in a tab** button. In that tab, allow the camera and tick **Remember this decision**. After that, the popup can use the camera directly.

## Build

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.100   # must match Cargo.toml
cargo test --release
./scripts/build.sh                                  # -> extension/pkg/
```

Try it in Firefox: open `about:debugging#/runtime/this-firefox`, choose **Load Temporary Add-on…**, and pick `extension/manifest.json`.
Or run `npx web-ext run --source-dir extension`.

To check the built wasm from Node: `npm i --no-save qrcode && node scripts/wasm-smoke-test.mjs`.

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
src/lib.rs                 Rust decoder: RGBA frame -> Option<String> (+ unit tests)
extension/                 the add-on (manifest, popup UI); pkg/ is generated
scripts/build.sh           cargo build + wasm-bindgen (+ wasm-opt if installed)
scripts/wasm-smoke-test.mjs  loads the built wasm in Node and decodes a generated code
amo-metadata.json          AMO listing (summary, category, license, reviewer notes)
.github/workflows/         ci.yml (every push) and release.yml (tags -> AMO)
```

## License

MIT
