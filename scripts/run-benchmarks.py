#!/usr/bin/env python3
from __future__ import annotations

import argparse
import dataclasses
import hashlib
import json
import os
import re
import subprocess
import time
from pathlib import Path
from typing import Any


START_MARKER = "<!-- BENCHMARKS:START -->"
END_MARKER = "<!-- BENCHMARKS:END -->"


@dataclasses.dataclass
class DocumentSummary:
    pages: int = 0
    blocks: int = 0
    bbox_blocks: int = 0
    anchored_blocks: int = 0
    warnings: int = 0

    @property
    def bbox_block_rate(self) -> float:
        return self.bbox_blocks / self.blocks if self.blocks else 0.0

    @property
    def anchored_block_rate(self) -> float:
        return self.anchored_blocks / self.blocks if self.blocks else 0.0


def summarize_document_json(path: Path) -> DocumentSummary:
    doc = json.loads(path.read_text())
    pages = doc.get("pages", [])
    blocks = 0
    bbox_blocks = 0
    anchored_blocks = 0
    warnings = len(doc.get("warnings", []))
    for page in pages:
        page_blocks = page.get("blocks", [])
        blocks += len(page_blocks)
        warnings += len(page.get("warnings", []))
        for block in page_blocks:
            if block.get("bbox"):
                bbox_blocks += 1
            if block.get("source_anchors"):
                anchored_blocks += 1
    return DocumentSummary(
        pages=len(pages),
        blocks=blocks,
        bbox_blocks=bbox_blocks,
        anchored_blocks=anchored_blocks,
        warnings=warnings,
    )


def replace_readme_benchmark_section(readme: str, table: str) -> str:
    replacement = f"{START_MARKER}\n{table.rstrip()}\n{END_MARKER}"
    pattern = re.compile(f"{re.escape(START_MARKER)}.*?{re.escape(END_MARKER)}", re.S)
    if pattern.search(readme):
        return pattern.sub(replacement, readme)
    return readme.rstrip() + "\n\n## Benchmarks\n\n" + replacement + "\n"


def load_manifest(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text())


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


def discover_pdfs(path: Path) -> list[Path]:
    if not path.exists():
        return []
    return sorted(path.rglob("*.pdf"))


def discover_images(path: Path) -> int:
    if not path.exists():
        return 0
    suffixes = {".png", ".jpg", ".jpeg", ".tif", ".tiff"}
    return sum(1 for item in path.rglob("*") if item.is_file() and item.suffix.lower() in suffixes)


def output_name(dataset_slug: str, pdf: Path) -> str:
    digest = hashlib.sha256(str(pdf).encode()).hexdigest()[:12]
    return f"{dataset_slug}-{pdf.stem}-{digest}.json"


def build_cli(repo_root: Path) -> Path:
    subprocess.run(["cargo", "build", "-q", "-p", "dongler"], cwd=repo_root, check=True)
    executable = "dongler.exe" if os.name == "nt" else "dongler"
    return repo_root / "target" / "debug" / executable


