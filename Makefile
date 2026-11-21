# Build the Rust decoder to wasm and lay it out in extension/pkg/.
# Requires: rustup target wasm32-unknown-unknown, wasm-bindgen-cli matching Cargo.toml.
# wasm-opt (binaryen) is optional: `sudo apt install binaryen`. Do not use --all-features:
# it emits modules Firefox/Node reject.
# BUILD_STD=1 is for toolchains without a prebuilt wasm32 std (needs rust-src).

WASM := target/wasm32-unknown-unknown/release/qrqrqr.wasm
PKG  := extension/pkg
SRC  := $(wildcard src/*.rs) Cargo.toml Cargo.lock

CARGOFLAGS := --release --lib --target wasm32-unknown-unknown
ifeq ($(BUILD_STD),1)
export RUSTC_BOOTSTRAP := 1
CARGOFLAGS += -Zbuild-std=std,panic_abort
endif

.PHONY: all wasm models test smoke e2e lint package clean bump-minor
all: wasm models

# OCR model (CC-BY-SA 4.0, https://github.com/robertknight/ocrs-models), fetched at *build*
# time, checksum-verified, and shipped inside the extension. They are never downloaded at runtime.
MODELS_URL := https://huggingface.co/robertknight/ocrs/resolve/main
MODEL_DIR  := extension/models
REC_SRC := text-rec-checkpoint-s52qdbqt.rten
REC_SHA := 606d9a0414c6b73c99df75b707c11c70d1c8b12e1d4f900922e185fc37bfca65

models: $(MODEL_DIR)/text-recognition.rten

$(MODEL_DIR)/text-recognition.rten:
	$(call fetch,$(REC_SRC),$(REC_SHA),$@)

define fetch
	mkdir -p $(MODEL_DIR)
	curl -fsSL -o $(3).tmp $(MODELS_URL)/$(1)
	echo "$(2)  $(3).tmp" | sha256sum -c --quiet
	mv $(3).tmp $(3)
endef

wasm: $(PKG)/qrqrqr_bg.wasm

$(WASM): $(SRC)
	cargo build $(CARGOFLAGS)

$(PKG)/qrqrqr_bg.wasm: $(WASM)
	rm -rf $(PKG)
	wasm-bindgen --target web --no-typescript --out-dir $(PKG) $(WASM)
	if command -v wasm-opt >/dev/null; then \
	  wasm-opt -Os --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-simd --enable-relaxed-simd \
	    --enable-mutable-globals --enable-multivalue --enable-reference-types \
	    $@ -o $@; \
	fi
	ls -lh $(PKG)

test: models
	cargo test --release

smoke: wasm models
	npm i --no-save qrcode pngjs
	node scripts/wasm-smoke-test.mjs

# Loads the built extension in a real Firefox (headless, via geckodriver) with a still image as
# the camera, and checks the scan list. Needs Firefox and Node; geckodriver is fetched by Selenium.
e2e: wasm models
	npm i --no-save selenium-webdriver pngjs
	node scripts/firefox-e2e.mjs

lint:
	npx --yes web-ext@10 lint --source-dir extension

package: wasm models
	npx --yes web-ext@10 build --source-dir extension --artifacts-dir dist --overwrite-dest

clean:
	rm -rf target $(PKG) $(MODEL_DIR) dist

# Bump the minor version (X.Y.Z -> X.(Y+1).0) in the manifest, Cargo.toml and Cargo.lock,
# then print the command that tags and pushes the release (the tag triggers release.yml).
bump-minor:
	@cur=$$(sed -n 's/^ *"version": *"\([0-9.]*\)".*/\1/p' extension/manifest.json | head -1); \
	new=$$(echo $$cur | awk -F. '{printf "%d.%d.0", $$1, $$2+1}'); \
	[ -n "$$cur" ] || { echo "version not found in extension/manifest.json" >&2; exit 1; }; \
	sed -i "s/^\( *\"version\": *\)\"$$cur\"/\1\"$$new\"/" extension/manifest.json; \
	sed -i "0,/^version = \"$$cur\"/s//version = \"$$new\"/" Cargo.toml; \
	sed -i "/^name = \"qrqrqr\"/{n;s/^version = .*/version = \"$$new\"/}" Cargo.lock; \
	echo "$$cur -> $$new"; \
	echo; \
	echo "Review with 'git diff', then release with:"; \
	echo "  git commit -am 'v$$new' && git tag v$$new && git push --follow-tags"
