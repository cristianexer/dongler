#!/usr/bin/env python3
"""Validate generated documentation sitemap and robots metadata."""

from __future__ import annotations

import sys
import xml.etree.ElementTree as ET
from pathlib import Path


SITE_URL = "https://cristianexer.github.io/dongler"
REQUIRED_URLS = {
    f"{SITE_URL}/",
    f"{SITE_URL}/docs/intro",
    f"{SITE_URL}/docs/quickstart",
    f"{SITE_URL}/docs/developer-guide",
    f"{SITE_URL}/docs/pdf-workflow",
    f"{SITE_URL}/docs/batch-processing",
    f"{SITE_URL}/docs/api",
    f"{SITE_URL}/docs/evals",
    f"{SITE_URL}/docs/bindings",
    f"{SITE_URL}/docs/cli",
}


def sitemap_urls(path: Path) -> set[str]:
    tree = ET.parse(path)
    root = tree.getroot()
    namespace = ""
    if root.tag.startswith("{"):
        namespace = root.tag.split("}", 1)[0] + "}"
    return {
        loc.text.strip()
        for loc in root.findall(f".//{namespace}loc")
        if loc.text and loc.text.strip()
    }


def main(argv: list[str]) -> int:
    build_dir = Path(argv[1]) if len(argv) > 1 else Path("website/build")
    sitemap = build_dir / "sitemap.xml"
    robots = build_dir / "robots.txt"
    errors: list[str] = []

    if not sitemap.exists():
        errors.append(f"{sitemap} does not exist")
        urls: set[str] = set()
    else:
        try:
            urls = sitemap_urls(sitemap)
        except ET.ParseError as exc:
            errors.append(f"{sitemap} is not valid XML: {exc}")
            urls = set()

    if not robots.exists():
        errors.append(f"{robots} does not exist")
    else:
        robots_text = robots.read_text(encoding="utf-8")
        expected = f"Sitemap: {SITE_URL}/sitemap.xml"
        if expected not in robots_text:
            errors.append(f"{robots} must contain {expected!r}")

    for url in sorted(urls):
        if not url.startswith(f"{SITE_URL}/"):
            errors.append(f"sitemap URL is outside the Dongler site prefix: {url}")

    missing = REQUIRED_URLS - urls
    for url in sorted(missing):
        errors.append(f"sitemap missing required URL: {url}")

    if errors:
        print("Site metadata check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(f"Site metadata is valid: {len(urls)} sitemap URLs under {SITE_URL}/")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
