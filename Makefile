.PHONY: test test-rust test-python test-js test-docs test-release test-wasm build build-rust build-python build-js build-docs build-wasm eval-data eval-smoke bench-data bench-run bench publish-dry-run

test: test-release test-rust test-python test-js test-docs test-wasm

test-release:
	python3 scripts/check-versions.py

test-wasm:
	rustup target add wasm32-unknown-unknown
	cargo build -p dongler-wasm --target wasm32-unknown-unknown

test-rust:
	cargo test --workspace

test-python:
	uv sync --dev
	uv run maturin develop
	uv run pytest

test-js:
	cd node && npm install
	cd node && npm test

test-docs:
	cd website && npm install
	cd website && npm audit --audit-level=high
	cd website && NO_UPDATE_NOTIFIER=1 npm run build
	python3 scripts/check-site-metadata.py website/build

build: build-rust build-python build-js build-docs build-wasm

build-rust:
	cargo build -p dongler-core -p dongler

build-wasm:
	scripts/build-wasm.sh

build-python:
	uv build
	uv run maturin build

build-js:
	cd node && npm install
	cd node && npm run build

build-docs:
	cd website && npm install
	cd website && NO_UPDATE_NOTIFIER=1 npm run build

eval-data:
	scripts/eval-data.sh all

eval-smoke:
	@test -n "$(PDF)" || (echo "usage: make eval-smoke PDF=path/to/file.pdf" >&2; exit 2)
	scripts/eval-smoke.sh "$(PDF)"

bench-data:
	python3 scripts/download-benchmark-data.py

bench-run:
	python3 scripts/run-benchmarks.py --update-readme

bench: bench-data bench-run

publish-dry-run:
	cargo publish --dry-run --allow-dirty -p dongler-core
	@echo "Run cargo publish --dry-run -p dongler after dongler-core is visible on crates.io."
	uv build
	uv run maturin build
	@echo "maturin does not currently support publish --dry-run; use the build artifacts above or TestPyPI for upload validation."
	cd node && npm pack --dry-run
	cd node && npm publish --dry-run
