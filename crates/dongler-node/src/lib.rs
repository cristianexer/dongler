use napi_derive::napi;

fn map_error(error: dongler_core::DonglerError) -> napi::Error {
    napi::Error::from_reason(error.to_string())
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
