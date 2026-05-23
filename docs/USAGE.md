# Using recallwell

This guide walks through the typical recallwell session.

## First run

```bash
./recallwell setup
```

The setup wizard:

1. Asks for your Groq API key (get one at <https://console.groq.com>).
2. Writes the config file to the OS-standard location.
3. Creates the default library directory.

## Starting the server

```bash
./recallwell           # default subcommand is `serve`
# or explicitly:
./recallwell serve --port 7676
```

When the server starts it prints a URL containing a one-time token, opens
your browser to that URL (unless you passed `--auto-open false`), and waits
for Ctrl+C.

## Ingesting documents

In the UI:

1. Pick or create a library from the header dropdown.
2. Drag PDFs, EPUBs, HTML pages, DOCX, or Markdown files onto the drop zone in
   the sidebar.
3. Each file becomes a job card with a live progress bar:
   - **Parsing**: format-specific parser extracts text.
   - **Ingesting**: text is handed to pagebridge, which builds the section tree.
   - **Summarizing**: pagebridge requests summaries for each tree node from Groq.
4. Jobs disappear from the UI 5 seconds after they complete.

You can ingest multiple files at once. The default concurrency is 2; raise it
via `[ingest].max_concurrent` in the config.

## Asking questions

Type a question into the main textarea and press Ask (or Ctrl+Enter inside
the textarea will submit through the form).

The answer streams in token by token. Citations appear in a panel below
the answer as they arrive; click "Open source" on any citation to open the
original document at the right page.

If the model used numeric footnote markers (`[^1]`, `[^2]`), they're rendered
inline in the answer with the citations panel as the legend.

## Switching libraries

Click the library name in the header to open the dropdown. You can switch to
an existing library, or type a name and click "add" to create a new one.

Switching reloads the page to apply the new active library across the UI.

## Browsing history

Every successful ask is recorded in the global history database
(`history.db`). The sidebar shows the most recent entries.

To search history, hit the `/api/history/search?q=...` endpoint directly for
now; a search box in the sidebar is on the v0.2 roadmap.

## Exporting an answer

Each past ask can be exported as Markdown via
`GET /api/history/:id/export`. The exported document includes the question,
the answer, and a Citations section with [^N] footnotes.

```bash
curl "http://localhost:7676/api/history/<ID>/export?t=<TOKEN>" -O
```

## Keyboard shortcuts

| Shortcut | Action |
|----------|--------|
| Ctrl/Cmd K | focus the ask textarea |
| Ctrl/Cmd L | open the library switcher |
| Ctrl/Cmd / | show shortcuts help overlay |
| Esc | close any overlay |

## Common gotchas

- **PDFs with no extractable text** (i.e. scanned books) fail with a clear
  error. OCR is on the v0.2 roadmap.
- **Rate limits**: Groq's free tier is generous but bursty ingest of large
  PDFs may hit it. The job state shows "failed" with the error message.
- **Closing the browser tab during a stream**: the server cancels the
  pagebridge task; no tokens are wasted on output you'll never see.

## Resetting

To start over with a clean state:

```bash
rm -rf ~/.local/share/recallwell           # Linux
rm -rf ~/Library/Application\ Support/com.recallwell.recallwell    # macOS
```

This deletes all libraries, history, and ingested file mirrors. The config
file (with your Groq API key) is preserved.
