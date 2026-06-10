import importlib.util
import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]


def load_downloader():
    path = REPO_ROOT / "scripts" / "download-benchmark-data.py"
    spec = importlib.util.spec_from_file_location("download_benchmark_data", path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def test_resolve_allowed_defaults_to_manifest_default_allow():
    dl = load_downloader()
    assert dl.resolve_allowed_license_classes(None, {"default_allow": ["permissive"]}) == {
        "permissive"
    }


def test_resolve_allowed_defaults_to_permissive_without_manifest_hint():
    dl = load_downloader()
    assert dl.resolve_allowed_license_classes(None, {}) == {"permissive"}


def test_resolve_allowed_all_means_no_gate():
    dl = load_downloader()
    assert dl.resolve_allowed_license_classes("all", {}) is None


def test_resolve_allowed_parses_comma_list():
    dl = load_downloader()
    assert dl.resolve_allowed_license_classes("permissive, eval_only", {}) == {
        "permissive",
        "eval_only",
    }


def test_eval_only_dataset_is_blocked_when_only_permissive_allowed(tmp_path):
    dl = load_downloader()
    dataset = {
        "name": "Example",
        "slug": "example",
        "license_class": "eval_only",
        "download": {"kind": "git", "url": "https://example.invalid/repo"},
    }
    result = dl.download_dataset(
        dataset, tmp_path, REPO_ROOT, dl.gb_to_bytes(1), {"permissive"}
    )
    assert result["status"] == "blocked_by_license"
    assert result["license_class"] == "eval_only"
    assert result["bytes_after"] == 0
    # No clone was attempted: the target directory must not exist.
    assert not (tmp_path / "example").exists()


def test_eval_only_dataset_allowed_when_class_permitted(tmp_path):
    dl = load_downloader()
    dataset = {
        "name": "Example",
        "slug": "example",
        "license_class": "eval_only",
        "download": {"kind": "disabled", "reason": "manual"},
    }
    result = dl.download_dataset(
        dataset, tmp_path, REPO_ROOT, dl.gb_to_bytes(1), {"permissive", "eval_only"}
    )
    assert result["status"] == "skipped"
    assert result["license_class"] == "eval_only"


def test_dataset_without_license_class_is_never_blocked(tmp_path):
    dl = load_downloader()
    dataset = {
        "name": "Legacy",
        "slug": "legacy",
        "download": {"kind": "disabled", "reason": "manual"},
    }
    result = dl.download_dataset(
        dataset, tmp_path, REPO_ROOT, dl.gb_to_bytes(1), {"permissive"}
    )
    assert result["status"] == "skipped"
    assert result["license_class"] is None


def test_no_gate_when_allowed_is_none(tmp_path):
    dl = load_downloader()
    dataset = {
        "name": "Example",
        "slug": "example",
        "license_class": "eval_only",
        "download": {"kind": "disabled", "reason": "manual"},
    }
    result = dl.download_dataset(dataset, tmp_path, REPO_ROOT, dl.gb_to_bytes(1), None)
    assert result["status"] == "skipped"


def test_v2_manifest_is_valid_and_classifies_every_dataset():
    manifest = json.loads(
        (REPO_ROOT / "eval" / "datasets" / "document-benchmarks-v2.json").read_text()
    )
    valid_classes = set(manifest["license_classes"].keys())
    valid_kinds = {"git", "hf", "url", "external", "disabled"}
    slugs = set()
    for dataset in manifest["datasets"]:
        assert dataset["license_class"] in valid_classes, dataset["slug"]
        assert dataset["download"]["kind"] in valid_kinds, dataset["slug"]
        assert dataset["slug"] not in slugs, f"duplicate slug {dataset['slug']}"
        slugs.add(dataset["slug"])
    assert {"omnidocbench", "olmocr-bench", "readoc", "pubtables-1m"} <= slugs
