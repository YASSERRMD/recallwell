# Configuration

recallwell reads configuration in this order, with later sources overriding
earlier ones:

1. Hard-coded defaults
2. `config.toml` at the OS-standard location
3. Environment variables
4. CLI flags

## Locations

| Platform | Config file | Data directory |
|----------|-------------|----------------|
| Linux | `~/.config/recallwell/config.toml` | `~/.local/share/recallwell/` |
| macOS | `~/Library/Application Support/com.recallwell.recallwell/config.toml` | same dir |
| Windows | `%APPDATA%\com\recallwell\recallwell\config.toml` | same dir |

You can override either with `--config <path>` or `--data-dir <path>` on the
command line.

## Reference

```toml
# Groq API settings.
[groq]
api_key = "gsk_..."                          # required (or via env)
synthesis_model = "llama-3.3-70b-versatile"  # used for the final answer
navigation_model = "llama-3.1-8b-instant"    # used for tree traversal
base_url = "https://api.groq.com/openai/v1"

# Server bind settings.
[server]
host = "127.0.0.1"
port = 7676
auto_open = true   # launch the browser when the server starts

# Where recallwell stores libraries, history, and ingested-file mirrors.
[data]
# Leave unset to use the OS-standard data dir.
# dir = "/custom/path"

# UI theme.
[ui]
theme = "auto"   # "light", "dark", "auto"

# Ingest pipeline.
[ingest]
max_concurrent = 2     # how many files to process in parallel
ocr_fallback = false   # OCR for scanned PDFs (v0.2)

# Ask defaults (mostly wired into pagebridge internals).
[ask]
max_navigation_steps = 4
beam_width = 3
bm25_candidate_limit = 30
max_leaves = 8
synthesis_temperature = 0.2
navigation_temperature = 0.0
```

## Environment variables

| Variable | Equivalent config |
|----------|-------------------|
| `RECALLWELL_GROQ_API_KEY` | `[groq].api_key` |
| `RECALLWELL_DATA_DIR` | `[data].dir` |
| `RECALLWELL_PORT` | `[server].port` |
| `RUST_LOG` | tracing filter (e.g. `recallwell=debug,info`) |

## CLI flags

| Flag | Effect |
|------|--------|
| `--data-dir <PATH>` | override the data directory |
| `--config <PATH>` | override the config file path |
| `--verbose` | enable debug-level logs |
| `--port <N>` (on `serve`) | override the port |
| `--auto-open <bool>` (on `serve`) | toggle browser open |

## Picking a Groq model

Groq's hosted models (as of May 2026):

- `llama-3.3-70b-versatile` — best quality, ~250 tok/s. Default for synthesis.
- `llama-3.1-8b-instant` — fastest, ~750 tok/s. Default for navigation.
- `mixtral-8x7b-32768` — large context, useful for synthesizing long answers.

For navigation, faster is strictly better; the 8B model is the right call.
For synthesis, the 70B model is the default; switch to mixtral if you need
longer outputs.

## File system layout

Under the data directory:

```
recallwell-data/
  config.toml              (in OS config dir, not data)
  state.json               active library name
  history.db               SQLite + FTS5, all asks across libraries
  libraries/
    default.db             pagebridge SQLite
    reading.db
    work.db
  ingested-files/
    default/
      <job_ulid>/
        original-filename.pdf
      sources.json         doc_id -> file mapping
```

Everything here is plain SQLite or JSON, so you can inspect or back up with
the usual tools.
