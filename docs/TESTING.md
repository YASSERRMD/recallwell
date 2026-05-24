# Testing recallwell

Three ways to run recallwell end-to-end so you can poke at the UI, ingest a
document, and ask a question. Pick whichever fits.

## Prerequisites (all paths)

A Groq API key. Get one for free at <https://console.groq.com> and copy it
somewhere handy — it looks like `gsk_...`.

---

## Path A — Native, fastest (~30 seconds)

Already have the repo cloned and `target/release/recallwell` built?

```bash
export RECALLWELL_GROQ_API_KEY=gsk_your_key_here
./target/release/recallwell --data-dir ./recallwell-data
```

Or if you have not built yet:

```bash
cargo build --release
RECALLWELL_GROQ_API_KEY=gsk_... ./target/release/recallwell --data-dir ./recallwell-data
```

The server prints something like:

```
recallwell v0.1.0
Server running at http://127.0.0.1:7676/?t=Kp9X2vRn8mQs7tWfZ3jY4hLc

Browser opening automatically. Bookmark this URL for this session.
Press Ctrl+C to stop.
```

Open the URL. Drag a small PDF or Markdown file into the ingest drop zone.
Wait for the job card to say "done" (usually 10–30 s for a small file). Type
a question. Click a citation in the answer to open the source.

`Ctrl+C` to stop. State persists in `./recallwell-data/`. Wipe it with
`rm -rf ./recallwell-data` to start fresh.

---

## Path B — Docker (clean isolation, repeatable, ~3–5 min cold build)

You need Docker Desktop running (or any Docker engine with the Compose v2
plugin).

```bash
# 1. Configure your key
cp .env.example .env
$EDITOR .env       # paste RECALLWELL_GROQ_API_KEY=gsk_...

# 2. Build and start
docker compose up --build
```

First build is slow (Rust compiles all the deps + the binary). Subsequent
runs are near-instant. The compose file:

- builds the multi-stage image from `Dockerfile`
- publishes container port `7676` on `localhost:7676`
- mounts a named volume `recallwell-data` for persistence
- runs the binary as a non-root user

Watch the container logs for the URL with the token:

```bash
docker compose logs -f recallwell
```

You will see a line like:

```
Server running at http://0.0.0.0:7676/?t=Kp9X2vRn8mQs7tWfZ3jY4hLc
```

Replace `0.0.0.0` with `localhost` in your browser:

```
http://localhost:7676/?t=Kp9X2vRn8mQs7tWfZ3jY4hLc
```

### Manage the container

```bash
docker compose ps                  # status
docker compose logs -f recallwell  # tail logs
docker compose restart recallwell  # restart (keeps data)
docker compose down                # stop, keep volume
docker compose down -v             # stop and wipe data
```

### Run from the pre-built image (without compose)

If you just want to spin up the image once:

```bash
docker build -t recallwell .
docker run --rm -it \
  -p 7676:7676 \
  -e RECALLWELL_GROQ_API_KEY=gsk_... \
  -v $(pwd)/recallwell-data:/data \
  recallwell
```

---

## Path C — Pre-built release binary (no toolchain at all)

Once a v0.1.0 release exists on GitHub:

```bash
curl -L https://github.com/yasserrmd/recallwell/releases/download/v0.1.0/recallwell-v0.1.0-$(uname -m)-apple-darwin.tar.gz \
  | tar xz
./recallwell setup       # interactive: prompts for the Groq key
./recallwell             # serves on http://localhost:7676/?t=...
```

---

## What to try once it is up

1. **Ingest something.** Drag in a short PDF (a paper or a chapter works
   best), or a couple of `.md` files. Watch the job card move through
   `parsing -> ingesting -> summarizing -> done`. Bigger docs take longer
   because pagebridge summarises every node via Groq.
2. **Ask a question.** Type something like *"what does the paper say about
   X?"*. Tokens stream into the answer panel. Citations appear in the right
   column as they are decided.
3. **Click a citation.** It opens the original file in a new tab (PDFs jump
   to the cited page via `#page=N`).
4. **Switch libraries.** Click the `library:default` chip in the header, type
   `notes` in the input at the bottom of the dropdown, hit `add`. The page
   reloads with `notes` active and the ingest zone is empty.
5. **Browse history.** The right sidebar lists past asks. The history DB is
   `history.db` in your data dir; it survives restarts.
6. **Export an answer.** Hit the export endpoint with the ask id, e.g.
   `curl "http://localhost:7676/api/history/<ID>/export?t=<TOKEN>" -O` and
   open the resulting `.md` in your notes app.

## Common issues

| Symptom | Fix |
|---|---|
| `unauthorized` page | URL is missing `?t=<token>`. Copy the full URL from the terminal logs, not `localhost:7676` on its own. |
| Container exits with "Groq API key not set" | `.env` is missing or `RECALLWELL_GROQ_API_KEY` is blank. |
| Ingest fails with "pdf appears to contain no extractable text" | Scanned PDF; OCR is on the v0.2 roadmap. Try a different file. |
| Long delay on first ask after ingest | pagebridge waits for all node summaries before answering; large docs need a minute or so the first time. |
| Port 7676 already in use | Set `RECALLWELL_PUBLIC_PORT=8000` in `.env` (Docker) or `--port 8000` (native). |
