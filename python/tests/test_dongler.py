import dongler
import json


def test_import_exposes_version():
    assert dongler.__version__ == "0.3.0"


def test_parse_text_returns_native_dict():
    document = dongler.parse_text("Hello from Dongler\n\nSecond paragraph")

    assert isinstance(document, dict)
    assert document["metadata"]["format"] == "text"
    assert document["metadata"]["block_count"] == 2
    assert isinstance(document["pages"], list)


def test_to_markdown_returns_markdown_string():
    assert dongler.to_markdown("Hello from Dongler") == "Hello from Dongler"


def test_detect_format_works_for_txt():
    assert dongler.detect_format("notes.txt") == "text"


def test_load_returns_document_object_for_text_path(tmp_path):
    path = tmp_path / "notes.txt"
    path.write_text("Hello from a file\n\nSecond paragraph")

    document = dongler.load(path)

    assert document.metadata["format"] == "text"
    assert document.metadata["source"] == str(path)
    assert document.to_markdown() == "Hello from a file\n\nSecond paragraph"
    assert "Hello from a file" in document.to_latex()
    assert json.loads(document.to_json())["metadata"]["block_count"] == 2


def test_load_returns_document_object_for_pdf_path(tmp_path):
    path = tmp_path / "invoice.pdf"
    path.write_bytes(minimal_pdf("Hello PDF"))

    document = dongler.load(path)

    assert document.metadata["format"] == "pdf"
    assert document.pages[0]["width"] == 612.0
    assert document.pages[0]["blocks"][0]["text"] == "Hello PDF"
    assert document.pages[0]["blocks"][0]["source_anchors"][0]["page_number"] == 1


def test_load_many_returns_per_file_results(tmp_path):
    text_path = tmp_path / "notes.txt"
    pdf_path = tmp_path / "invoice.pdf"
    text_path.write_text("Batch document")
    pdf_path.write_bytes(minimal_pdf("Batch PDF"))

    results = dongler.load_many([text_path, pdf_path])

    assert len(results) == 2
    assert results[0]["path"] == str(text_path)
    assert results[0]["ok"] is True
    assert isinstance(results[0]["document"], dongler.DonglerDocument)
    assert results[0]["document"].to_markdown() == "Batch document"

    assert results[1]["path"] == str(pdf_path)
    assert results[1]["ok"] is True
    assert isinstance(results[1]["document"], dongler.DonglerDocument)
    assert results[1]["document"].metadata["format"] == "pdf"
    assert results[1]["error"] is None


def minimal_pdf(text: str) -> bytes:
    content = f"BT /F1 12 Tf 72 720 Td ({text}) Tj ET"
    return (
        b"%PDF-1.4\n"
        b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n"
        b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n"
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        b"/Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>\nendobj\n"
        b"4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n"
        + f"5 0 obj\n<< /Length {len(content)} >>\nstream\n{content}\nendstream\nendobj\n".encode()
        + b"trailer\n<< /Root 1 0 R >>\n%%EOF\n"
    )
