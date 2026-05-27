use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

fn map_error(error: dongler_core::DonglerError) -> PyErr {
    PyRuntimeError::new_err(error.to_string())
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

#[pymodule]
fn _dongler(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add_function(wrap_pyfunction!(to_markdown, module)?)?;
    module.add_function(wrap_pyfunction!(to_json, module)?)?;
    module.add_function(wrap_pyfunction!(to_latex, module)?)?;
    module.add_function(wrap_pyfunction!(detect_format, module)?)?;
    Ok(())
}
