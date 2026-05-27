use napi_derive::napi;

fn map_error(error: dongler_core::DonglerError) -> napi::Error {
    napi::Error::from_reason(error.to_string())
}

fn map_runtime_error(error: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(error.to_string())
}

fn parse_document(document_json: &str) -> napi::Result<dongler_core::Document> {
    serde_json::from_str(document_json).map_err(map_runtime_error)
}

fn parse_options(options_json: Option<String>) -> napi::Result<dongler_core::ExtractOptions> {
    options_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(map_runtime_error)
        .map(|options| options.unwrap_or_default())
}

#[napi(js_name = "version")]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[napi(js_name = "parseTextJson")]
pub fn parse_text_json(text: String) -> napi::Result<String> {
    dongler_core::to_json(&text).map_err(map_error)
}

#[napi(js_name = "toMarkdown")]
pub fn to_markdown(text: String) -> napi::Result<String> {
    dongler_core::to_markdown(&text).map_err(map_error)
}

#[napi(js_name = "toJson")]
pub fn to_json(text: String) -> napi::Result<String> {
    dongler_core::to_json(&text).map_err(map_error)
}

#[napi(js_name = "toLatex")]
pub fn to_latex(text: String) -> napi::Result<String> {
    dongler_core::to_latex(&text).map_err(map_error)
}

#[napi(js_name = "detectFormat")]
pub fn detect_format(path: String) -> napi::Result<String> {
    dongler_core::detect_format(&path).map_err(map_error)
}

#[napi(js_name = "loadPathJson")]
pub fn load_path_json(path: String) -> napi::Result<String> {
    dongler_core::load_path(path)
        .and_then(|document| document.to_json())
        .map_err(map_error)
}

#[napi(js_name = "loadPathJsonWithOptions")]
pub fn load_path_json_with_options(
    path: String,
    options_json: Option<String>,
) -> napi::Result<String> {
    let options = parse_options(options_json)?;
    dongler_core::load_path_with_options(path, options)
        .and_then(|document| document.to_json())
        .map_err(map_error)
}

#[napi(js_name = "loadManyJson")]
pub fn load_many_json(paths: Vec<String>) -> napi::Result<String> {
    serde_json::to_string(&dongler_core::load_many(paths)).map_err(map_runtime_error)
}

#[napi(js_name = "documentToMarkdown")]
pub fn document_to_markdown(document_json: String) -> napi::Result<String> {
    parse_document(&document_json)?
        .to_markdown()
        .map_err(map_error)
}

#[napi(js_name = "documentToJson")]
pub fn document_to_json(document_json: String) -> napi::Result<String> {
    parse_document(&document_json)?.to_json().map_err(map_error)
}

#[napi(js_name = "documentToLatex")]
pub fn document_to_latex(document_json: String) -> napi::Result<String> {
    parse_document(&document_json)?
        .to_latex()
        .map_err(map_error)
}
