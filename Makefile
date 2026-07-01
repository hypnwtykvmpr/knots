
COVERAGE_FILE := .ci/coverage-threshold.txt
ARTIFACT_MAX_AGE_HOURS ?= 24
SANITY_TARGET_DIR ?= target/sanity
SANITY_COVERAGE_TARGET_DIR ?= target/sanity-coverage
EXE_SUFFIX := $(if $(filter Windows_NT,$(OS)),.exe,)

.PHONY: fmt lint test coverage sanity reap-artifacts install-hooks check-threshold
.PHONY: loom-bundle demo demo-gif

fmt:
	cargo fmt --all -- --check

ifeq ($(OS),Windows_NT)
POWERSHELL := powershell.exe -NoProfile -ExecutionPolicy Bypass

lint: reap-artifacts
	$(POWERSHELL) -File .\Invoke-LocalChecks.ps1 -SkipTests -SkipCoverage

test: reap-artifacts
	$(POWERSHELL) -File .\Invoke-LocalChecks.ps1 -SkipCoverage

coverage: reap-artifacts
	$(POWERSHELL) -File .\Invoke-LocalChecks.ps1

sanity:
	$(POWERSHELL) -File .\Invoke-LocalChecks.ps1 -Sanity

reap-artifacts:
	$(POWERSHELL) -File .\scripts\repo\Reap-StaleArtifacts.ps1 -MaxAgeHours "$(ARTIFACT_MAX_AGE_HOURS)"

install-hooks:
	$(POWERSHELL) -File .\scripts\repo\Install-Hooks.ps1

check-threshold:
	$(POWERSHELL) -File .\scripts\repo\Check-CoverageThreshold.ps1 -BaseRef origin/main
else
COVERAGE_MIN ?= $(shell tr -d '[:space:]' < $(COVERAGE_FILE))

lint: reap-artifacts
	npm run check-changesets
	CARGO_TARGET_DIR=$(SANITY_TARGET_DIR) cargo check --all-targets --all-features
	CARGO_TARGET_DIR=$(SANITY_TARGET_DIR) cargo clippy --all-targets --all-features -- -D warnings
	bash scripts/repo/check-file-sizes.sh

test: reap-artifacts
	CARGO_TARGET_DIR=$(SANITY_TARGET_DIR) cargo test --all-targets --all-features
	npm run test-release

coverage: reap-artifacts
	@if ! cargo tarpaulin --version >/dev/null 2>&1; then \
	  echo "cargo-tarpaulin is required. Install with: cargo install cargo-tarpaulin --locked"; \
	  exit 1; \
	fi
	mkdir -p coverage
	CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=$(SANITY_COVERAGE_TARGET_DIR) \
	  cargo tarpaulin --engine llvm --all-features \
	  --workspace --timeout 120 --out Xml \
	  --objects "$(SANITY_COVERAGE_TARGET_DIR)/debug/knots$(EXE_SUFFIX)" \
	  --output-dir coverage --fail-under "$(COVERAGE_MIN)"

sanity: fmt lint test coverage

reap-artifacts:
	bash scripts/repo/reap-stale-artifacts.sh "$(ARTIFACT_MAX_AGE_HOURS)"

install-hooks:
	bash scripts/repo/install-hooks.sh

check-threshold:
	bash scripts/repo/check-coverage-threshold.sh origin/main
endif

loom-bundle:
	loom build loom/work_sdlc --emit knots-bundle > loom/work_sdlc/dist/bundle.json

demo:
	@echo 'Run: asciinema rec --overwrite -c "bash scripts/demo.sh" assets/demo.cast'
	@echo 'Then: make demo-gif  (renders assets/demo.gif for inline README playback).'

demo-gif:
	@command -v agg >/dev/null 2>&1 || { \
	  echo "agg not found; install with: brew install agg"; \
	  exit 1; \
	}
	agg assets/demo.cast assets/demo.gif
	@echo "Wrote assets/demo.gif"
