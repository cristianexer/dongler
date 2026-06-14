//! Model weight download & caching (PRD §5.2/§4.J). Fetches ONNX artifacts and
//! their companion files (e.g. SLANet's structure char-dict) from Hugging Face
//! into a local cache, verifying sha256 against the [`crate::registry`]. The
//! born-digital fast path never calls this — weights download only on first
//! `convert()`. Behind the `ml` feature.
//!
//! Offline: set `DONGLER_OFFLINE=1` (or `HF_HUB_OFFLINE=1`) to require artifacts
//! already present in the cache and fail loudly with the expected path otherwise.

use crate::ml::MlError;
use crate::registry::ModelEntry;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Root cache directory: `$DONGLER_CACHE_DIR` if set, else the OS cache dir
/// (`~/.cache` on Linux, `~/Library/Caches` on macOS) under `dongler/models`.
pub fn cache_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("DONGLER_CACHE_DIR") {
        return PathBuf::from(dir);
    }
    let base = dirs::cache_dir().unwrap_or_else(|| PathBuf::from(".cache"));
    base.join("dongler").join("models")
}

fn offline() -> bool {
    matches!(std::env::var("DONGLER_OFFLINE").as_deref(), Ok("1") | Ok("true"))
        || matches!(std::env::var("HF_HUB_OFFLINE").as_deref(), Ok("1") | Ok("true"))
}

/// Strip the `hf:` scheme from a registry `source` (`hf:owner/repo` → `owner/repo`).
pub fn parse_hf_repo(source: &str) -> Option<&str> {
    source.strip_prefix("hf:")
}

/// Ensure `filename` from the model's HF repo is present in the cache, returning
/// its local path. Downloads it (unless offline) using `hf-hub`, which caches
/// under [`cache_dir`].
pub fn ensure_file(entry: &ModelEntry, filename: &str) -> Result<PathBuf, MlError> {
    let repo = parse_hf_repo(entry.source)
        .ok_or_else(|| MlError::Download(format!("non-hf source: {}", entry.source)))?;

    if offline() {
        // hf-hub lays files out under <cache>/models--<owner>--<repo>/...; rather
        // than reimplement that layout, surface a clear, actionable error.
        return Err(MlError::OfflineMissing(format!(
            "{repo}/{filename} (offline; pre-fetch into {})",
            cache_dir().display()
        )));
    }

    use hf_hub::api::sync::ApiBuilder;
    let api = ApiBuilder::new()
        .with_cache_dir(cache_dir())
        .build()
        .map_err(|e| MlError::Download(e.to_string()))?;
    let path = api
        .model(repo.to_string())
        .get(filename)
        .map_err(|e| MlError::Download(e.to_string()))?;
    Ok(path)
}

/// Ensure the model's ONNX artifact is cached and (if a sha256 is pinned in the
/// registry) verified. Returns the local ONNX path.
pub fn ensure_model(entry: &ModelEntry, onnx_filename: &str) -> Result<PathBuf, MlError> {
    let path = ensure_file(entry, onnx_filename)?;
    if entry.sha256.is_empty() {
        // Not yet pinned (pre-spike). Warn once rather than hard-fail so the
        // bake-off can run; PR0 pins it and turns this into an enforced check.
        eprintln!(
            "dongler: warning: model '{}' has no pinned sha256 (registry.rs); skipping integrity check",
            entry.name
        );
    } else {
        verify_sha256(&path, entry.sha256)?;
    }
    Ok(path)
}

/// Verify a file's sha256 against `expected` (lowercase hex).
pub fn verify_sha256(path: &Path, expected: &str) -> Result<(), MlError> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let got = hex_lower(&hasher.finalize());
    if got.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(MlError::Sha256Mismatch {
            path: path.display().to_string(),
            expected: expected.to_string(),
            got,
        })
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hf_scheme() {
        assert_eq!(parse_hf_repo("hf:RapidAI/RapidTable"), Some("RapidAI/RapidTable"));
        assert_eq!(parse_hf_repo("https://example/x"), None);
    }

    #[test]
    fn sha256_matches_known_vector() {
        // sha256("") = e3b0c4...855
        let dir = std::env::temp_dir().join("dongler-sha-test");
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("empty");
        std::fs::write(&f, b"").unwrap();
        assert!(verify_sha256(
            &f,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        )
        .is_ok());
        assert!(matches!(
            verify_sha256(&f, "deadbeef"),
            Err(MlError::Sha256Mismatch { .. })
        ));
    }

    #[test]
    fn cache_dir_honors_env_override() {
        // SAFETY: single-threaded test; restore after.
        let prev = std::env::var("DONGLER_CACHE_DIR").ok();
        std::env::set_var("DONGLER_CACHE_DIR", "/tmp/dongler-cache-xyz");
        assert_eq!(cache_dir(), PathBuf::from("/tmp/dongler-cache-xyz"));
        match prev {
            Some(v) => std::env::set_var("DONGLER_CACHE_DIR", v),
            None => std::env::remove_var("DONGLER_CACHE_DIR"),
        }
    }
}
