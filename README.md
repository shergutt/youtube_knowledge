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

## Deploying backend changes

The backend is deployed to a Linux VPS with `systemd` under a **symlink-rolled**
pattern: each release is a distinct file, and `systemctl` runs through a
`current` symlink. Updating the symlink + restarting is atomic; on a failed
health check the previous binary is restored.

### Layout on the VPS

```text
/opt/transcript-backend/
├── src/                                  git clone of this repo
│   └── backend/                          Rust crate (cargo build runs here)
├── target/release/
│   ├── transcript-backend-0.1.0+g976a7d9   ← built binary (one per release)
│   ├── transcript-backend-0.1.1+abcd1234
│   └── transcript-backend.current → /opt/transcript-backend/target/release/transcript-backend-0.1.0+g976a7d9
└── deploy.sh                             symlink-rolled deploy script

/etc/systemd/system/transcript-backend.service.d/override.conf
   └─ ExecStart=/opt/transcript-backend/target/release/transcript-backend.current

/var/lib/transcript-backend/
├── cookies.txt                           yt-dlp cookies (mode 0600, owned by `transcript`)
└── storage/jobs/…                        per-job output directories
```

### One-time bootstrap (already done on the production VPS)

1. Install build deps (`build-essential` already present) and rustup user-local
   for `root` (`curl https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal`).
2. `git clone git@github.com:shergutt/youtube_knowledge.git /opt/transcript-backend/src`
3. `cd /opt/transcript-backend/src/backend && cargo build --release`
4. Copy the binary to `/opt/transcript-backend/target/release/transcript-backend-<version>`
5. Symlink `transcript-backend.current` to it
6. Drop the `override.conf` in place and `systemctl daemon-reload`
7. `systemctl restart transcript-backend.service`; verify `/health` returns `{"status":"ok"}`

### Rolling out a new change

On the dev machine:

```bash
git add -A
git commit -m "feat: …"
git push origin master
# (frontend is auto-deployed by Vercel at this point)
```

Deploy the backend to the VPS (run from this repo on the dev machine):

```bash
scripts/deploy-backend.sh
```

That script is **not in the repo** (it is gitignored under `scripts/deploy-*.sh`)
and lives only on your dev machine. It SSHes into the VPS as `root` using your
local SSH key (one-time setup: `ssh-copy-id root@185.245.183.179`) and runs
`/opt/transcript-backend/deploy.sh` on the VPS.

It accepts an optional ref argument and supports a dry-run:

```bash
scripts/deploy-backend.sh v0.1.2         # deploy a specific tag/branch/SHA
scripts/deploy-backend.sh --dry-run      # print the ssh command, don't run it
scripts/deploy-backend.sh --logs         # tail the journal after the deploy
```

Internally the script:

1. `git fetch --tags --prune` in `/opt/transcript-backend/src`
2. `cargo build --release --bin transcript-backend`
3. Copies the new binary to `target/release/transcript-backend-<git describe>`
4. Updates the `current` symlink atomically
5. `systemctl restart transcript-backend.service`
6. Polls `/health` for up to 10 seconds; on failure, restores the previous
   binary's symlink and restarts
7. Garbage-collects everything except the last 5 release binaries and the one
   currently pointed to by `current`

To deploy a specific ref (tag, branch, or SHA) instead of `master` HEAD:

```bash
/opt/transcript-backend/deploy.sh v0.1.2
```

### Emergency back-out

The old binary at `/usr/local/bin/transcript-backend` is preserved for one
release as a safety net. To revert to it:

```bash
sudo rm /etc/systemd/system/transcript-backend.service.d/override.conf
sudo systemctl daemon-reload
sudo systemctl restart transcript-backend.service
```

## Deploying frontend changes

The frontend is hosted on Vercel and connected to this GitHub repo, so every
push to `master` is auto-deployed. PR branches get preview URLs.

To set this up on a fresh project:

1. Go to <https://vercel.com/new> and import `shergutt/youtube_knowledge`
2. Set the **Root Directory** to `frontend`
3. Framework preset: **Vite**
4. Add env var `VITE_API_BASE` (e.g. `https://yt-api.valorix.lat`)
5. Deploy

After that, every push to `master` deploys to the production URL and every PR
gets a preview. Manual `vercel --prod` is no longer needed.
