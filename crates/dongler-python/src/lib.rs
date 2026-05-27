use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use std::fmt;

fn map_error(error: dongler_core::DonglerError) -> PyErr {
    PyRuntimeError::new_err(error.to_string())
}

fn map_runtime_error(error: impl fmt::Display) -> PyErr {
    PyRuntimeError::new_err(error.to_string())
}

fn parse_document(document_json: &str) -> PyResult<dongler_core::Document> {
    serde_json::from_str(document_json).map_err(map_runtime_error)
}

fn parse_options(options_json: Option<&str>) -> PyResult<dongler_core::ExtractOptions> {
    options_json
        .map(serde_json::from_str)
        .transpose()
        .map_err(map_runtime_error)
        .map(|options| options.unwrap_or_default())
}

#[pyfunction]
fn to_markdown(text: &str) -> PyResult<String> {
    dongler_core::to_markdown(text).map_err(map_error)
}

#[pyfunction]
fn to_json(text: &str) -> PyResult<String> {
    dongler_core::to_json(text).map_err(map_error)
}

#[pyfunction]
fn to_latex(text: &str) -> PyResult<String> {
    dongler_core::to_latex(text).map_err(map_error)
}

#[pyfunction]
fn detect_format(path: &str) -> PyResult<String> {
    dongler_core::detect_format(path).map_err(map_error)
}

#[pyfunction]
fn load_path_json(path: &str) -> PyResult<String> {
    dongler_core::load_path(path)
        .and_then(|document| document.to_json())
        .map_err(map_error)
}

#[pyfunction(signature = (path, options_json=None))]
fn load_path_json_with_options(path: &str, options_json: Option<&str>) -> PyResult<String> {
    let options = parse_options(options_json)?;
    dongler_core::load_path_with_options(path, options)
        .and_then(|document| document.to_json())
        .map_err(map_error)
}

#[pyfunction]
fn load_many_json(paths: Vec<String>) -> PyResult<String> {
    serde_json::to_string(&dongler_core::load_many(paths)).map_err(map_runtime_error)
}

#[pyfunction]
fn document_to_markdown(document_json: &str) -> PyResult<String> {
    parse_document(document_json)?
        .to_markdown()
        .map_err(map_error)
}

#[pyfunction]
fn document_to_json(document_json: &str) -> PyResult<String> {
    parse_document(document_json)?.to_json().map_err(map_error)
}

#[pyfunction]
fn document_to_latex(document_json: &str) -> PyResult<String> {
    parse_document(document_json)?.to_latex().map_err(map_error)
}

#[pymodule]
fn _dongler(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add_function(wrap_pyfunction!(to_markdown, module)?)?;
    module.add_function(wrap_pyfunction!(to_json, module)?)?;
    module.add_function(wrap_pyfunction!(to_latex, module)?)?;
    module.add_function(wrap_pyfunction!(detect_format, module)?)?;
    module.add_function(wrap_pyfunction!(load_path_json, module)?)?;
    module.add_function(wrap_pyfunction!(load_path_json_with_options, module)?)?;
    module.add_function(wrap_pyfunction!(load_many_json, module)?)?;
    module.add_function(wrap_pyfunction!(document_to_markdown, module)?)?;
    module.add_function(wrap_pyfunction!(document_to_json, module)?)?;
    module.add_function(wrap_pyfunction!(document_to_latex, module)?)?;
    Ok(())
}
