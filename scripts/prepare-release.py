#!/usr/bin/env python3
"""Prepare a Dongler release by bumping manifests and refreshing lock files."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VERSION_RE = re.compile(r"^v?(\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?)$")


def normalize_version(raw: str) -> str:
    match = VERSION_RE.match(raw.strip())
    if not match:
        raise ValueError("expected a version like 0.3.4 or v0.3.4")
    return match.group(1)


def replace_once(path: str, pattern: str, replacement: str) -> None:
    file_path = ROOT / path
    text = file_path.read_text(encoding="utf-8")
    updated, count = re.subn(pattern, replacement, text, count=1, flags=re.MULTILINE)
    if count != 1:
        raise RuntimeError(f"could not update {path}; pattern matched {count} times")
    file_path.write_text(updated, encoding="utf-8")


def set_package_version(path: str, version: str) -> None:
    replace_once(path, r'^version = "[^"]+"', f'version = "{version}"')


def set_internal_dependency(path: str, crate: str, version: str) -> None:
    """Bump the version pin of an internal path dependency, e.g.
    `dongler-core = { path = "../dongler-core", version = "X" }`. crates.io
    rejects path deps without a version, so every internal dep must carry one."""
    crate_re = re.escape(crate)
    replace_once(
        path,
        rf'({crate_re} = \{{ path = "\.\./{crate_re}", version = ")[^"]+(")',
        rf"\g<1>{version}\g<2>",
    )


def set_project_version(path: str, version: str) -> None:
    replace_once(path, r'^version = "[^"]+"', f'version = "{version}"')


def set_json_version(path: str, version: str) -> None:
    file_path = ROOT / path
    data = json.loads(file_path.read_text(encoding="utf-8"))
    data["version"] = version
    file_path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def run(command: list[str], *, cwd: Path = ROOT) -> None:
    print(f"$ {' '.join(command)}")
    subprocess.run(command, cwd=cwd, check=True)


def update_manifests(version: str) -> None:
    cargo_manifests = [
        "Cargo.toml",
        "crates/dongler-core/Cargo.toml",
        "crates/dongler-cli/Cargo.toml",
        "crates/dongler-python/Cargo.toml",
        "crates/dongler-node/Cargo.toml",
        "crates/dongler-wasm/Cargo.toml",
    ]
    for manifest in cargo_manifests:
        set_package_version(manifest, version)

    # `dongler-core` is depended on by the CLI, bindings, and the pipeline crate.
    for manifest in [
        "crates/dongler-cli/Cargo.toml",
        "crates/dongler-python/Cargo.toml",
        "crates/dongler-node/Cargo.toml",
        "crates/dongler-wasm/Cargo.toml",
        "crates/dongler-pipeline/Cargo.toml",
    ]:
        set_internal_dependency(manifest, "dongler-core", version)

    # `dongler-pipeline` is depended on by the CLI.
    set_internal_dependency("crates/dongler-cli/Cargo.toml", "dongler-pipeline", version)

    set_project_version("pyproject.toml", version)
    set_json_version("node/package.json", version)
    set_json_version("website/package.json", version)


def refresh_locks() -> None:
    run(["cargo", "update", "-w"])
    run(["uv", "lock"])
    run(["npm", "install", "--package-lock-only", "--ignore-scripts"], cwd=ROOT / "node")
    run(["npm", "install", "--package-lock-only", "--ignore-scripts"], cwd=ROOT / "website")


def print_next_steps(version: str) -> None:
    print()
    print(f"Dongler {version} is prepared locally.")
    print()
    print("Run verification:")
    print("  python3 scripts/check-versions.py")
    print("  cargo test --workspace")
    print("  uv run pytest")
    print("  cd node && npm test")
    print("  cd website && npm run build")
    print("  python3 scripts/check-site-metadata.py website/build")
    print()
    print("Then publish through main:")
    print(
        "  git add .github/workflows/workflow.yml Cargo.toml Cargo.lock "
        "pyproject.toml uv.lock README.md docs node website python scripts llm.txt"
    )
    print(f'  git commit -m "Release {version}"')
    print("  git push origin main")


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: scripts/prepare-release.py <version>", file=sys.stderr)
        return 2

    try:
        version = normalize_version(argv[1])
        update_manifests(version)
        refresh_locks()
        run(["python3", "scripts/check-versions.py"])
    except (ValueError, RuntimeError, subprocess.CalledProcessError) as exc:
        print(f"prepare-release failed: {exc}", file=sys.stderr)
        return 1

    print_next_steps(version)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
