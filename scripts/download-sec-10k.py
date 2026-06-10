#!/usr/bin/env python3
"""Build a diverse SEC 10-K PDF corpus for extraction evaluation.

SEC EDGAR serves modern 10-K filings as HTML/iXBRL (no PDF rendition), so this
script downloads each filing's primary HTML document plus its referenced raster
assets and renders the result to a born-digital, text-based PDF with headless
Chrome. The PDFs are exactly the kind of digitally-born document dongler targets
(no OCR), and 10-Ks are extremely table-dense, which makes them a hard,
general-purpose stress test for the extractor.

The corpus is curated for *diversity* (market cap, sector, table style) rather
than completeness: ~150 filings spread across an explicit sector map plus a
random small/mid-cap tail. Nothing here is 10-K specific on dongler's side; this
script only produces ordinary PDFs.

Output layout (under --out, default eval/data/sec-10k/, gitignored):
    html/<ticker>/...      downloaded primary doc + image assets
    pdfs/<ticker>.pdf      rendered PDF
    meta/<ticker>.json     per-filing metadata
    manifest.json          index of every successful filing

EDGAR fair-access: a descriptive User-Agent is required and requests are rate
limited. See https://www.sec.gov/os/accessing-edgar-data .
"""
from __future__ import annotations

import argparse
import json
import os
import random
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

EDGAR_TICKERS = "https://www.sec.gov/files/company_tickers.json"
EDGAR_SUBMISSIONS = "https://data.sec.gov/submissions/CIK{cik:010d}.json"
EDGAR_ARCHIVE = "https://www.sec.gov/Archives/edgar/data/{cik}/{acc}"
RASTER_SUFFIXES = (".jpg", ".jpeg", ".png", ".gif", ".bmp")

DEFAULT_UA = os.environ.get("SEC_USER_AGENT", "dongler-eval dcristian353@gmail.com")
DEFAULT_CHROME = os.environ.get(
    "CHROME_BIN", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
)

# Curated diversity map: spread across sectors *and* market caps so the corpus
# exercises many table layouts (dense financial statements, footnote schedules,
# MD&A grids) and font/encoding setups. Tickers are resolved to CIKs at runtime.
SECTOR_TICKERS: dict[str, list[str]] = {
    "tech_megacap": ["AAPL", "MSFT", "NVDA", "GOOGL", "AMZN", "META", "ORCL", "CRM", "ADBE", "CSCO"],
    "tech_midcap": ["AMD", "INTC", "IBM", "HPQ", "DELL", "NET", "DDOG", "SNOW", "PLTR", "TWLO"],
    "finance": ["JPM", "BAC", "WFC", "GS", "MS", "C", "AXP", "BLK", "SCHW", "USB"],
    "insurance": ["BRK-B", "MET", "PRU", "AIG", "TRV", "ALL", "PGR", "CB"],
    "energy": ["XOM", "CVX", "COP", "SLB", "OXY", "KMI", "PSX", "VLO", "HAL", "DVN"],
    "health_pharma": ["JNJ", "PFE", "MRK", "ABBV", "LLY", "GILD", "AMGN", "BMY", "MRNA", "REGN"],
    "health_services": ["UNH", "CVS", "CI", "HUM", "HCA", "MCK", "CNC"],
    "retail_consumer": ["WMT", "COST", "HD", "TGT", "NKE", "SBUX", "MCD", "KO", "PEP", "PG", "LOW", "DG"],
    "industrials": ["BA", "CAT", "GE", "HON", "UPS", "LMT", "DE", "MMM", "RTX", "EMR"],
    "telecom_media": ["T", "VZ", "DIS", "CMCSA", "NFLX", "WBD", "TMUS", "PARA"],
    "reit": ["AMT", "PLD", "SPG", "O", "EQIX", "PSA", "WELL", "VICI"],
    "utilities": ["NEE", "DUK", "SO", "AEP", "EXC", "XEL", "ED"],
    "materials": ["LIN", "APD", "NEM", "FCX", "DOW", "DD", "NUE", "SHW"],
    "autos_transport": ["TSLA", "F", "GM", "DAL", "UAL", "AAL", "LUV", "RIVN", "LCID"],
    "semis_hardware": ["AVGO", "QCOM", "TXN", "MU", "AMAT", "LRCX", "KLAC", "ON"],
    "smallcap_misc": ["GME", "AMC", "BBWI", "WOLF", "PLUG", "FUBO", "RUN", "SOFI", "CHPT", "OPEN"],
}


