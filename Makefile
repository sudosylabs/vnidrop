ROOT := $(patsubst %/,%,$(dir $(abspath $(lastword $(MAKEFILE_LIST)))))

SHELL := bash
.SHELLFLAGS := -eu -o pipefail -c
.DEFAULT_GOAL := help

include $(ROOT)/config.mk
-include $(ROOT)/config.override.mk
include $(ROOT)/make/release.mk

.PHONY: help doctor setup setup-localization setup-docs setup-diagnostics
.PHONY: format test check check-rust audit-rust test-rust test-rust-all
.PHONY: test-rust-transfer test-rust-approval test-rust-lifecycle test-rust-output-sink
.PHONY: check-shared test-shared test-android-host check-android verify-android-libs build-android run-desktop
.PHONY: apple-core apple-project open-apple-project open-apple build-apple-macos build-apple-ios check-apple
.PHONY: check-localization localization localization-migrate
.PHONY: check-docs run-docs check-diagnostics run-diagnostics diagnostics-db-local diagnostics-db-remote diagnostics-typegen deploy-diagnostics

help: ## Show available commands and common configuration variables.
	@grep -hE '^[A-Za-z0-9_.-]+:.*## ' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*## "} {printf "  %-28s %s\n", $$1, $$2}'
	@printf '\nCommon variables:\n'
	@printf '  %-28s %s\n' 'VERSION=x.y.z' 'Package version (default: $(VERSION))'
	@printf '  %-28s %s\n' 'APPLE_PROFILE=debug|release' 'Rust profile for the Apple XCFramework'
	@printf '  %-28s %s\n' 'APPLE_CONFIGURATION=...' 'Xcode configuration (default: $(APPLE_CONFIGURATION))'
	@printf '  %-28s %s\n' 'APPLE_DESTINATION=...' 'Optional xcodebuild destination override'
	@printf '  %-28s %s\n' 'APPLE_CODE_SIGNING=NO|YES' 'Enable Apple code signing (default: $(APPLE_CODE_SIGNING))'

doctor: ## Check that tools required by the current host are available.
	@missing=0; \
	for tool in "$(firstword $(CARGO))" java "$(firstword $(NPM))" "$(firstword $(BUN))"; do \
		if command -v "$$tool" >/dev/null 2>&1; then \
			printf 'ok      %s\n' "$$tool"; \
		else \
			printf 'missing %s\n' "$$tool"; \
			missing=1; \
		fi; \
	done; \
	if [[ ! -f "$(GRADLE)" ]]; then printf 'missing %s\n' "$(GRADLE)"; missing=1; else printf 'ok      %s\n' "$(GRADLE)"; fi; \
	if [[ "$(HOST_OS)" == macos ]]; then \
		for tool in "$(firstword $(XCODEBUILD))" "$(firstword $(XCODEGEN))"; do \
			if command -v "$$tool" >/dev/null 2>&1; then printf 'ok      %s\n' "$$tool"; else printf 'missing %s\n' "$$tool"; missing=1; fi; \
		done; \
	fi; \
	exit $$missing

setup: setup-localization setup-docs setup-diagnostics ## Install repository-local JavaScript dependencies.

setup-localization: ## Install localization CLI dependencies with Bun.
	cd $(ROOT)/localization && $(BUN) install --frozen-lockfile

setup-docs: ## Install documentation website dependencies.
	cd $(ROOT)/docs && $(NPM) ci

setup-diagnostics: ## Install diagnostics Worker dependencies.
	cd $(ROOT)/services/diagnostics-api && $(NPM) ci

format: ## Format Rust sources.
	cd $(ROOT) && $(CARGO) fmt --all

test: test-rust test-shared ## Run the main Rust and shared JVM test suites.

check: check-rust check-shared check-localization check-docs check-diagnostics ## Run portable pre-PR verification.

check-rust: ## Run Rust formatting, lint, tests, and documentation checks.
	cd $(ROOT) && $(CARGO) fmt --all -- --check
	cd $(ROOT) && $(CARGO) clippy --workspace --all-targets -- -D warnings
	cd $(ROOT) && $(CARGO) test --workspace --all-targets
	cd $(ROOT) && RUSTDOCFLAGS='-D warnings' $(CARGO) doc --workspace --no-deps

