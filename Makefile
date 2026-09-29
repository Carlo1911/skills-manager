# Convenience commands for working on the local fork of skills-manager.
#
# Remotes expected:
#   origin -> https://github.com/xingkongliang/skills-manager.git (upstream)
#   fork   -> https://github.com/Carlo1911/skills-manager.git     (your fork)

SYNC_BRANCH := main
APP_NAME := skills-manager.app
BUNDLE_DIR := src-tauri/target/release/bundle/macos
CLI_BIN := src-tauri/target/release/skills-manager-cli
CLI_LINK := $(HOME)/.local/bin/skills-manager-cli

.PHONY: help sync dev build install cli update check clean

help:
	@echo "Targets:"
	@echo "  make sync     - merge fork and upstream (origin) into $(SYNC_BRANCH), preserving local commits"
	@echo "  make check    - cargo check + tsc --noEmit + eslint"
	@echo "  make dev      - run the app in dev mode (npm run tauri:dev)"
	@echo "  make build    - produce a release .app bundle (npm run tauri:build)"
	@echo "  make install  - copy the built .app into /Applications (overwrites the existing install)"
	@echo "  make cli      - symlink the CLI binary into $(CLI_LINK)"
	@echo "  make update   - sync + build + install + cli, all in one go"
	@echo "  make clean    - remove build artifacts (src-tauri/target)"

sync:
	@test -z "$$(git status --porcelain)" || (echo "Commit or stash local changes (including untracked files) before syncing." && exit 1)
	git fetch origin
	git fetch fork
	git switch $(SYNC_BRANCH)
	git merge --no-edit fork/$(SYNC_BRANCH)
	git merge --no-edit origin/main
	@echo "Synced without rewriting history. After verification, push with: git push fork $(SYNC_BRANCH)"

check:
	cd src-tauri && cargo check
	npx tsc --noEmit -p tsconfig.app.json
	npx eslint .

dev:
	npm run tauri:dev

# Local installs don't need signed updater artifacts or a DMG.
# Disable updater artifact generation explicitly; never hide build failures.
build:
	npm run tauri:build -- --bundles app --config '{"bundle":{"createUpdaterArtifacts":false}}'
	@test -d "$(BUNDLE_DIR)/$(APP_NAME)" || (echo "Build failed: $(APP_NAME) not found in $(BUNDLE_DIR) (a real error, not just the missing updater signing key)" && exit 1)
	@echo "Build produced $(BUNDLE_DIR)/$(APP_NAME)"

install: build
	@pkill -x skills-manager 2>/dev/null || true
	rm -rf "/Applications/$(APP_NAME)"
	cp -R "$(BUNDLE_DIR)/$(APP_NAME)" /Applications/
	@echo "Installed to /Applications/$(APP_NAME)"

cli:
	cargo build --manifest-path src-tauri/Cargo.toml --bin skills-manager-cli --release --locked
	@mkdir -p "$(dir $(CLI_LINK))"
	ln -sf "$(abspath $(CLI_BIN))" "$(CLI_LINK)"
	@echo "Linked $(CLI_LINK) -> $(abspath $(CLI_BIN))"
	@command -v skills-manager-cli >/dev/null || echo "Warning: $(dir $(CLI_LINK)) is not on your PATH"

# Recursive calls keep syncing and installation ordered even with make -j.
update:
	$(MAKE) sync
	npm ci
	$(MAKE) install
	$(MAKE) cli

clean:
	rm -rf src-tauri/target
