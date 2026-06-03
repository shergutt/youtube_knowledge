# YouTube Transcript Downloader

A self-hosted web app that downloads YouTube video transcripts (captions) using `yt-dlp` as a subprocess.

It does **not** use the YouTube Data API, OAuth, API keys, or cookies.

## Features

- Probe a YouTube video and list its manual + auto-generated caption tracks,
  with human-readable names in 40+ languages (English, **Spanish**,
  Portuguese, French, German, Italian, Japanese, Chinese, etc.).
- Download captions as **VTT**, **SRT**, **TXT**, or **JSON**.
- Choose caption source: `manual`, `auto`, or `best` (manual first, then auto).
- Quick-pick chips for the most common caption languages: EN, ES, PT, FR, DE,
  IT, JA, KO, ZH.
- **AI analysis** via MiniMax-M3: turn any transcript into a structured markdown
  document (summary, study notes, key takeaways, action items, blog post,
  tutorial, or a custom goal).
- AI analysis **output language** is configurable. Pick any of the quick
  languages (EN, ES, PT, FR, DE, IT, JA) or type a BCP-47 code. When Spanish
  (`es`, `es-ES`, `es-MX`, `es-AR`, `es-419`, …) is selected, the model is
  explicitly instructed to write in natural Spanish and to preserve accents
  (á, é, í, ó, ú, ñ, ü) and idioms.
- Job-based generation with concurrency limits and per-job timeouts.
- Auto-cleanup of expired files (transcripts, analyses, and playlist
  packages).

## Architecture

```
┌────────────┐    /api/*    ┌─────────────────┐    subprocess    ┌────────┐
│  Frontend  │ ───────────▶ │ Rust + Axum API │ ───────────────▶ │ yt-dlp │
│ React/Vite │              │   (Tokio async) │                 └────────┘
└────────────┘              └─────────────────┘
                                       │
                                       │  HTTP /v1/messages
                                       ▼
                              ┌─────────────────┐
                              │  MiniMax-M3 API │
                              └─────────────────┘
                                       │
                                       ▼
                              ┌─────────────────┐
                              │ Local FS (jobs) │
                              └─────────────────┘
```

## Endpoints

| Method | Path                                  | Description                                  |
| ------ | ------------------------------------- | -------------------------------------------- |
| GET    | `/health`                             | Health check (reports `analyzer` configured) |
| POST   | `/api/probe`                          | List caption tracks                          |
| POST   | `/api/transcripts`                    | Create a transcript job                      |
| GET    | `/api/jobs/{job_id}`                  | Check transcript job status                  |
| GET    | `/api/downloads/{job_id}`             | Download generated transcript file            |
| POST   | `/api/analyze`                        | Create an AI analysis job                    |
| GET    | `/api/analyses/{id}`                  | Check analysis job status                    |
| GET    | `/api/analyses/{id}/download`         | Download generated `.md` analysis            |

## Configuration

Backend reads these environment variables (defaults shown):

```text
APP_HOST=0.0.0.0
APP_PORT=8080
STORAGE_DIR=./storage
MAX_CONCURRENT_JOBS=2
JOB_TIMEOUT_SECONDS=60
FILE_TTL_MINUTES=60
MAX_URL_LENGTH=2048
MAX_TRANSCRIPT_FILE_MB=10
CORS_ORIGIN=http://localhost:5173
MAX_PLAYLIST_VIDEOS=50
MAX_PLAYLIST_CONCURRENT_VIDEOS=2        # worker pool size per playlist
MAX_PLAYLIST_ZIP_MB=200

# AI analyzer (MiniMax-M3, Anthropic-compatible API)
MINIMAX_API_KEY=                       # required for AI analysis
MINIMAX_BASE_URL=https://api.minimax.io/anthropic
MINIMAX_MODEL=MiniMax-M3
ANALYSIS_MAX_OUTPUT_TOKENS=16384       # M3 supports up to 524288
ANALYSIS_TIMEOUT_SECONDS=1200          # 20 minutes
ANALYSIS_TEMPERATURE=1.0               # M3-recommended
ANALYSIS_TOP_P=0.95                    # M3-recommended default
MAX_ANALYSIS_FILE_MB=5
MAX_TRANSCRIPT_CHARS_FOR_ANALYSIS=400000
```

## Running with Docker Compose

```bash
docker compose up -d --build
```

Then open <http://localhost:3000>.

## Running locally (development)

Backend:

```bash
cd backend
cargo run
```

Frontend:

```bash
cd frontend
npm install
npm run dev
```

The Vite dev server proxies `/api` and `/health` to `http://localhost:8080` by default. Override with `VITE_API_TARGET`.

## Tests

```bash
cd backend
cargo test
```