audit-rust: ## Audit Rust dependencies (requires cargo-audit).
	cd $(ROOT) && $(CARGO) audit

test-rust: ## Run the focused Rust core suite.
	cd $(ROOT) && $(CARGO) test -p vnidrop

test-rust-all: ## Run every Rust workspace test target.
	cd $(ROOT) && $(CARGO) test --workspace --all-targets

test-rust-transfer: ## Run Rust transfer integration tests.
	cd $(ROOT) && $(CARGO) test -p vnidrop --test transfer

test-rust-approval: ## Run Rust approval integration tests.
	cd $(ROOT) && $(CARGO) test -p vnidrop --test approval

test-rust-lifecycle: ## Run Rust lifecycle integration tests.
	cd $(ROOT) && $(CARGO) test -p vnidrop --test lifecycle

test-rust-output-sink: ## Run Rust output-sink integration tests.
	cd $(ROOT) && $(CARGO) test -p vnidrop --test output_sink

check-shared: ## Test and compile the shared Android/JVM module.
	cd $(ROOT) && $(GRADLE) :shared:jvmTest :shared:compileKotlinJvm $(GRADLE_FLAGS)

test-shared: ## Run shared JVM tests.
	cd $(ROOT) && $(GRADLE) :shared:jvmTest $(GRADLE_FLAGS)

test-android-host: ## Run Android host-side shared tests.
	cd $(ROOT) && $(GRADLE) :shared:testAndroidHostTest $(GRADLE_FLAGS)

check-android: ## Build Android debug and verify packaged Rust libraries.
	cd $(ROOT) && $(GRADLE) :androidApp:assembleDebug :androidApp:verifyDebugVnidropLibraries $(GRADLE_FLAGS)

verify-android-libs: ## Verify the Rust libraries packaged in the Android debug app.
	cd $(ROOT) && $(GRADLE) :androidApp:verifyDebugVnidropLibraries $(GRADLE_FLAGS)

build-android: ## Build the Android debug APK.
	cd $(ROOT) && $(GRADLE) :androidApp:assembleDebug $(GRADLE_FLAGS)

run-desktop: ## Run the Windows/Linux Compose desktop app.
	cd $(ROOT) && $(GRADLE) :desktopApp:run $(GRADLE_FLAGS)

apple-core: ## Build the Rust XCFramework and generated Swift bindings.
	@test "$(HOST_OS)" = macos || { printf 'Apple builds require macOS.\n' >&2; exit 1; }
	cd $(ROOT) && apple/scripts/build-core.sh $(APPLE_PROFILE)

apple-project: apple-core localization ## Generate the native Apple Xcode project.
	cd $(ROOT)/apple && $(XCODEGEN) generate

open-apple-project: apple-project ## Generate and open the native Apple Xcode project.
	cd $(ROOT)/apple && $(OPEN) VniDrop.xcodeproj

build-apple-macos: apple-project ## Build the native macOS app (unsigned by default).
	cd $(ROOT)/apple && $(XCODEBUILD) -project VniDrop.xcodeproj -scheme VniDrop -configuration $(APPLE_CONFIGURATION) -derivedDataPath "$(APPLE_DERIVED_DATA)" -destination 'platform=macOS' CODE_SIGNING_ALLOWED=$(APPLE_CODE_SIGNING) CODE_SIGNING_REQUIRED=$(APPLE_CODE_SIGNING) build

open-apple: build-apple-macos ## Build and launch the native macOS app.
	@test -d "$(APPLE_DERIVED_DATA)/Build/Products/$(APPLE_CONFIGURATION)/VniDrop.app" || { printf 'Built macOS app was not found.\n' >&2; exit 1; }
	$(OPEN) "$(APPLE_DERIVED_DATA)/Build/Products/$(APPLE_CONFIGURATION)/VniDrop.app"

