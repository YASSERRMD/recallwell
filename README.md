# recallwell

> Your knowledge, indexed and citable.

A personal knowledge base built on [pagebridge](https://github.com/YASSERRMD/pagebridge).
Ingest your PDFs, EPUBs, HTML, DOCX, and Markdown files; ask questions; get
answers with real citations that link back to the original source.

**Status:** pre-alpha. Under active development.

## What recallwell is

A single Rust binary that runs on your laptop. It embeds an HTTP server, a
SQLite database, a HTMX-based UI, and the pagebridge engine. You point it at
your documents; it builds a hierarchical knowledge tree; you ask it questions;
it answers with citations and lets you click through to the original passage.

## Why recallwell

Vector RAG over a personal library has problems: uploads to a cloud service,
weak citations, and a subscription bill. recallwell takes the other path:

- **Everything stays local.** Documents, summaries, embeddings, history — all
  on your machine. Only the question and the top-relevant excerpts are sent to
  Groq for synthesis.
- **Citations are first-class.** Every answer comes with citations that link
  back to the original document at the page or section.
- **One binary, no setup.** Download, run, drag files in, ask questions.
- **Your library, your way.** Multiple libraries for reading, work, recipes,
  whatever you need.

## Quickstart (planned)

```bash
# Download the binary for your platform from GitHub Releases
./recallwell setup     # one-time, asks for your Groq API key
./recallwell           # starts the server, opens your browser
```

## License

Dual MIT or Apache-2.0. Pick whichever you prefer.
