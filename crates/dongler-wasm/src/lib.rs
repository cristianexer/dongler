//! WebAssembly bindings for `dongler-core`.
//!
//! These bindings expose the filesystem-free extraction API so documents can
//! be processed entirely in the browser (or any other wasm host) from a byte
//! buffer such as a `File`/`Blob` upload. There is no path-based loader here:
//! callers pass the raw bytes together with the original file name, which is
//! used only to detect the format.

use wasm_bindgen::prelude::*;

/// Install a panic hook that forwards Rust panics to `console.error`, making
/// wasm failures debuggable. Runs automatically when the module is loaded.
#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

fn map_error(error: dongler_core::DonglerError) -> JsError {
    JsError::new(&error.to_string())
}

fn map_runtime_error(error: impl std::fmt::Display) -> JsError {
    JsError::new(&error.to_string())
}

fn parse_document(document_json: &str) -> Result<dongler_core::Document, JsError> {
    serde_json::from_str(document_json).map_err(map_runtime_error)
}

fn parse_options(options_json: Option<String>) -> Result<dongler_core::ExtractOptions, JsError> {
    options_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(map_runtime_error)
        .map(Option::unwrap_or_default)
}

/// Package version (matches the crate's `Cargo.toml`).
#[wasm_bindgen(js_name = "version")]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Parse plain text and return the document IR as JSON.
#[wasm_bindgen(js_name = "parseTextJson")]
pub fn parse_text_json(text: &str) -> Result<String, JsError> {
    dongler_core::to_json(text).map_err(map_error)
}

#[wasm_bindgen(js_name = "toMarkdown")]
pub fn to_markdown(text: &str) -> Result<String, JsError> {
    dongler_core::to_markdown(text).map_err(map_error)
}

#[wasm_bindgen(js_name = "toJson")]
pub fn to_json(text: &str) -> Result<String, JsError> {
    dongler_core::to_json(text).map_err(map_error)
}

#[wasm_bindgen(js_name = "toLatex")]
pub fn to_latex(text: &str) -> Result<String, JsError> {
    dongler_core::to_latex(text).map_err(map_error)
}

/// Detect the format name (e.g. `"pdf"`) from a file name's extension.
#[wasm_bindgen(js_name = "detectFormat")]
pub fn detect_format(filename: &str) -> Result<String, JsError> {
    dongler_core::detect_format(filename).map_err(map_error)
}

/// Extract a document from raw bytes and return the document IR as JSON.
///
/// `filename` is used only to detect the format (its extension); the bytes are
/// the actual document contents.
#[wasm_bindgen(js_name = "extractBytesJson")]
pub fn extract_bytes_json(bytes: &[u8], filename: &str) -> Result<String, JsError> {
    dongler_core::extract_bytes(bytes, filename)
        .and_then(|document| document.to_json())
        .map_err(map_error)
}

/// Like [`extract_bytes_json`] but accepts an `ExtractOptions` JSON string.
#[wasm_bindgen(js_name = "extractBytesJsonWithOptions")]
pub fn extract_bytes_json_with_options(
    bytes: &[u8],
    filename: &str,
    options_json: Option<String>,
) -> Result<String, JsError> {
    let options = parse_options(options_json)?;
    dongler_core::extract_bytes_with_options(bytes, filename, options)
        .and_then(|document| document.to_json())
        .map_err(map_error)
}

/// Extract a document from raw bytes and render it directly to Markdown.
#[wasm_bindgen(js_name = "extractBytesMarkdown")]
pub fn extract_bytes_markdown(bytes: &[u8], filename: &str) -> Result<String, JsError> {
    dongler_core::extract_bytes(bytes, filename)
        .and_then(|document| document.to_markdown())
        .map_err(map_error)
}

/// Extract a document from raw bytes and render it directly to LaTeX.
#[wasm_bindgen(js_name = "extractBytesLatex")]
pub fn extract_bytes_latex(bytes: &[u8], filename: &str) -> Result<String, JsError> {
    dongler_core::extract_bytes(bytes, filename)
        .and_then(|document| document.to_latex())
        .map_err(map_error)
}

#[wasm_bindgen(js_name = "documentToMarkdown")]
pub fn document_to_markdown(document_json: &str) -> Result<String, JsError> {
    parse_document(document_json)?
        .to_markdown()
        .map_err(map_error)
}

#[wasm_bindgen(js_name = "documentToJson")]
pub fn document_to_json(document_json: &str) -> Result<String, JsError> {
    parse_document(document_json)?.to_json().map_err(map_error)
}

#[wasm_bindgen(js_name = "documentToLatex")]
pub fn document_to_latex(document_json: &str) -> Result<String, JsError> {
    parse_document(document_json)?.to_latex().map_err(map_error)
}
