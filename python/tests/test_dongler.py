import dongler


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
