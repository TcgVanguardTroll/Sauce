//! Optional visual-similarity sidecar.
//!
//! Character-art similarity ("looks-like") is computed from image embeddings.
//! Rather than bundle a model into the Rust binary, we shell out to a small
//! Python script (`embed_image.py`) that loads a CLIP-style model and prints a
//! JSON float array. This keeps the core tool dependency-free: if Python or the
//! model isn't present, embedding simply degrades to attribute-only ranking.
//!
//! Contract: `python3 embed_image.py <image-url-or-path>` → `[f32, f32, …]` on
//! stdout, or a non-zero exit on failure.

use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;
use std::process::Command;

/// Locates `embed_image.py` next to the binary or in the current directory.
fn script_path() -> Option<PathBuf> {
    let candidates = [
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("embed_image.py"))),
        Some(PathBuf::from("embed_image.py")),
        Some(PathBuf::from("./scripts/embed_image.py")),
    ];
    candidates.into_iter().flatten().find(|p| p.exists())
}

/// True when the Python sidecar appears to be available.
pub fn available() -> bool {
    script_path().is_some()
}

/// Embeds a single image (URL or local path) into a float vector.
pub fn embed(image: &str) -> Result<Vec<f32>> {
    let script = script_path().ok_or_else(|| {
        anyhow!("embed_image.py not found — install it to enable visual similarity")
    })?;
    let output = Command::new("python3")
        .arg(&script)
        .arg(image)
        .output()
        .context("running python3 embed_image.py")?;
    if !output.status.success() {
        return Err(anyhow!(
            "embed_image.py failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let stdout = String::from_utf8(output.stdout).context("sidecar output was not UTF-8")?;
    let vec: Vec<f32> = serde_json::from_str(stdout.trim()).context("parsing embedding JSON")?;
    if vec.is_empty() {
        return Err(anyhow!("sidecar returned an empty embedding"));
    }
    Ok(vec)
}
