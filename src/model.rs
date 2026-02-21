use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const MODEL_FILES: &[(&str, &str)] = &[
    (
        "model.onnx",
        "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx",
    ),
    (
        "tokenizer.json",
        "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json",
    ),
    (
        "config.json",
        "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/config.json",
    ),
];

/// Returns the model directory path
pub fn model_dir(base_dir: &Path) -> PathBuf {
    base_dir.join("models").join("all-MiniLM-L6-v2")
}

/// Checks if the model is already downloaded
pub fn is_model_downloaded(base_dir: &Path) -> bool {
    let dir = model_dir(base_dir);
    MODEL_FILES
        .iter()
        .all(|(filename, _)| dir.join(filename).exists())
}

/// Downloads the model files if not already present
pub fn ensure_model(base_dir: &Path) -> Result<PathBuf> {
    let dir = model_dir(base_dir);

    if is_model_downloaded(base_dir) {
        log::debug!("Model already downloaded at {:?}", dir);
        return Ok(dir);
    }

    eprintln!("Downloading embedding model (all-MiniLM-L6-v2)...");
    eprintln!("This is a one-time download (~80MB).\n");

    fs::create_dir_all(&dir).context("Failed to create model directory")?;

    for (filename, url) in MODEL_FILES {
        let dest = dir.join(filename);
        if dest.exists() {
            log::debug!("{} already exists, skipping", filename);
            continue;
        }

        download_file(url, &dest, filename)?;
    }

    eprintln!("\nModel downloaded successfully.\n");
    Ok(dir)
}

/// Downloads a single file with progress bar
fn download_file(url: &str, dest: &Path, display_name: &str) -> Result<()> {
    let response = ureq::get(url)
        .call()
        .with_context(|| format!("Failed to download {}", url))?;

    let total_size = response
        .header("content-length")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let pb = if total_size > 0 {
        let pb = ProgressBar::new(total_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template(&format!(
                    "  {{spinner:.green}} {} [{{bar:30.cyan/blue}}] {{bytes}}/{{total_bytes}} ({{eta}})",
                    display_name
                ))
                .expect("Invalid progress bar template")
                .progress_chars("=> "),
        );
        Some(pb)
    } else {
        eprintln!("  Downloading {}...", display_name);
        None
    };

    let mut file =
        fs::File::create(dest).with_context(|| format!("Failed to create file {:?}", dest))?;

    let mut reader = response.into_reader();
    let mut buffer = [0u8; 8192];
    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .context("Failed to read response body")?;
        if bytes_read == 0 {
            break;
        }
        file.write_all(&buffer[..bytes_read])
            .context("Failed to write to file")?;
        if let Some(ref pb) = pb {
            pb.inc(bytes_read as u64);
        }
    }

    if let Some(pb) = pb {
        pb.finish();
    }

    Ok(())
}
