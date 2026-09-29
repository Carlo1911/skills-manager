# Convenience commands for working on the local fork of skills-manager.
#
# Remotes expected:
#   origin -> https://github.com/xingkongliang/skills-manager.git (upstream)
#   fork   -> https://github.com/Carlo1911/skills-manager.git     (your fork)

FEATURE_BRANCH := feature/custom-agent-icon-picker
APP_NAME := skills-manager.app
BUNDLE_DIR := src-tauri/target/release/bundle/macos
CLI_BIN := src-tauri/target/release/skills-manager-cli
CLI_LINK := $(HOME)/.local/bin/skills-manager-cli

.PHONY: help sync dev build install cli update check clean

help:
	@echo "Targets:"
	@echo "  make sync     - fetch upstream (origin), fast-forward local main, rebase $(FEATURE_BRANCH) on top"
	@echo "  make check    - cargo check + tsc --noEmit + eslint"
	@echo "  make dev      - run the app in dev mode (npm run tauri:dev)"
	@echo "  make build    - produce a release .app bundle (npm run tauri:build)"
	@echo "  make install  - copy the built .app into /Applications (overwrites the existing install)"
	@echo "  make cli      - symlink the CLI binary into $(CLI_LINK)"
	@echo "  make update   - sync + build + install + cli, all in one go"
	@echo "  make clean    - remove build artifacts (src-tauri/target)"

sync:
	git fetch origin
	git checkout main
	git merge --ff-only origin/main
	git checkout $(FEATURE_BRANCH)
	git rebase main
	@echo "Synced. Push the rebased branch with: git push --force-with-lease fork $(FEATURE_BRANCH)"

check:
	cd src-tauri && cargo check
	npx tsc --noEmit -p tsconfig.app.json
	npx eslint .

dev:
	npm run tauri:dev

# `tauri build` fails at the very last step (updater artifact signing) unless
# TAURI_SIGNING_PRIVATE_KEY is set, which we don't need for a local install.
# Wipe the previous bundle first so a stale one can't masquerade as success,
# then treat "bundle exists" as the real pass/fail signal instead of the exit code.
build:
	rm -rf "$(BUNDLE_DIR)"
	npm run tauri:build || true
	@test -d "$(BUNDLE_DIR)/$(APP_NAME)" || (echo "Build failed: $(APP_NAME) not found in $(BUNDLE_DIR) (a real error, not just the missing updater signing key)" && exit 1)
	@echo "Build produced $(BUNDLE_DIR)/$(APP_NAME)"

install: build
	@pkill -x skills-manager 2>/dev/null || true
	rm -rf "/Applications/$(APP_NAME)"
	cp -R "$(BUNDLE_DIR)/$(APP_NAME)" /Applications/
	@echo "Installed to /Applications/$(APP_NAME)"

cli: build
	@mkdir -p "$(dir $(CLI_LINK))"
	ln -sf "$(abspath $(CLI_BIN))" "$(CLI_LINK)"
	@echo "Linked $(CLI_LINK) -> $(abspath $(CLI_BIN))"
	@command -v skills-manager-cli >/dev/null || echo "Warning: $(dir $(CLI_LINK)) is not on your PATH"

update: sync build install cli

clean:
	rm -rf src-tauri/target
