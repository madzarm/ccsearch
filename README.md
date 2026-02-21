# ccsearch

Hybrid search CLI for Claude Code chat sessions. Finds past conversations using BM25 keyword search + vector embeddings + Reciprocal Rank Fusion, presents results in an interactive TUI picker, and auto-resumes the selected session via `claude --resume`.

## Why?

Claude Code's built-in `--resume` picker only shows the last ~50 sessions with no search. Finding a specific past conversation means scrolling through a flat list. **ccsearch** fixes this with fast, intelligent search across all your sessions.

## Features

- **Hybrid search**: BM25 keyword matching + semantic vector search (all-MiniLM-L6-v2)
- **Reciprocal Rank Fusion**: Combines keyword and semantic results for better relevance
- **Interactive TUI**: Browse results with keyboard navigation, preview pane, and one-key resume
- **JIT indexing**: Automatically detects new/changed sessions before each search
- **Graceful degradation**: Works with BM25 only if the embedding model isn't available
- **JSON output**: Scriptable `--json` flag for integration with other tools

## Installation

### Shell script (macOS & Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/madzarm/ccsearch/master/install.sh | sh
```

To install to a custom directory:

```bash
INSTALL_DIR=~/.local/bin curl -fsSL https://raw.githubusercontent.com/madzarm/ccsearch/master/install.sh | sh
```

### Windows / From source (requires Rust)

```bash
cargo install --git https://github.com/madzarm/ccsearch.git
```

## Quick Start

```bash
# Index all your Claude Code sessions
ccsearch index

# Search (shorthand — no subcommand needed)
ccsearch "authentication bug"

# Equivalent explicit form
ccsearch search "authentication bug"

# Search with plain text output (no TUI)
ccsearch search "refactor database" --no-tui

# JSON output for scripting
ccsearch search "dark mode" --json

# List recent sessions
ccsearch list --days 7

# Show config
ccsearch config
```

## Commands

### `ccsearch <query>` / `ccsearch search <query>`

Primary command. Searches sessions and shows an interactive TUI picker. The `search` subcommand is optional — `ccsearch "my query"` works directly.

| Flag | Default | Description |
|------|---------|-------------|
| `--days N` | 30 | Only search sessions from last N days |
| `--project PATH` | | Filter to specific project |
| `--limit N` | 20 | Max results |
| `--no-tui` | | Print results to stdout |
| `--json` | | JSON output for scripting |
| `--bm25-weight F` | 1.0 | BM25 weight in RRF fusion |
| `--vec-weight F` | 1.0 | Vector weight in RRF fusion |

### `ccsearch index`

Rebuilds the search index.

| Flag | Default | Description |
|------|---------|-------------|
| `--days N` | all | Only index sessions from last N days |
| `--force` | | Reindex everything, ignore staleness |
| `--verbose` | | Show per-session progress |

### `ccsearch list`

Lists sessions without searching.

| Flag | Default | Description |
|------|---------|-------------|
| `--days N` | 30 | Last N days |
| `--project PATH` | | Filter by project |
| `--json` | | JSON output |

### `ccsearch config`

Shows current configuration. Creates default config at `~/.ccsearch/config.toml` if none exists.

## How Hybrid Search Works

1. **BM25 (keyword)**: Queries the FTS5 full-text index for exact keyword matches. Good for finding sessions where you used specific terms.

2. **Vector (semantic)**: Embeds the query using all-MiniLM-L6-v2 (384-dim) and finds sessions with similar meaning via cosine distance. Good for finding conceptually related sessions even with different wording.

3. **RRF (fusion)**: Combines both result lists using Reciprocal Rank Fusion:
   ```
   score = bm25_weight / (bm25_rank + k) + vec_weight / (vec_rank + k)
   ```
   where `k=60` by default. This produces a single ranked list that benefits from both approaches.

## TUI Controls

| Key | Action |
|-----|--------|
| `↑`/`↓` or `j`/`k` | Navigate results |
| `Enter` | Resume selected session (`claude --resume`) |
| `/` | Filter within results |
| `g`/`G` | Jump to top/bottom |
| `q`/`Esc` | Quit |

## Architecture

```
CLI (clap) ──> Indexer ──> SQLite DB (FTS5 + embeddings)
    │                              │
    └──> Searcher ◄────────────────┘
             │
         TUI Picker (ratatui)
             │
         claude --resume <id>
```

## Configuration

Config file: `~/.ccsearch/config.toml`

```toml
bm25_weight = 1.0
vec_weight = 1.0
rrf_k = 60.0
max_results = 20
default_days = 30
max_text_chars = 8000
```

## Data Storage

| Path | Contents |
|------|----------|
| `~/.ccsearch/index.db` | SQLite database with FTS5 + vector index |
| `~/.ccsearch/models/` | Downloaded ONNX embedding model (~80MB) |
| `~/.ccsearch/config.toml` | User configuration |

The embedding model is downloaded automatically on first use from HuggingFace.

## Development

```bash
# Run tests
cargo test

# Run with clippy
cargo clippy -- -D warnings

# Format
cargo fmt

# Build release
cargo build --release
```

## License

MIT
