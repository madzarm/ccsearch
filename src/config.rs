use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_bm25_weight")]
    pub bm25_weight: f64,

    #[serde(default = "default_vec_weight")]
    pub vec_weight: f64,

    #[serde(default = "default_rrf_k")]
    pub rrf_k: f64,

    #[serde(default = "default_max_results")]
    pub max_results: usize,

    #[serde(default = "default_days")]
    pub default_days: u32,

    #[serde(default = "default_max_text_chars")]
    pub max_text_chars: usize,

    /// Recency boost half-life in days. Sessions this many days old get 50% boost.
    /// Set to 0 to disable recency boosting.
    #[serde(default = "default_recency_halflife")]
    pub recency_halflife: f64,
}

fn default_bm25_weight() -> f64 {
    1.0
}
fn default_vec_weight() -> f64 {
    1.0
}
fn default_rrf_k() -> f64 {
    60.0
}
fn default_max_results() -> usize {
    20
}
fn default_days() -> u32 {
    30
}
fn default_max_text_chars() -> usize {
    8000
}
fn default_recency_halflife() -> f64 {
    7.0
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bm25_weight: default_bm25_weight(),
            vec_weight: default_vec_weight(),
            rrf_k: default_rrf_k(),
            max_results: default_max_results(),
            default_days: default_days(),
            max_text_chars: default_max_text_chars(),
            recency_halflife: default_recency_halflife(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path).context("Failed to read config file")?;
            let config: Config = toml::from_str(&content).context("Failed to parse config")?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create config directory")?;
        }
        let content = toml::to_string_pretty(self).context("Failed to serialize config")?;
        std::fs::write(&path, content).context("Failed to write config file")?;
        Ok(())
    }
}

pub fn ccsearch_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Could not determine home directory")
        .join(".ccsearch")
}

pub fn config_path() -> PathBuf {
    ccsearch_dir().join("config.toml")
}

pub fn db_path() -> PathBuf {
    ccsearch_dir().join("index.db")
}

#[allow(dead_code)]
pub fn models_dir() -> PathBuf {
    ccsearch_dir().join("models")
}