build-apple-ios: apple-project ## Build the native iOS simulator app (unsigned by default).
	@destination="$(APPLE_DESTINATION)"; \
	if [[ -z "$$destination" ]]; then \
		device_id="$$(xcrun simctl list devices available | sed -nE '/iPhone/ s/.*\(([0-9A-F-]{36})\) \((Booted|Shutdown)\).*/\1/p' | head -1 || true)"; \
		[[ -n "$$device_id" ]] || { printf 'No available iPhone simulator found. Set APPLE_DESTINATION explicitly.\n' >&2; exit 1; }; \
		destination="platform=iOS Simulator,id=$$device_id"; \
	fi; \
	cd $(ROOT)/apple && $(XCODEBUILD) -project VniDrop.xcodeproj -scheme VniDrop -configuration $(APPLE_CONFIGURATION) -derivedDataPath "$(APPLE_DERIVED_DATA)" -destination "$$destination" CODE_SIGNING_ALLOWED=$(APPLE_CODE_SIGNING) CODE_SIGNING_REQUIRED=$(APPLE_CODE_SIGNING) build

check-apple: apple-project ## Build the Apple core and run iOS simulator tests.
	@destination="$(APPLE_DESTINATION)"; \
	if [[ -z "$$destination" ]]; then \
		device_id="$$(xcrun simctl list devices available | sed -nE '/iPhone/ s/.*\(([0-9A-F-]{36})\) \((Booted|Shutdown)\).*/\1/p' | head -1 || true)"; \
		[[ -n "$$device_id" ]] || { printf 'No available iPhone simulator found. Set APPLE_DESTINATION explicitly.\n' >&2; exit 1; }; \
		destination="platform=iOS Simulator,id=$$device_id"; \
	fi; \
	printf 'Testing on: %s\n' "$$destination"; \
	cd $(ROOT)/apple && $(XCODEBUILD) test -project VniDrop.xcodeproj -scheme VniDrop -configuration $(APPLE_CONFIGURATION) -derivedDataPath "$(APPLE_DERIVED_DATA)" -destination "$$destination" CODE_SIGNING_ALLOWED=$(APPLE_CODE_SIGNING) CODE_SIGNING_REQUIRED=$(APPLE_CODE_SIGNING)

check-localization: setup-localization ## Validate the localization source catalog.
	cd $(ROOT)/localization && $(BUN) run validate

localization: setup-localization ## Regenerate Apple and KMP localization resources.
	cd $(ROOT)/localization && $(BUN) run generate

localization-migrate: setup-localization ## Rebuild strings.json from platform resources.
	cd $(ROOT)/localization && $(BUN) run migrate

check-docs: setup-docs ## Lint, type-check, and build the documentation website.
	cd $(ROOT)/docs && $(NPM) run lint
	cd $(ROOT)/docs && $(NPM) run typecheck
	cd $(ROOT)/docs && $(NPM) run build

run-docs: setup-docs ## Run the documentation development server.
	cd $(ROOT)/docs && $(NPM) run dev

check-diagnostics: setup-diagnostics ## Run diagnostics types, tests, and deployment dry-run.
	cd $(ROOT)/services/diagnostics-api && $(NPM) run check

run-diagnostics: setup-diagnostics ## Run the diagnostics Worker locally.
	cd $(ROOT)/services/diagnostics-api && $(NPM) run dev

diagnostics-db-local: setup-diagnostics ## Apply diagnostics database migrations locally.
	cd $(ROOT)/services/diagnostics-api && $(NPM) run db:migrate:local

diagnostics-db-remote: setup-diagnostics ## Apply diagnostics database migrations to the configured remote D1 database.
	cd $(ROOT)/services/diagnostics-api && $(NPM) run db:migrate:remote

diagnostics-typegen: setup-diagnostics ## Regenerate diagnostics Worker binding types.
	cd $(ROOT)/services/diagnostics-api && $(NPM) run typegen

deploy-diagnostics: setup-diagnostics ## Check and deploy the diagnostics Worker to Cloudflare.
	cd $(ROOT)/services/diagnostics-api && $(NPM) run deploy
