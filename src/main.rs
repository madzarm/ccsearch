mod claude;
mod cli;
mod config;
mod db;
mod indexer;
mod model;
mod search;
mod tui;

use anyhow::Result;
use clap::Parser;
use colored::Colorize;

use cli::{Cli, Commands};
use config::Config;
use db::Database;

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Search(args) => cmd_search(args),
        Commands::Index(args) => cmd_index(args),
        Commands::List(args) => cmd_list(args),
        Commands::Config => cmd_config(),
    }
}

fn cmd_search(args: cli::SearchArgs) -> Result<()> {
    let config = Config::load()?;
    let db = Database::open(&config::db_path())?;

    // Try to load embedder for vector search
    let mut embedder = load_embedder_if_available();

    // JIT index: quick check for new/changed sessions
    {
        let mut indexer = indexer::Indexer::new(&db, None, &config, false);
        if let Err(e) = indexer.jit_index() {
            log::warn!("JIT index error: {}", e);
        }
    }

    // Perform hybrid search
    let results = search::hybrid_search(
        &db,
        embedder.as_mut(),
        &args.query,
        args.limit,
        args.bm25_weight,
        args.vec_weight,
        config.rrf_k,
    )?;

    if results.is_empty() {
        eprintln!(
            "{} No sessions found matching \"{}\"",
            "Info:".blue(),
            args.query
        );
        eprintln!("Try running `ccsearch index` first, or broaden your search.");
        return Ok(());
    }

    // Output mode
    if args.json {
        let json = serde_json::to_string_pretty(&results)?;
        println!("{}", json);
        return Ok(());
    }

    if args.no_tui {
        print_results_plain(&results);
        return Ok(());
    }

    // Interactive TUI picker
    let selected = tui::run(results, &args.query)?;
    if let Some((session_id, project_path)) = selected {
        eprintln!(
            "{} Resuming session {}...",
            "→".green(),
            &session_id[..8.min(session_id.len())]
        );
        claude::resume_session(&session_id, Some(&project_path))?;
    }

    Ok(())
}

fn cmd_index(args: cli::IndexArgs) -> Result<()> {
    let config = Config::load()?;
    let db = Database::open(&config::db_path())?;

    let embedder = load_embedder_if_available();

    let mut indexer = indexer::Indexer::new(&db, embedder, &config, args.verbose);

    eprintln!("{} Indexing Claude Code sessions...\n", "→".green());

    let _stats = indexer.index_all(args.force, args.days)?;

    Ok(())
}

fn cmd_list(args: cli::ListArgs) -> Result<()> {
    let config = Config::load()?;
    let db = Database::open(&config::db_path())?;

    // JIT index
    {
        let mut indexer = indexer::Indexer::new(&db, None, &config, false);
        if let Err(e) = indexer.jit_index() {
            log::warn!("JIT index error: {}", e);
        }
    }

    let sessions = db.list_sessions(Some(args.days), args.project.as_deref(), 100)?;

    if sessions.is_empty() {
        eprintln!(
            "{} No sessions found. Try running `ccsearch index` first.",
            "Info:".blue()
        );
        return Ok(());
    }

    if args.json {
        let json = serde_json::to_string_pretty(&sessions)?;
        println!("{}", json);
        return Ok(());
    }

    // Print as a table
    eprintln!(
        "{} ({} sessions)\n",
        "Claude Code Sessions".bold(),
        sessions.len()
    );

    for session in &sessions {
        let title = session
            .summary
            .as_deref()
            .or(session.first_prompt.as_deref())
            .unwrap_or("(no title)");

        let date = chrono::DateTime::parse_from_rfc3339(&session.created_at)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|_| session.created_at.chars().take(16).collect());

        let branch = session
            .git_branch
            .as_deref()
            .map(|b| format!(" [{}]", b))
            .unwrap_or_default();

        println!(
            "  {} {} {}{}",
            date.blue(),
            title,
            short_path(&session.project_path).green(),
            branch.magenta()
        );
        println!("    {}: {}", "id".dimmed(), session.session_id.dimmed());
    }

    Ok(())
}

fn cmd_config() -> Result<()> {
    let config = Config::load()?;
    let path = config::config_path();

    println!("{} {}\n", "Config file:".bold(), path.display());
    println!("{}", toml::to_string_pretty(&config)?);

    if !path.exists() {
        println!(
            "\n{} No config file found. Creating default at {}",
            "Note:".yellow(),
            path.display()
        );
        config.save()?;
    }

    Ok(())
}

/// Attempts to load the embedding model, returns None if not available
fn load_embedder_if_available() -> Option<indexer::embedder::Embedder> {
    let base_dir = config::ccsearch_dir();

    // Check if model is downloaded
    if !model::is_model_downloaded(&base_dir) {
        // Try to download
        match model::ensure_model(&base_dir) {
            Ok(_) => {}
            Err(e) => {
                log::warn!("Could not download embedding model: {}", e);
                eprintln!(
                    "{} Embedding model not available. Using BM25 search only.",
                    "Note:".yellow()
                );
                eprintln!("  Run `ccsearch index` with internet access to download.\n");
                return None;
            }
        }
    }

    let model_dir = model::model_dir(&base_dir);
    match indexer::embedder::Embedder::new(&model_dir) {
        Ok(e) => Some(e),
        Err(e) => {
            log::warn!("Failed to load embedder: {}", e);
            eprintln!(
                "{} Could not load embedding model: {}",
                "Warning:".yellow(),
                e
            );
            eprintln!("  Falling back to BM25 keyword search only.\n");
            None
        }
    }
}

/// Prints search results in plain text format
fn print_results_plain(results: &[search::SearchResult]) {
    for (i, result) in results.iter().enumerate() {
        let title = result
            .session
            .summary
            .as_deref()
            .or(result.session.first_prompt.as_deref())
            .unwrap_or("(no title)");

        let date = chrono::DateTime::parse_from_rfc3339(&result.session.created_at)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|_| result.session.created_at.chars().take(16).collect());

        println!(
            "{}. {} (score: {:.4})",
            (i + 1).to_string().bold(),
            title,
            result.score
        );
        println!(
            "   {} {} {}",
            date.blue(),
            short_path(&result.session.project_path).green(),
            result
                .session
                .git_branch
                .as_deref()
                .map(|b| format!("[{}]", b).magenta().to_string())
                .unwrap_or_default()
        );
        println!("   id: {}", result.session_id.dimmed());
        println!();
    }
}

fn short_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() > 3 {
        format!(".../{}/{}", parts[parts.len() - 2], parts[parts.len() - 1])
    } else {
        path.to_string()
    }
}
