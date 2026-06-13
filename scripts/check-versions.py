#!/usr/bin/env python3
"""Verify Dongler release versions stay consistent across package surfaces."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - Python 3.10 fallback.
    print("Python 3.11+ is required for tomllib", file=sys.stderr)
    sys.exit(2)


ROOT = Path(__file__).resolve().parents[1]
VERSION_RE = re.compile(r"^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$")
DOC_VERSION_RE = re.compile(r"\b\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?\b")


def read_text(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def read_toml(path: str) -> dict:
    return tomllib.loads(read_text(path))


def read_json(path: str) -> dict:
    return json.loads(read_text(path))


def add_error(errors: list[str], label: str, value: object, expected: str) -> None:
    if value != expected:
        errors.append(f"{label}: expected {expected!r}, found {value!r}")


def package_versions() -> list[tuple[str, str]]:
    return [
        ("Cargo workspace", read_toml("Cargo.toml")["workspace"]["package"]["version"]),
        ("crates/dongler-core", read_toml("crates/dongler-core/Cargo.toml")["package"]["version"]),
        ("crates/dongler-cli", read_toml("crates/dongler-cli/Cargo.toml")["package"]["version"]),
        ("crates/dongler-python", read_toml("crates/dongler-python/Cargo.toml")["package"]["version"]),
        ("crates/dongler-node", read_toml("crates/dongler-node/Cargo.toml")["package"]["version"]),
        ("crates/dongler-wasm", read_toml("crates/dongler-wasm/Cargo.toml")["package"]["version"]),
        ("pyproject.toml", read_toml("pyproject.toml")["project"]["version"]),
        ("node/package.json", read_json("node/package.json")["version"]),
        ("website/package.json", read_json("website/package.json")["version"]),
    ]


def cargo_dependency_versions() -> list[tuple[str, str]]:
    # Every internal path dependency must pin the workspace version — crates.io
    # rejects path deps without a version, and a stale pin breaks `cargo publish`.
    manifests = [
        "crates/dongler-cli/Cargo.toml",
        "crates/dongler-python/Cargo.toml",
        "crates/dongler-node/Cargo.toml",
        "crates/dongler-wasm/Cargo.toml",
        "crates/dongler-pipeline/Cargo.toml",
    ]
    versions = []
    for manifest in manifests:
        dependency = read_toml(manifest)["dependencies"]["dongler-core"]
        versions.append((f"{manifest} dependency dongler-core", dependency["version"]))

    cli_deps = read_toml("crates/dongler-cli/Cargo.toml")["dependencies"]
    versions.append(
        ("crates/dongler-cli dependency dongler-pipeline", cli_deps["dongler-pipeline"]["version"])
    )
    return versions


def cargo_lock_versions() -> list[tuple[str, str | None]]:
    lock = read_toml("Cargo.lock")
    wanted = {
        "dongler",
        "dongler-core",
        "dongler-pipeline",
        "dongler-python",
        "dongler-node",
        "dongler-wasm",
    }
    found = {}
    for package in lock.get("package", []):
        if package.get("name") in wanted:
            found[package["name"]] = package["version"]
    return [(f"Cargo.lock package {name}", found.get(name)) for name in sorted(wanted)]


def uv_lock_versions() -> list[tuple[str, str]]:
    lock = read_toml("uv.lock")
    return [
        ("uv.lock package dongler", package["version"])
        for package in lock.get("package", [])
        if package.get("name") == "dongler"
    ]


def npm_lock_versions(path: str, package_name: str) -> list[tuple[str, str]]:
    lock = read_json(path)
    versions = [
        (f"{path} top-level", lock.get("version")),
        (f"{path} package root", lock.get("packages", {}).get("", {}).get("version")),
    ]
    if lock.get("name") != package_name:
        versions.append((f"{path} package name", lock.get("name")))
    return versions


def documentation_version_mentions() -> list[str]:
    paths = [ROOT / "README.md", ROOT / "llm.txt"]
    paths.extend((ROOT / "docs").glob("*.md"))
    paths.extend((ROOT / "website" / "static").glob("llm*.txt"))

    mentions: list[str] = []
    for path in paths:
        if not path.exists():
            continue
        rel = path.relative_to(ROOT)
        for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if DOC_VERSION_RE.search(line):
                mentions.append(f"{rel}:{line_no}: avoid hardcoded package version text: {line.strip()}")
    return mentions


def main() -> int:
    versions = package_versions()
    expected = versions[0][1]
    errors: list[str] = []

    if not VERSION_RE.match(expected):
        errors.append(f"Cargo workspace version is not a supported release version: {expected!r}")

    for label, version in versions:
        add_error(errors, label, version, expected)

    for label, version in cargo_dependency_versions():
        add_error(errors, label, version, expected)

    for label, version in cargo_lock_versions():
        add_error(errors, label, version, expected)

    uv_versions = uv_lock_versions()
    if not uv_versions:
        errors.append("uv.lock package dongler: package not found")
    for label, version in uv_versions:
        add_error(errors, label, version, expected)

    for label, version in npm_lock_versions("node/package-lock.json", "@cristianexer/dongler"):
        if "package name" in label:
            add_error(errors, label, version, "@cristianexer/dongler")
        else:
            add_error(errors, label, version, expected)

    for label, version in npm_lock_versions("website/package-lock.json", "dongler-docs"):
        if "package name" in label:
            add_error(errors, label, version, "dongler-docs")
        else:
            add_error(errors, label, version, expected)

    errors.extend(documentation_version_mentions())

    if errors:
        print("Version consistency check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(f"All Dongler package versions are consistent at {expected}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
