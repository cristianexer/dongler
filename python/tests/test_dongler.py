import dongler
import json
import pytest


def test_import_exposes_version():
    assert dongler.__version__ == "0.1.0"


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


def test_load_returns_planned_error_for_pdf_path(tmp_path):
    path = tmp_path / "invoice.pdf"
    path.write_text("%PDF planned fixture")

    with pytest.raises(RuntimeError, match="pdf extraction"):
        dongler.load(path)


def test_load_many_returns_per_file_results(tmp_path):
    text_path = tmp_path / "notes.txt"
    pdf_path = tmp_path / "invoice.pdf"
    text_path.write_text("Batch document")
    pdf_path.write_text("%PDF planned fixture")

    results = dongler.load_many([text_path, pdf_path])

    assert len(results) == 2
    assert results[0]["path"] == str(text_path)
    assert results[0]["ok"] is True
    assert isinstance(results[0]["document"], dongler.DonglerDocument)
    assert results[0]["document"].to_markdown() == "Batch document"

    assert results[1]["path"] == str(pdf_path)
    assert results[1]["ok"] is False
    assert results[1]["document"] is None
    assert "pdf extraction" in results[1]["error"]
