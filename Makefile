.PHONY: test test-rust test-python test-js test-docs build build-rust build-python build-js build-docs publish-dry-run

test: test-rust test-python test-js test-docs

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

build: build-rust build-python build-js build-docs

build-rust:
	cargo build -p dongler-core -p dongler

build-python:
	uv build
	uv run maturin build

build-js:
	cd node && npm install
	cd node && npm run build

build-docs:
	cd website && npm install
	cd website && NO_UPDATE_NOTIFIER=1 npm run build

publish-dry-run:
	cargo publish --dry-run --allow-dirty -p dongler-core
	@echo "Run cargo publish --dry-run -p dongler after dongler-core 0.1.0 exists on crates.io."
	uv build
	uv run maturin build
	@echo "maturin does not currently support publish --dry-run; use the build artifacts above or TestPyPI for upload validation."
	cd node && npm pack --dry-run
	cd node && npm publish --dry-run
