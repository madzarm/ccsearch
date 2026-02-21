pub mod embedder;
pub mod parser;
#[allow(dead_code)]
pub mod tokenizer;

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

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

    /// Runs a full index of all sessions.
    /// First indexes sessions from sessions-index.json files (rich metadata),
    /// then discovers any .jsonl session files not covered by the index.
    pub fn index_all(&mut self, force: bool, days_filter: Option<u32>) -> Result<IndexStats> {
        let mut stats = IndexStats::default();
        let mut indexed_ids = HashSet::new();

        // Phase 1: Index from sessions-index.json (has metadata like summary, git branch)
        let indices = claude::discover_session_indices()?;

        let total_phases = if indices.is_empty() { 1 } else { 2 };
        if !indices.is_empty() {
            eprintln!("→ Phase 1/{}: Indexing from session indices...", total_phases);

            let pb = ProgressBar::new(indices.len() as u64);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({msg})")
                    .expect("Invalid progress bar template")
                    .progress_chars("#>-"),
            );

            for index_path in &indices {
                pb.set_message(claude::encoded_project_name(index_path).unwrap_or_default());

                match self.index_project(index_path, force, days_filter, &mut indexed_ids) {
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

            pb.finish_and_clear();
        }

        // Phase 2: Discover .jsonl files not in any sessions-index.json
        eprintln!(
            "→ Phase {}/{}: Scanning for unlisted session files...",
            total_phases, total_phases
        );

        let all_files = claude::discover_all_session_files()?;
        let unlisted: Vec<_> = all_files
            .iter()
            .filter(|(sid, _)| !indexed_ids.contains(*sid))
            .collect();

        if !unlisted.is_empty() {
            let pb = ProgressBar::new(unlisted.len() as u64);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({msg})")
                    .expect("Invalid progress bar template")
                    .progress_chars("#>-"),
            );

            let cutoff = days_filter
                .map(|days| chrono::Utc::now() - chrono::Duration::days(days as i64));

            for (session_id, (jsonl_path, encoded_name)) in &unlisted {
                pb.set_message(encoded_name.clone());

                // Staleness check
                if !force {
                    let current_mtime = parser::file_mtime(jsonl_path).unwrap_or(0);
                    if let Ok(Some(stored_mtime)) = self.db.get_session_mtime(session_id) {
                        if stored_mtime >= current_mtime {
                            stats.sessions_skipped += 1;
                            pb.inc(1);
                            continue;
                        }
                    }
                }

                // Date filter based on file mtime
                if let Some(ref cutoff_time) = cutoff {
                    if let Ok(mtime) = parser::file_mtime(jsonl_path) {
                        let file_time = chrono::DateTime::from_timestamp(mtime, 0);
                        if let Some(ft) = file_time {
                            if ft < *cutoff_time {
                                stats.sessions_skipped += 1;
                                pb.inc(1);
                                continue;
                            }
                        }
                    }
                }

                let decoded_path = claude::decode_project_path(encoded_name);
                // Create a minimal entry for sessions not in the index
                let entry = SessionIndexEntry {
                    session_id: session_id.to_string(),
                    full_path: Some(jsonl_path.to_string_lossy().to_string()),
                    first_prompt: None,
                    summary: None,
                    slug: None,
                    project_path: Some(decoded_path.clone()),
                    message_count: None,
                    created: None,
                    modified: None,
                    created_at: None,
                    last_activity_at: None,
                    file_mtime: None,
                    git_branch: None,
                };

                match self.index_session(&entry, jsonl_path, &decoded_path) {
                    Ok(_) => {
                        stats.sessions_indexed += 1;
                        if self.verbose {
                            log::info!("Indexed unlisted session: {}", session_id);
                        }
                    }
                    Err(e) => {
                        log::warn!("Error indexing session {}: {}", session_id, e);
                        stats.sessions_errored += 1;
                    }
                }

                pb.inc(1);
            }

            pb.finish_and_clear();
        }

        eprintln!(
            "\nDone: {} sessions indexed, {} skipped, {} errors",
            stats.sessions_indexed, stats.sessions_skipped, stats.sessions_errored
        );

        Ok(stats)
    }

    /// Performs a quick JIT index check — only indexes new/changed sessions
    pub fn jit_index(&mut self) -> Result<()> {
        let mut indexed_ids = HashSet::new();

        // Check sessions-index.json files
        let indices = claude::discover_session_indices()?;
        for index_path in &indices {
            if let Err(e) = self.index_project(index_path, false, None, &mut indexed_ids) {
                log::warn!("JIT index error for {:?}: {}", index_path, e);
            }
        }

        // Also check for unlisted .jsonl files
        let all_files = claude::discover_all_session_files()?;
        for (session_id, (jsonl_path, encoded_name)) in &all_files {
            if indexed_ids.contains(session_id.as_str()) {
                continue;
            }
            // Staleness check
            let current_mtime = parser::file_mtime(jsonl_path).unwrap_or(0);
            if let Ok(Some(stored_mtime)) = self.db.get_session_mtime(session_id) {
                if stored_mtime >= current_mtime {
                    continue;
                }
            }

            let decoded_path = claude::decode_project_path(encoded_name);
            let entry = SessionIndexEntry {
                session_id: session_id.to_string(),
                full_path: Some(jsonl_path.to_string_lossy().to_string()),
                first_prompt: None,
                summary: None,
                slug: None,
                project_path: Some(decoded_path.clone()),
                message_count: None,
                created: None,
                modified: None,
                created_at: None,
                last_activity_at: None,
                file_mtime: None,
                git_branch: None,
            };

            if let Err(e) = self.index_session(&entry, jsonl_path, &decoded_path) {
                log::warn!("JIT index error for session {}: {}", session_id, e);
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
        indexed_ids: &mut HashSet<String>,
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
            // Track that we've seen this session (even if we skip it)
            indexed_ids.insert(entry.session_id.clone());

            // Apply date filter (try "created" first, then "createdAt")
            if let Some(ref cutoff_time) = cutoff {
                let created_str = entry.created.as_ref().or(entry.created_at.as_ref());
                if let Some(created) = created_str {
                    if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(created) {
                        if ts < *cutoff_time {
                            stats.sessions_skipped += 1;
                            continue;
                        }
                    }
                }
            }

            // Use fullPath from index if available, otherwise construct it
            let jsonl_path = if let Some(ref fp) = entry.full_path {
                PathBuf::from(fp)
            } else {
                project_dir.join(format!("{}.jsonl", &entry.session_id))
            };
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
        let parsed =
            parser::parse_conversation_jsonl(jsonl_path, self.config.max_text_chars)?;

        let mtime = parser::file_mtime(jsonl_path)?;
        let now = chrono::Utc::now().to_rfc3339();

        // For sessions without index metadata, derive timestamps from file mtime
        let mtime_rfc3339 = chrono::DateTime::from_timestamp(mtime, 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| now.clone());

        // Prefer: index metadata > JSONL timestamps > file mtime
        let created_at = entry
            .created
            .clone()
            .or_else(|| entry.created_at.clone())
            .or(parsed.first_timestamp)
            .unwrap_or_else(|| mtime_rfc3339.clone());

        let modified_at = entry
            .modified
            .clone()
            .or_else(|| entry.last_activity_at.clone())
            .or(parsed.last_timestamp)
            .unwrap_or_else(|| mtime_rfc3339);

        let session = ParsedSession {
            session_id: entry.session_id.clone(),
            project_path: entry
                .project_path
                .clone()
                .unwrap_or_else(|| decoded_path.to_string()),
            first_prompt: parsed.first_prompt
                .or_else(|| entry.first_prompt.clone())
                .or_else(|| entry.summary.clone()),
            summary: entry.summary.clone(),
            slug: entry.slug.clone(),
            git_branch: entry.git_branch.clone(),
            message_count: entry.message_count.unwrap_or(parsed.message_count),
            created_at,
            modified_at,
            full_text: parsed.full_text,
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
        // Take first portion of full text (char-safe truncation)
        let truncated: String = session.full_text.chars().take(2000).collect();
        parts.push(truncated);
    }

    parts.join(" ")
}

#[derive(Debug, Default)]
pub struct IndexStats {
    pub sessions_indexed: usize,
    pub sessions_skipped: usize,
    pub sessions_errored: usize,
}
