# Build the Rust decoder to wasm and lay it out in extension/pkg/.
# Requires: rustup target wasm32-unknown-unknown, wasm-bindgen-cli matching Cargo.toml.
# BUILD_STD=1 is for toolchains without a prebuilt wasm32 std (needs rust-src).

WASM := target/wasm32-unknown-unknown/release/qrqrqr.wasm
PKG  := extension/pkg
SRC  := $(wildcard src/*.rs) Cargo.toml Cargo.lock

CARGOFLAGS := --release --lib --target wasm32-unknown-unknown
ifeq ($(BUILD_STD),1)
export RUSTC_BOOTSTRAP := 1
CARGOFLAGS += -Zbuild-std=std,panic_abort
endif

.PHONY: all wasm test smoke lint package clean bump-minor
all: wasm

wasm: $(PKG)/qrqrqr_bg.wasm

$(WASM): $(SRC)
	cargo build $(CARGOFLAGS)

$(PKG)/qrqrqr_bg.wasm: $(WASM)
	rm -rf $(PKG)
	wasm-bindgen --target web --no-typescript --out-dir $(PKG) $(WASM)
	if command -v wasm-opt >/dev/null; then \
	  wasm-opt -Os --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
	    $@ -o $@; \
	fi
	ls -l $(PKG)

test:
	cargo test --release

smoke: wasm
	npm i --no-save qrcode
	node scripts/wasm-smoke-test.mjs

lint:
	npx --yes web-ext@10 lint --source-dir extension

package: wasm
	npx --yes web-ext@10 build --source-dir extension --artifacts-dir dist --overwrite-dest

clean:
	rm -rf target $(PKG) dist

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