def http_get(url: str, ua: str, retries: int = 4, timeout: int = 60) -> bytes:
    last: Exception | None = None
    for attempt in range(retries):
        req = urllib.request.Request(url, headers={"User-Agent": ua, "Accept-Encoding": "gzip, deflate"})
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                data = resp.read()
                if resp.headers.get("Content-Encoding") == "gzip":
                    import gzip

                    data = gzip.decompress(data)
                return data
        except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError) as exc:
            last = exc
            time.sleep(1.5 * (attempt + 1))
    raise RuntimeError(f"GET {url} failed: {last}")


def load_ticker_map(ua: str, cache: Path) -> dict[str, dict[str, Any]]:
    if not cache.exists():
        cache.parent.mkdir(parents=True, exist_ok=True)
        cache.write_bytes(http_get(EDGAR_TICKERS, ua))
    raw = json.loads(cache.read_text())
    out: dict[str, dict[str, Any]] = {}
    for entry in raw.values():
        out[entry["ticker"].upper()] = {"cik": int(entry["cik_str"]), "title": entry["title"]}
    return out


def find_latest_10k(cik: int, ua: str, throttle: float, years: tuple[str, ...]) -> dict[str, Any] | None:
    data = json.loads(http_get(EDGAR_SUBMISSIONS.format(cik=cik), ua))
    time.sleep(throttle)
    recent = data["filings"]["recent"]
    cols = ["accessionNumber", "form", "filingDate", "primaryDocument", "primaryDocDescription"]
    for acc, form, fdate, doc, _desc in zip(*[recent[c] for c in cols]):
        if form == "10-K" and fdate[:4] in years and doc.lower().endswith((".htm", ".html")):
            return {"accession": acc, "filing_date": fdate, "primary_doc": doc, "company": data.get("name", "")}
    return None


def download_filing_assets(cik: int, filing: dict[str, Any], dest: Path, ua: str, throttle: float) -> Path:
    acc_nodash = filing["accession"].replace("-", "")
    base = EDGAR_ARCHIVE.format(cik=cik, acc=acc_nodash)
    dest.mkdir(parents=True, exist_ok=True)
    index = json.loads(http_get(f"{base}/index.json", ua))
    time.sleep(throttle)
    names = [it["name"] for it in index["directory"]["item"]]
    primary = filing["primary_doc"]
    wanted = [primary] + [n for n in names if n.lower().endswith(RASTER_SUFFIXES)]
    for name in wanted:
        target = dest / name
        if target.exists() and target.stat().st_size > 0:
            continue
        target.write_bytes(http_get(f"{base}/{name}", ua))
        time.sleep(throttle)
    return dest / primary


def render_pdf(html: Path, pdf: Path, chrome: str, timeout: int = 180) -> None:
    pdf.parent.mkdir(parents=True, exist_ok=True)
    cmd = [
        chrome,
        "--headless=new",
        "--disable-gpu",
        "--no-sandbox",
        "--no-pdf-header-footer",
        "--run-all-compositor-stages-before-draw",
        "--virtual-time-budget=20000",
        f"--print-to-pdf={pdf}",
        f"file://{html.resolve()}",
    ]
    proc = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    if proc.returncode != 0 or not pdf.exists() or pdf.stat().st_size == 0:
        raise RuntimeError(f"chrome render failed ({proc.returncode}): {proc.stderr.strip()[:300]}")


def pdf_page_count(pdf: Path) -> int | None:
    try:
        out = subprocess.run(["pdfinfo", str(pdf)], capture_output=True, text=True, check=True).stdout
        for line in out.splitlines():
            if line.startswith("Pages:"):
                return int(line.split(":", 1)[1].strip())
    except Exception:
        return None
    return None


