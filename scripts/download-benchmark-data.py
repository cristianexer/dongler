#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import urllib.request
import zipfile
from pathlib import Path
from typing import Any


def directory_size(path: Path) -> int:
    if not path.exists():
        return 0
    if path.is_file():
        return path.stat().st_size
    total = 0
    for item in path.rglob("*"):
        if item.is_file() and not item.is_symlink():
            total += item.stat().st_size
    return total


def gb_to_bytes(value: float) -> int:
    return int(value * 1024 * 1024 * 1024)


def load_manifest(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text())


def ensure_budget(root: Path, budget_bytes: int) -> None:
    used = directory_size(root)
    if used > budget_bytes:
        raise RuntimeError(
            f"benchmark data uses {used / 1024**3:.2f}GB, above the configured "
            f"{budget_bytes / 1024**3:.2f}GB budget"
        )


def run(command: list[str], cwd: Path) -> None:
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, cwd=cwd, check=True)


def download_git(dataset: dict[str, Any], target: Path, repo_root: Path) -> None:
    url = dataset["download"]["url"]
    if (target / ".git").exists():
        run(["git", "-C", str(target), "pull", "--ff-only"], repo_root)
    elif target.exists() and any(target.iterdir()):
        print(f"Keeping existing non-git directory {target}")
    else:
        target.parent.mkdir(parents=True, exist_ok=True)
        run(["git", "clone", "--depth", "1", url, str(target)], repo_root)


def download_hf(dataset: dict[str, Any], target: Path, repo_root: Path) -> None:
    hf_command = ["hf"]
    if shutil.which("hf") is None:
        if shutil.which("uvx") is None:
            raise RuntimeError("hf CLI is required: install huggingface_hub or uvx")
        hf_command = ["uvx", "--from", "huggingface-hub", "hf"]
    spec = dataset["download"]
    command = [
        *hf_command,
        "download",
        spec["repo"],
        "--repo-type",
        "dataset",
        "--local-dir",
        str(target),
    ]
    for pattern in spec.get("include", []):
        command.extend(["--include", pattern])
    target.parent.mkdir(parents=True, exist_ok=True)
    run(command, repo_root)


def download_url(dataset: dict[str, Any], target: Path) -> None:
    spec = dataset["download"]
    target.mkdir(parents=True, exist_ok=True)
    archive = target / spec.get("filename", Path(spec["url"]).name)
    if not archive.exists():
        print(f"+ download {spec['url']} -> {archive}", flush=True)
        urllib.request.urlretrieve(spec["url"], archive)
    if archive.suffix == ".zip":
        marker = target / ".unzipped"
        if not marker.exists():
            with zipfile.ZipFile(archive) as handle:
                handle.extractall(target)
            marker.write_text("ok\n")


def download_dataset(dataset: dict[str, Any], data_root: Path, repo_root: Path, budget_bytes: int) -> dict[str, Any]:
    target = data_root / dataset.get("local_dir", dataset["slug"])
    spec = dataset["download"]
    before = directory_size(target)
    status = "skipped"
    error = None

    try:
        if spec["kind"] == "git":
            download_git(dataset, target, repo_root)
            status = "downloaded"
        elif spec["kind"] == "hf":
            download_hf(dataset, target, repo_root)
            status = "downloaded"
        elif spec["kind"] == "url":
            download_url(dataset, target)
            status = "downloaded"
        elif spec["kind"] == "external":
            env_name = spec["env"]
            if env_name not in os.environ:
                status = "skipped"
                error = f"{env_name} is not set"
            else:
                target.mkdir(parents=True, exist_ok=True)
                run(["azcopy", "copy", os.environ[env_name], str(target), "--recursive=true"], repo_root)
                status = "downloaded"
        elif spec["kind"] == "disabled":
            status = "skipped"
            error = spec.get("reason")
        else:
            raise RuntimeError(f"unknown download kind: {spec['kind']}")
        ensure_budget(data_root, budget_bytes)
        max_bytes = spec.get("max_bytes")
        if max_bytes is not None and directory_size(target) > max_bytes:
            raise RuntimeError(
                f"{dataset['name']} local data exceeds per-dataset cap "
                f"({directory_size(target) / 1024**3:.2f}GB > {max_bytes / 1024**3:.2f}GB)"
            )
    except Exception as exc:
        status = "error"
        error = str(exc)

    after = directory_size(target)
    return {
        "dataset": dataset["name"],
        "slug": dataset["slug"],
        "status": status,
        "bytes_before": before,
        "bytes_after": after,
        "error": error,
        "local_dir": str(target),
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Download bounded local benchmark data.")
    parser.add_argument("datasets", nargs="*", help="Dataset slugs to download; default uses safe local subset.")
    parser.add_argument("--manifest", default="eval/datasets/document-benchmarks-v1.json")
    parser.add_argument("--data-root", default=os.environ.get("DONGLER_EVAL_DATA_DIR", "eval/data"))
    parser.add_argument("--budget-gb", type=float, default=float(os.environ.get("DONGLER_DATA_BUDGET_GB", "100")))
    parser.add_argument("--strict", action="store_true", help="Exit nonzero if any selected dataset fails.")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo_root = Path.cwd()
    manifest = load_manifest(repo_root / args.manifest)
    data_root = repo_root / args.data_root
    data_root.mkdir(parents=True, exist_ok=True)
    budget_bytes = gb_to_bytes(args.budget_gb)
    selected = set(args.datasets)
    default_slugs = {
        "docbank",
        "tablebank",
        "funsd",
        "sroie",
        "omnidocbench",
        "olmocr-bench",
        "ckorzen",
    }
    datasets = [
        dataset
        for dataset in manifest["datasets"]
        if (dataset["slug"] in selected if selected else dataset["slug"] in default_slugs)
    ]
    known_slugs = {dataset["slug"] for dataset in manifest["datasets"]}
    unknown_slugs = selected - known_slugs
    if unknown_slugs:
        print(f"unknown dataset slug(s): {', '.join(sorted(unknown_slugs))}", file=sys.stderr)
        return 2
    results = [
        download_dataset(dataset, data_root, repo_root, budget_bytes)
        for dataset in datasets
    ]
    out_dir = repo_root / "eval" / "out" / "benchmarks"
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "download-latest.json").write_text(json.dumps(results, indent=2, sort_keys=True))
    print(json.dumps(results, indent=2, sort_keys=True))
    if args.strict and any(result["status"] == "error" for result in results):
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
