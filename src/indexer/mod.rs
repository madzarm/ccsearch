pub mod embedder;
pub mod parser;
#[allow(dead_code)]
pub mod tokenizer;

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;

use crate::claude;
use crate::config::Config;
use crate::db::Database;
use parser::{ParsedSession, SessionIndexEntry};

/// Orchestrates the full indexing pipeline
pub struct Indexer<'a> {
    db: &'a Database,
    embedder: Option<embedder::Embedder>,
    config: &'a Config,
    verbose: bool,
}

impl<'a> Indexer<'a> {
    pub fn new(
        db: &'a Database,
        embedder: Option<embedder::Embedder>,
        config: &'a Config,
        verbose: bool,
    ) -> Self {
        Self {
            db,
            embedder,
            config,
            verbose,
        }
    }

    /// Runs a full index of all sessions
    pub fn index_all(&mut self, force: bool, days_filter: Option<u32>) -> Result<IndexStats> {
        let indices = claude::discover_session_indices()?;
        let mut stats = IndexStats::default();

        if indices.is_empty() {
            log::info!("No session indices found");
            return Ok(stats);
        }

        let pb = ProgressBar::new(indices.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({msg})",
                )
                .expect("Invalid progress bar template")
                .progress_chars("#>-"),
        );

        for index_path in &indices {
            pb.set_message(claude::encoded_project_name(index_path).unwrap_or_default());

            match self.index_project(index_path, force, days_filter) {
                Ok(project_stats) => {
                    stats.sessions_indexed += project_stats.sessions_indexed;
                    stats.sessions_skipped += project_stats.sessions_skipped;
                    stats.sessions_errored += project_stats.sessions_errored;
                }
                Err(e) => {
                    log::warn!("Error indexing {:?}: {}", index_path, e);
                    stats.sessions_errored += 1;
                }
            }

            pb.inc(1);
        }

        pb.finish_with_message(format!(
            "Done: {} indexed, {} skipped, {} errors",
            stats.sessions_indexed, stats.sessions_skipped, stats.sessions_errored
        ));

        Ok(stats)
    }

    /// Performs a quick JIT index check — only indexes new/changed sessions
    pub fn jit_index(&mut self) -> Result<()> {
        let indices = claude::discover_session_indices()?;
        for index_path in &indices {
            if let Err(e) = self.index_project(index_path, false, None) {
                log::warn!("JIT index error for {:?}: {}", index_path, e);
            }
        }
        Ok(())
    }

    /// Indexes sessions from a single project's sessions-index.json
    fn index_project(
        &mut self,
        index_path: &Path,
        force: bool,
        days_filter: Option<u32>,
    ) -> Result<IndexStats> {
        let mut stats = IndexStats::default();

        let project_dir = claude::project_dir_from_index(index_path)
            .context("Could not determine project directory")?;

        let encoded_name =
            claude::encoded_project_name(index_path).unwrap_or_else(|| "unknown".to_string());

        let decoded_path = claude::decode_project_path(&encoded_name);

        let entries = match parser::parse_session_index(index_path) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("Failed to parse {:?}: {}", index_path, e);
                return Ok(stats);
            }
        };

        let cutoff =
            days_filter.map(|days| chrono::Utc::now() - chrono::Duration::days(days as i64));

        for entry in &entries {
            // Apply date filter
            if let Some(ref cutoff_time) = cutoff {
                if let Some(ref created) = entry.created_at {
                    if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(created) {
                        if ts < *cutoff_time {
                            stats.sessions_skipped += 1;
                            continue;
                        }
                    }
                }
            }

            let jsonl_path = project_dir.join(format!("{}.jsonl", &entry.session_id));
            if !jsonl_path.exists() {
                stats.sessions_skipped += 1;
                continue;
            }

            // Staleness check
            if !force {
                let current_mtime = parser::file_mtime(&jsonl_path).unwrap_or(0);
                if let Ok(Some(stored_mtime)) = self.db.get_session_mtime(&entry.session_id) {
                    if stored_mtime >= current_mtime {
                        stats.sessions_skipped += 1;
                        continue;
                    }
                }
            }

            // Parse and index
            match self.index_session(entry, &jsonl_path, &decoded_path) {
                Ok(_) => {
                    stats.sessions_indexed += 1;
                    if self.verbose {
                        log::info!("Indexed session: {}", &entry.session_id);
                    }
                }
                Err(e) => {
                    log::warn!("Error indexing session {}: {}", &entry.session_id, e);
                    stats.sessions_errored += 1;
                }
            }
        }

        Ok(stats)
    }

    /// Indexes a single session
    fn index_session(
        &mut self,
        entry: &SessionIndexEntry,
        jsonl_path: &Path,
        decoded_path: &str,
    ) -> Result<()> {
        let (full_text, first_prompt, message_count) =
            parser::parse_conversation_jsonl(jsonl_path, self.config.max_text_chars)?;

        let mtime = parser::file_mtime(jsonl_path)?;
        let now = chrono::Utc::now().to_rfc3339();

        let session = ParsedSession {
            session_id: entry.session_id.clone(),
            project_path: entry
                .project_path
                .clone()
                .unwrap_or_else(|| decoded_path.to_string()),
            first_prompt: first_prompt.or_else(|| entry.summary.clone()),
            summary: entry.summary.clone(),
            slug: entry.slug.clone(),
            git_branch: entry.git_branch.clone(),
            message_count,
            created_at: entry.created_at.clone().unwrap_or_else(|| now.clone()),
            modified_at: entry
                .last_activity_at
                .clone()
                .unwrap_or_else(|| now.clone()),
            full_text,
        };

        // Store in DB
        self.db.upsert_session(&session, mtime, &now)?;

        // Generate and store embedding if embedder is available
        if let Some(ref mut embedder) = self.embedder {
            let text_for_embedding = build_embedding_text(&session);
            let embedding = embedder.embed(&text_for_embedding)?;
            self.db.upsert_embedding(&session.session_id, &embedding)?;
        }

        Ok(())
    }
}

/// Builds the text to embed, prioritizing summary and first prompt
fn build_embedding_text(session: &ParsedSession) -> String {
    let mut parts = Vec::new();

    if let Some(ref summary) = session.summary {
        parts.push(summary.clone());
    }
    if let Some(ref first_prompt) = session.first_prompt {
        parts.push(first_prompt.clone());
    }
    if !session.full_text.is_empty() {
        // Take first portion of full text
        let max = 2000;
        if session.full_text.len() > max {
            parts.push(session.full_text[..max].to_string());
        } else {
            parts.push(session.full_text.clone());
        }
    }

    parts.join(" ")
}

#[derive(Debug, Default)]
pub struct IndexStats {
    pub sessions_indexed: usize,
    pub sessions_skipped: usize,
    pub sessions_errored: usize,
}