def build_candidate_pool(ticker_map: dict[str, dict[str, Any]], target: int, seed: int) -> list[tuple[str, str]]:
    """Curated sector tickers first, then a random small/mid-cap tail for diversity."""
    pool: list[tuple[str, str]] = []
    seen: set[str] = set()
    for sector, tickers in SECTOR_TICKERS.items():
        for t in tickers:
            key = t.upper()
            if key not in seen:
                seen.add(key)
                pool.append((key, sector))
    rng = random.Random(seed)
    tail = [t for t in ticker_map if t not in seen]
    rng.shuffle(tail)
    # Over-provision the random tail: many tickers are funds/SPACs with no 10-K.
    for t in tail[: max(0, (target - len(pool)) * 4 + 60)]:
        pool.append((t, "random_tail"))
    return pool


def normalize_ticker(t: str) -> str:
    # EDGAR ticker map uses '-' for share classes (BRK-B); some sources use '.'.
    return t.replace(".", "-").upper()


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", default="eval/data/sec-10k")
    ap.add_argument("--target", type=int, default=150, help="number of successful filings to collect")
    ap.add_argument("--years", default="2024,2025")
    ap.add_argument("--user-agent", default=DEFAULT_UA)
    ap.add_argument("--chrome", default=DEFAULT_CHROME)
    ap.add_argument("--throttle", type=float, default=0.18, help="seconds between EDGAR requests")
    ap.add_argument("--seed", type=int, default=20242025)
    ap.add_argument("--limit", type=int, default=0, help="debug: stop after N candidates (0 = no limit)")
    args = ap.parse_args()

    out = Path(args.out)
    (out / "html").mkdir(parents=True, exist_ok=True)
    (out / "pdfs").mkdir(parents=True, exist_ok=True)
    (out / "meta").mkdir(parents=True, exist_ok=True)
    years = tuple(y.strip() for y in args.years.split(","))

    if not Path(args.chrome).exists():
        print(f"Chrome not found at {args.chrome}; set --chrome or CHROME_BIN", file=sys.stderr)
        return 2

    ticker_map = load_ticker_map(args.user_agent, out / "company_tickers.json")
    pool = build_candidate_pool(ticker_map, args.target, args.seed)
    print(f"candidate pool: {len(pool)} tickers; target {args.target} filings", flush=True)

    manifest: list[dict[str, Any]] = []
    manifest_path = out / "manifest.json"
    if manifest_path.exists():
        manifest = json.loads(manifest_path.read_text())
    done = {m["ticker"] for m in manifest}

    collected = len(manifest)
    for i, (ticker, sector) in enumerate(pool):
        if collected >= args.target:
            break
        if args.limit and i >= args.limit:
            break
        if ticker in done:
            continue
        norm = normalize_ticker(ticker)
        info = ticker_map.get(norm) or ticker_map.get(ticker)
        if not info:
            continue
        cik = info["cik"]
        try:
            filing = find_latest_10k(cik, args.user_agent, args.throttle, years)
            if not filing:
                print(f"  [{collected}/{args.target}] {ticker}: no 10-K in {years}", flush=True)
                continue
            html_dir = out / "html" / ticker
            html = download_filing_assets(cik, filing, html_dir, args.user_agent, args.throttle)
            pdf = out / "pdfs" / f"{ticker}.pdf"
            render_pdf(html, pdf, args.chrome)
            pages = pdf_page_count(pdf)
            meta = {
                "ticker": ticker,
                "sector": sector,
                "cik": cik,
                "company": filing["company"],
                "accession": filing["accession"],
                "filing_date": filing["filing_date"],
                "primary_doc": filing["primary_doc"],
                "pdf": str(pdf),
                "pages": pages,
                "pdf_bytes": pdf.stat().st_size,
            }
            (out / "meta" / f"{ticker}.json").write_text(json.dumps(meta, indent=2))
            manifest.append(meta)
            manifest_path.write_text(json.dumps(manifest, indent=2))
            collected += 1
            print(f"  [{collected}/{args.target}] {ticker} ({sector}) {filing['filing_date']} -> {pages}p", flush=True)
        except Exception as exc:  # one bad filing must not abort the corpus build
            print(f"  !! {ticker}: {exc}", flush=True)
            continue

    by_sector: dict[str, int] = {}
    for m in manifest:
        by_sector[m["sector"]] = by_sector.get(m["sector"], 0) + 1
    print(f"\ndone: {len(manifest)} filings", flush=True)
    print("by sector: " + json.dumps(by_sector, indent=2), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