def run_pdf(cli: Path, pdf: Path, json_out: Path) -> tuple[bool, float, str | None]:
    started = time.perf_counter()
    result = subprocess.run(
        [str(cli), "extract", str(pdf), "--format", "json"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    elapsed = max(time.perf_counter() - started, 0.000001)
    if result.returncode != 0:
        return False, elapsed, result.stderr.strip() or result.stdout.strip()
    json_out.write_text(result.stdout)
    return True, elapsed, None


def format_percent(value: float | None) -> str:
    if value is None:
        return "n/a"
    return f"{value * 100:.1f}%"


def format_float(value: float | None) -> str:
    if value is None:
        return "n/a"
    return f"{value:.2f}"


def native_coverage_score(
    parse_success_rate: float | None,
    bbox_block_rate: float | None,
    anchored_block_rate: float | None,
) -> float | None:
    if parse_success_rate is None or bbox_block_rate is None or anchored_block_rate is None:
        return None
    return parse_success_rate * min(bbox_block_rate, anchored_block_rate)


def benchmark_dataset(
    dataset: dict[str, Any],
    data_root: Path,
    out_dir: Path,
    cli: Path,
    max_pdfs: int,
) -> dict[str, Any]:
    slug = dataset["slug"]
    local_dir = data_root / dataset.get("local_dir", slug)
    pdfs = discover_pdfs(local_dir)
    images = discover_images(local_dir)
    selected = pdfs[:max_pdfs]
    result = {
        "dataset": dataset["name"],
        "slug": slug,
        "task": dataset["task"],
        "status": "missing" if not local_dir.exists() else "no_pdfs",
        "local_dir": str(local_dir),
        "local_bytes": directory_size(local_dir),
        "pdfs_found": len(pdfs),
        "pdfs_evaluated": 0,
        "images_found": images,
        "parse_successes": 0,
        "parse_failures": 0,
        "parse_success_rate": None,
        "pages": 0,
        "blocks": 0,
        "bbox_block_rate": None,
        "anchored_block_rate": None,
        "native_coverage_score": None,
        "pages_per_second": None,
        "warnings": 0,
        "ground_truth_accuracy": None,
        "notes": dataset.get("notes", ""),
        "errors": [],
    }

    if not local_dir.exists():
        return result
    if not selected:
        result["status"] = "no_pdfs"
        return result

    dataset_out = out_dir / slug
    dataset_out.mkdir(parents=True, exist_ok=True)
    total_elapsed = 0.0
    total_bbox_blocks = 0
    total_anchored = 0

    for pdf in selected:
        json_out = dataset_out / output_name(slug, pdf)
        ok, elapsed, error = run_pdf(cli, pdf, json_out)
        total_elapsed += elapsed
        result["pdfs_evaluated"] += 1
        if not ok:
            result["parse_failures"] += 1
            result["errors"].append({"pdf": str(pdf), "error": error})
            continue

        result["parse_successes"] += 1
        summary = summarize_document_json(json_out)
        result["pages"] += summary.pages
        result["blocks"] += summary.blocks
        result["warnings"] += summary.warnings
        total_bbox_blocks += summary.bbox_blocks
        total_anchored += summary.anchored_blocks

    result["status"] = "ok" if result["parse_successes"] else "failed"
    result["parse_success_rate"] = result["parse_successes"] / result["pdfs_evaluated"]
    result["bbox_block_rate"] = total_bbox_blocks / result["blocks"] if result["blocks"] else None
    result["anchored_block_rate"] = total_anchored / result["blocks"] if result["blocks"] else None
    result["native_coverage_score"] = native_coverage_score(
        result["parse_success_rate"],
        result["bbox_block_rate"],
        result["anchored_block_rate"],
    )
    result["pages_per_second"] = result["pages"] / total_elapsed if total_elapsed else None
    return result


def markdown_table(results: list[dict[str, Any]], generated_at: str, max_pdfs: int) -> str:
    total_mb = sum(row["local_bytes"] for row in results) / (1024 * 1024)
    lines = [
        f"_Generated by `scripts/run-benchmarks.py` on {generated_at}. "
        f"Local cache represented in this table is {total_mb:.1f} MB; PDF runs are capped at {max_pdfs} PDFs per dataset. "
        "Native score is parse success times the lower of bbox and source-anchor coverage; it is not a ground-truth accuracy. "
        "Ground-truth accuracy is `n/a` when no aligned target is available to this native PDF runner._",
        "",
        "| Dataset | Task | Status | Local data | PDFs found | Images found | PDFs eval | Parse success | BBox coverage | Anchor coverage | Native score | Pages/sec | GT accuracy | Notes |",
        "| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |",
    ]
    for row in results:
        local_mb = row["local_bytes"] / (1024 * 1024)
        note = row["notes"].replace("|", "\\|")
        if len(note) > 100:
            note = note[:97].rstrip() + "..."
        lines.append(
            "| {dataset} | {task} | {status} | {local_mb:.1f} MB | {pdfs_found} | {images_found} | {pdfs_evaluated} | {parse_success} | {bboxes} | {anchors} | {native_score} | {pps} | {accuracy} | {notes} |".format(
                dataset=row["dataset"],
                task=row["task"],
                status=row["status"],
                local_mb=local_mb,
                pdfs_found=row["pdfs_found"],
                images_found=row["images_found"],
                pdfs_evaluated=row["pdfs_evaluated"],
                parse_success=format_percent(row["parse_success_rate"]),
                bboxes=format_percent(row["bbox_block_rate"]),
                anchors=format_percent(row["anchored_block_rate"]),
                native_score=format_percent(row["native_coverage_score"]),
                pps=format_float(row["pages_per_second"]),
                accuracy=format_percent(row["ground_truth_accuracy"]),
                notes=note,
            )
        )
    return "\n".join(lines)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run Dongler local document benchmarks.")
    parser.add_argument("--manifest", default="eval/datasets/document-benchmarks-v1.json")
    parser.add_argument("--data-root", default=os.environ.get("DONGLER_EVAL_DATA_DIR", "eval/data"))
    parser.add_argument("--out-dir", default=os.environ.get("DONGLER_EVAL_OUT_DIR", "eval/out/benchmarks"))
    parser.add_argument("--max-pdfs-per-dataset", type=int)
    parser.add_argument("--dataset", action="append", help="Dataset slug to include; may repeat.")
    parser.add_argument("--update-readme", action="store_true")
    parser.add_argument("--readme", default="README.md")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo_root = Path.cwd()
    manifest = load_manifest(repo_root / args.manifest)
    data_root = repo_root / args.data_root
    out_dir = repo_root / args.out_dir
    out_dir.mkdir(parents=True, exist_ok=True)
    max_pdfs = args.max_pdfs_per_dataset or manifest.get("default_max_pdfs_per_dataset", 50)
    selected = set(args.dataset or [])
    datasets = [
        dataset
        for dataset in manifest["datasets"]
        if not selected or dataset["slug"] in selected
    ]

    cli = build_cli(repo_root)
    generated_at = time.strftime("%Y-%m-%d %H:%M:%S %Z")
    results = [
        benchmark_dataset(dataset, data_root, out_dir, cli, max_pdfs)
        for dataset in datasets
    ]
    payload = {"generated_at": generated_at, "manifest": manifest["name"], "results": results}
    (out_dir / "latest.json").write_text(json.dumps(payload, indent=2, sort_keys=True))
    table = markdown_table(results, generated_at, max_pdfs)
    (out_dir / "latest.md").write_text(table + "\n")
    print(table)

    if args.update_readme:
        readme_path = repo_root / args.readme
        readme_path.write_text(replace_readme_benchmark_section(readme_path.read_text(), table))

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
