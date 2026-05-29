#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import unicodedata
from pathlib import Path

IMAGE_SUFFIXES = {".png", ".jpg", ".jpeg", ".tif", ".tiff", ".gif", ".bmp", ".webp"}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Render original/extracted Markdown/extracted LaTeX comparison screenshots."
    )
    parser.add_argument("document", help="Document to extract and compare.")
    parser.add_argument("--cli", default="target/debug/dongler")
    parser.add_argument("--out-dir", default="eval/out/visual-comparisons")
    return parser.parse_args()


def run_extract(cli: Path, document: Path, output_format: str) -> str:
    result = subprocess.run(
        [str(cli), "extract", str(document), "--format", output_format],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or result.stdout.strip())
    return result.stdout


def render_pdf_first_page(pdf: Path, output_prefix: Path) -> Path:
    subprocess.run(
        [
            "pdftoppm",
            "-f",
            "1",
            "-l",
            "1",
            "-singlefile",
            "-png",
            str(pdf),
            str(output_prefix),
        ],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    return output_prefix.with_suffix(".png")


def compile_latex(tex_path: Path, output_dir: Path) -> Path:
    subprocess.run(
        [
            "pdflatex",
            "-interaction=nonstopmode",
            "-halt-on-error",
            f"-output-directory={output_dir}",
            str(tex_path),
        ],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    return output_dir / f"{tex_path.stem}.pdf"


def write_error_artifact(
    output_dir: Path,
    filename: str,
    stage: str,
    error: BaseException,
) -> Path:
    artifact = {
        "stage": stage,
        "error_type": error.__class__.__name__,
        "message": str(error),
    }
    if isinstance(error, subprocess.CalledProcessError):
        artifact["returncode"] = error.returncode
        artifact["cmd"] = [str(part) for part in error.cmd]
        artifact["stdout"] = error.stdout or error.output or ""
        artifact["stderr"] = error.stderr or ""
    path = output_dir / filename
    path.write_text(json.dumps(artifact, indent=2, sort_keys=True))
    return path


def compile_latex_or_artifact(
    tex_path: Path,
    output_dir: Path,
    stage: str,
    error_filename: str,
) -> tuple[Path | None, Path | None]:
    try:
        return compile_latex(tex_path, output_dir), None
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        return None, write_error_artifact(output_dir, error_filename, stage, error)


def render_pdf_first_page_or_artifact(
    pdf: Path,
    output_prefix: Path,
    stage: str,
    error_filename: str,
) -> tuple[Path | None, Path | None]:
    try:
        return render_pdf_first_page(pdf, output_prefix), None
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        return None, write_error_artifact(output_prefix.parent, error_filename, stage, error)


def markdown_to_latex_document(markdown: str) -> str:
    blocks = markdown.splitlines()
    output = [
        "\\documentclass{article}",
        "\\usepackage[margin=0.7in]{geometry}",
        "\\usepackage{array}",
        "\\begin{document}",
    ]
    index = 0
    while index < len(blocks):
        line = blocks[index].strip()
        if not line:
            index += 1
            continue
        heading = re.match(r"^(#{1,6})\s+(.+)$", line)
        if heading:
            level = len(heading.group(1))
            command = {
                1: "section",
                2: "subsection",
                3: "subsubsection",
            }.get(level, "paragraph")
            output.append(f"\\{command}{{{escape_latex(heading.group(2))}}}")
            index += 1
            continue
        if markdown_table_start(blocks, index):
            table, index = markdown_table_to_latex(blocks, index)
            output.append(table)
            continue
        if line.startswith("- "):
            output.append("\\begin{itemize}")
            while index < len(blocks) and blocks[index].strip().startswith("- "):
                output.append(f"\\item {escape_latex(blocks[index].strip()[2:].strip())}")
                index += 1
            output.append("\\end{itemize}")
            continue
        paragraph = [line]
        index += 1
        while index < len(blocks) and blocks[index].strip():
            if markdown_table_start(blocks, index) or blocks[index].strip().startswith("- "):
                break
            if re.match(r"^(#{1,6})\s+(.+)$", blocks[index].strip()):
                break
            paragraph.append(blocks[index].strip())
            index += 1
        output.append(escape_latex(" ".join(paragraph)))
    output.append("\\end{document}")
    return "\n\n".join(output) + "\n"


def markdown_table_start(lines: list[str], index: int) -> bool:
    return (
        index + 1 < len(lines)
        and "|" in lines[index]
        and "|" in lines[index + 1]
        and all(
            cell.strip().strip(":").replace("-", "") == ""
            and cell.strip().strip(":")
            for cell in lines[index + 1].strip().strip("|").split("|")
        )
    )


def markdown_table_to_latex(lines: list[str], index: int) -> tuple[str, int]:
    rows = [markdown_cells(lines[index])]
    index += 2
    while index < len(lines) and "|" in lines[index] and lines[index].strip():
        rows.append(markdown_cells(lines[index]))
        index += 1
    width = max(len(row) for row in rows)
    spec = "l" * width
    output = [f"\\begin{{tabular}}{{{spec}}}"]
    for row_index, row in enumerate(rows):
        row = row + [""] * (width - len(row))
        output.append(" & ".join(escape_latex(cell) for cell in row) + r" \\")
        if row_index == 0:
            output.append("\\hline")
    output.append("\\end{tabular}")
    return "\n".join(output), index


def markdown_cells(line: str) -> list[str]:
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def escape_latex(text: str) -> str:
    replacements = {
        "\\": "\\textbackslash{}",
        "&": "\\&",
        "%": "\\%",
        "$": "\\$",
        "#": "\\#",
        "_": "\\_",
        "{": "\\{",
        "}": "\\}",
        "~": "\\textasciitilde{}",
        "^": "\\textasciicircum{}",
    }
    return "".join(escape_latex_character(character, replacements) for character in text)


def escape_latex_character(character: str, replacements: dict[str, str]) -> str:
    if character in replacements:
        return replacements[character]
    if unicodedata.category(character).startswith("C"):
        if character == "\n":
            return "\n"
        return " " if character.isspace() else ""
    if character.isascii():
        return character
    return {
        "\u00a0": " ",
        "–": "-",
        "−": "-",
        "—": "---",
        "‘": "'",
        "’": "'",
        "‚": "'",
        "“": '"',
        "”": '"',
        "„": '"',
        "•": "*",
        "…": "...",
        "×": "x",
        "÷": "/",
        "≤": "<=",
        "≥": ">=",
        "≠": "!=",
        "±": "+/-",
    }.get(character, "?")


def main() -> int:
    args = parse_args()
    cli = Path(args.cli)
    document = Path(args.document)
    out_dir = Path(args.out_dir) / document.stem
    out_dir.mkdir(parents=True, exist_ok=True)

    markdown = run_extract(cli, document, "markdown")
    latex = run_extract(cli, document, "latex")
    markdown_path = out_dir / "extracted.md"
    latex_path = out_dir / "extracted.tex"
    markdown_render_tex = out_dir / "extracted-markdown-render.tex"
    markdown_path.write_text(markdown)
    latex_path.write_text(latex)
    markdown_render_tex.write_text(markdown_to_latex_document(markdown))

    artifacts: dict[str, str] = {
        "source": str(document),
        "extracted_markdown": str(markdown_path),
        "extracted_latex": str(latex_path),
    }

    if document.suffix.lower() == ".pdf":
        original_png, original_error = render_pdf_first_page_or_artifact(
            document,
            out_dir / "original",
            "source_pdf_render",
            "original-render.error.json",
        )
        if original_png:
            artifacts["original_png"] = str(original_png)
        if original_error:
            artifacts["original_render_error"] = str(original_error)
    elif document.suffix.lower() in IMAGE_SUFFIXES:
        original_image = out_dir / f"original{document.suffix.lower()}"
        shutil.copyfile(document, original_image)
        artifacts["original_image"] = str(original_image)

    markdown_pdf, markdown_compile_error = compile_latex_or_artifact(
        markdown_render_tex,
        out_dir,
        "markdown_latex_compile",
        "extracted-markdown-render.error.json",
    )
    if markdown_compile_error:
        artifacts["markdown_render_error"] = str(markdown_compile_error)
    elif markdown_pdf:
        markdown_png, markdown_render_error = render_pdf_first_page_or_artifact(
            markdown_pdf,
            out_dir / "markdown",
            "markdown_pdf_render",
            "markdown-render.error.json",
        )
        if markdown_png:
            artifacts["markdown_png"] = str(markdown_png)
        if markdown_render_error:
            artifacts["markdown_render_error"] = str(markdown_render_error)

    latex_pdf, latex_compile_error = compile_latex_or_artifact(
        latex_path,
        out_dir,
        "latex_compile",
        "extracted-latex.error.json",
    )
    if latex_compile_error:
        artifacts["latex_render_error"] = str(latex_compile_error)
    elif latex_pdf:
        latex_png, latex_render_error = render_pdf_first_page_or_artifact(
            latex_pdf,
            out_dir / "latex",
            "latex_pdf_render",
            "latex-render.error.json",
        )
        if latex_png:
            artifacts["latex_png"] = str(latex_png)
        if latex_render_error:
            artifacts["latex_render_error"] = str(latex_render_error)

    manifest_path = out_dir / "comparison.json"
    manifest_path.write_text(json.dumps(artifacts, indent=2, sort_keys=True))
    print(json.dumps(artifacts, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
