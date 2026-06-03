# Rust YouTube Transcript Downloader — Agent Implementation Prompt

## Role

You are a senior full-stack software engineer and Rust backend developer.

Build a production-ready MVP for a **Rust YouTube Transcript Downloader** web application.

The app allows a user to paste a YouTube video URL, detect available captions, select a language/output format, and download the transcript when captions are available.

---

## Critical Constraint

Do **not** use the official YouTube Data API.

Do **not** use:

- YouTube API keys
- OAuth
- Google Cloud credentials
- YouTube `captions.download`
- Any official YouTube Data API endpoint

The backend must use `yt-dlp` as a controlled subprocess from Rust.

The Rust backend is responsible for validating inputs, safely executing `yt-dlp`, parsing outputs, managing jobs, generating transcript files, and serving downloads.

---

## Recommended Stack

| Layer | Technology |
|---|---|
| Frontend | React + Vite + TypeScript |
| Backend | Rust + Axum |
| Async runtime | Tokio |
| Transcript engine | `yt-dlp` subprocess |
| Storage MVP | Local filesystem |
| Storage later | SQLite |
| Deployment | Docker / Docker Compose |

---

## Product Goal

Create a web app where the user can:

1. Paste a YouTube video URL.
2. Check available manual and auto-generated captions.
3. Select caption source:
   - Manual captions
   - Auto-generated captions
   - Best available
4. Select language.
5. Select output format:
   - `vtt`
   - `srt`
   - `txt`
   - `json`
6. Generate the transcript.
7. Download the transcript file.

---

## MVP Scope

### Include

- Single YouTube video URLs only.
- URL validation.
- Caption probing.
- Manual caption downloads.
- Auto-generated caption downloads.
- Job-based transcript generation.
- Download endpoint.
- Local temporary file storage.
- File cleanup.
- Basic frontend UI.
- Docker deployment.

### Exclude From MVP

Do not implement these in the first version:

- YouTube login
- Cookies
- Playlists
- Channels
- Bulk downloads
- User accounts
- Payments
- AI summarization
- Translation
- Permanent database storage
- Browser extension
- Queue dashboard

---

## Supported YouTube URL Formats

The backend should support only these URL types:

```text
https://www.youtube.com/watch?v=VIDEO_ID
https://youtube.com/watch?v=VIDEO_ID
https://youtu.be/VIDEO_ID
https://www.youtube.com/shorts/VIDEO_ID
https://youtube.com/shorts/VIDEO_ID
```

Reject:

```text
youtube.com/playlist
youtube.com/channel
youtube.com/@handle
youtube.com/results
youtube.com/feed
non-youtube domains
file:// URLs
localhost URLs
private IP URLs
```

Extract the video ID and convert all valid input into this canonical format:

```text
https://www.youtube.com/watch?v=VIDEO_ID
```

Use the canonical URL for all `yt-dlp` calls.

---

## Backend Requirements

### Framework

Use:

- Rust
- Axum
- Tokio
- Serde
- Tower HTTP
- Tracing

### Required Backend Endpoints

Implement these endpoints:

```http
GET /health
POST /api/probe
POST /api/transcripts
GET /api/jobs/:job_id
GET /api/downloads/:job_id
```

---

## Endpoint Details

### 1. Health Check

```http
GET /health
```

Response:

```json
{
  "status": "ok"
}
```

---

### 2. Probe Captions

```http
POST /api/probe
```

Request:

```json
{
  "url": "https://www.youtube.com/watch?v=VIDEO_ID"
}
```

Response:

```json
{
  "video_id": "VIDEO_ID",
  "title": "Example Video",
  "duration_seconds": 523,
  "thumbnail_url": "https://...",
  "manual_captions": [
    {
      "language": "en",
      "name": "English",
      "formats": ["vtt", "srv3", "ttml"]
    }
  ],
  "automatic_captions": [
    {
      "language": "en",
      "name": "English auto-generated",
      "formats": ["vtt", "srv3", "ttml"]
    }
  ]
}
```

Use this `yt-dlp` command:

```bash
yt-dlp -J --skip-download --no-playlist --no-warnings CANONICAL_URL
```

Parse the JSON output and extract:

- Video ID
- Title
- Duration
- Thumbnail
- Manual captions
- Automatic captions

If the video has no captions, return empty arrays instead of a server error.

---

### 3. Create Transcript Job

```http
POST /api/transcripts
```

Request:

```json
{
  "url": "https://www.youtube.com/watch?v=VIDEO_ID",
  "language": "en",
  "caption_source": "manual",
  "output_format": "txt"
}
```

Allowed values:

```text
caption_source = manual | auto | best
output_format = vtt | srt | txt | json
```

Response:

```json
{
  "job_id": "UUID",
  "status": "queued"
}
```

This endpoint should:

1. Validate URL.
2. Extract canonical video ID.
3. Validate language.
4. Validate output format.
5. Create a job.
6. Spawn a background Tokio task.
7. Return the job ID immediately.

---

### 4. Check Job Status

```http
GET /api/jobs/:job_id
```

Queued/running response:

```json
{
  "job_id": "UUID",
  "status": "running",
  "progress": 50
}
```

Completed response:

```json
{
  "job_id": "UUID",
  "status": "completed",
  "download_url": "/api/downloads/UUID"
}
```

Failed response:

```json
{
  "job_id": "UUID",
  "status": "failed",
  "error": {
    "code": "NO_CAPTIONS_FOUND",
    "message": "No captions were found for the selected language."
  }
}
```

---

### 5. Download Transcript

```http
GET /api/downloads/:job_id
```

Return the transcript file as an attachment.

Example headers:

```http
Content-Type: text/plain
Content-Disposition: attachment; filename="VIDEO_ID.en.txt"
```

Only serve files that:

- Belong to a valid completed job.
- Are inside the configured storage directory.
- Match an expected transcript format.
- Have not expired.

Never expose raw server paths.

---

## `yt-dlp` Integration

Create a service similar to:

```rust
pub struct YtDlpService {
    config: AppConfig,
}
```

Use `tokio::process::Command`.

Never use shell commands.

### Incorrect

```rust
Command::new("sh")
    .arg("-c")
    .arg(format!("yt-dlp {}", user_url));
```

### Correct

```rust
Command::new("yt-dlp")
    .args([
        "-J",
        "--skip-download",
        "--no-playlist",
        "--no-warnings",
        canonical_url,
    ])
```

---

## Commands

### Probe Metadata

```bash
yt-dlp -J --skip-download --no-playlist --no-warnings CANONICAL_URL
```

---

### Download Manual Captions

```bash
yt-dlp   --skip-download   --write-subs   --sub-langs LANG   --sub-format vtt/srt/best   --no-playlist   -P subtitle:JOB_DIR   -o subtitle:%(id)s.%(ext)s   CANONICAL_URL
```

---

### Download Auto Captions

```bash
yt-dlp   --skip-download   --write-auto-subs   --sub-langs LANG   --sub-format vtt/srt/best   --no-playlist   -P subtitle:JOB_DIR   -o subtitle:%(id)s.%(ext)s   CANONICAL_URL
```

---

### SRT Output

For SRT output, use:

```bash
yt-dlp   --skip-download   --write-subs   --sub-langs LANG   --sub-format vtt/srt/best   --convert-subs srt   --no-playlist   -P subtitle:JOB_DIR   -o subtitle:%(id)s.%(ext)s   CANONICAL_URL
```

For auto captions, replace `--write-subs` with `--write-auto-subs`.

---

### TXT Output

For TXT:

1. Download VTT.
2. Parse VTT in Rust.
3. Remove timestamps, cue IDs, metadata, and duplicate blank lines.
4. Write a clean `.txt` file.

---

### JSON Output

For JSON:

1. Download VTT.
2. Parse VTT in Rust.
3. Convert each cue to a segment.

Output shape:

```json
{
  "video_id": "VIDEO_ID",
  "language": "en",
  "source": "auto",
  "segments": [
    {
      "start": "00:00:01.000",
      "end": "00:00:04.000",
      "text": "Hello and welcome."
    }
  ]
}
```

---

## Caption Source Logic

### Manual

Use only:

```text
--write-subs
```

### Auto

Use only:

```text
--write-auto-subs
```

### Best

Implement this logic:

```text
1. Try manual captions first.
2. If no file is generated, try auto-generated captions.
3. If still no file is generated, fail with NO_CAPTIONS_FOUND.
```

Do not pass both `--write-subs` and `--write-auto-subs` in the MVP.

---

## Backend File Structure

Create this structure:

```text
backend/
  Cargo.toml
  Dockerfile
  src/
    main.rs
    config.rs
    app_state.rs

    routes/
      mod.rs
      health.rs
      probe.rs
      jobs.rs
      downloads.rs

    services/
      mod.rs
      youtube_url.rs
      ytdlp.rs
      transcript.rs
      cleanup.rs

    models/
      mod.rs
      api.rs
      job.rs
      transcript.rs

    errors/
      mod.rs
      app_error.rs

    utils/
      mod.rs
      files.rs
      time.rs

  storage/
    jobs/
    cache/
```

---

## Suggested Rust Dependencies

Use these dependencies:

```toml
[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tower-http = { version = "0.6", features = ["cors", "trace", "limit", "timeout"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
thiserror = "2"
tracing = "0.1"
tracing-subscriber = "0.3"
regex = "1"
url = "2"
tempfile = "3"
chrono = { version = "0.4", features = ["serde"] }
mime_guess = "2"
```

Optional later:

```toml
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "uuid", "chrono", "migrate"] }
```

---

## Core Backend Models

### Caption Source

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptionSource {
    Manual,
    Auto,
    Best,
}
```

### Output Format

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Vtt,
    Srt,
    Txt,
    Json,
}
```

### Job Status

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Expired,
}
```

### Transcript Segment

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub start: String,
    pub end: String,
    pub text: String,
}
```

### API Error Body

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
}
```

---

## In-Memory Job Store

For the MVP, use an in-memory job store:

```rust
pub struct AppState {
    pub jobs: Arc<RwLock<HashMap<Uuid, TranscriptJob>>>,
    pub semaphore: Arc<Semaphore>,
    pub config: AppConfig,
}
```

Use SQLite only after the MVP is working.

---

## Configuration

Load config from environment variables:

```text
APP_HOST=0.0.0.0
APP_PORT=8080
STORAGE_DIR=/app/storage
MAX_CONCURRENT_JOBS=2
JOB_TIMEOUT_SECONDS=60
FILE_TTL_MINUTES=60
MAX_URL_LENGTH=2048
MAX_TRANSCRIPT_FILE_MB=10
CORS_ORIGIN=http://localhost:5173
```

Create:

```rust
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub storage_dir: PathBuf,
    pub max_concurrent_jobs: usize,
    pub job_timeout_seconds: u64,
    pub file_ttl_minutes: i64,
    pub max_url_length: usize,
    pub max_transcript_file_mb: u64,
    pub cors_origin: String,
}
```

---

## Security Requirements

Implement all of these:

- Never use shell execution.
- Never allow arbitrary `yt-dlp` flags from the frontend.
- Validate URL and extract the video ID before calling `yt-dlp`.
- Use only canonical YouTube video URLs.
- Reject playlists and channels.
- Use `--no-playlist`.
- Limit concurrent jobs with a semaphore.
- Add subprocess timeout.
- Store every job in a unique job directory.
- Serve only files linked to completed jobs.
- Delete expired files.
- Restrict CORS in production.
- Do not expose raw `yt-dlp` stderr to users.
- Log backend errors with `tracing`.
- Add basic request body size limits.
- Add rate limiting if exposing publicly.

---

## Error Codes

Normalize backend errors using these codes:

```text
INVALID_URL
UNSUPPORTED_URL_TYPE
YTDLP_NOT_INSTALLED
YTDLP_TIMEOUT
YTDLP_FAILED
YTDLP_METADATA_PARSE_FAILED
NO_CAPTIONS_FOUND
CAPTION_FORMAT_UNAVAILABLE
TRANSCRIPT_PARSE_FAILED
JOB_NOT_FOUND
FILE_EXPIRED
INTERNAL_ERROR
```

Every error response should follow this format:

```json
{
  "error": {
    "code": "INVALID_URL",
    "message": "Only single YouTube video URLs are supported."
  }
}
```

---

## Frontend Requirements

Use:

- React
- Vite
- TypeScript

Frontend should include:

```text
frontend/
  package.json
  vite.config.ts
  src/
    main.tsx
    App.tsx

    api/
      client.ts
      transcripts.ts

    components/
      UrlInput.tsx
      CaptionSelector.tsx
      FormatSelector.tsx
      JobStatus.tsx
      DownloadButton.tsx
      ErrorMessage.tsx

    types/
      api.ts

    utils/
      validators.ts
```

---

## Frontend Flow

Implement this flow:

```text
1. User pastes YouTube URL.
2. User clicks "Check captions".
3. Frontend calls POST /api/probe.
4. Show video title, thumbnail, and available captions.
5. User selects:
   - Caption source
   - Language
   - Output format
6. User clicks "Generate transcript".
7. Frontend calls POST /api/transcripts.
8. Frontend polls GET /api/jobs/:job_id every 1–2 seconds.
9. When complete, show download button.
10. Download button links to GET /api/downloads/:job_id.
```

---

## Frontend Types

Create these TypeScript types:

```ts
export type CaptionSource = "manual" | "auto" | "best";
export type OutputFormat = "vtt" | "srt" | "txt" | "json";

export interface ProbeRequest {
  url: string;
}

export interface CaptionTrack {
  language: string;
  name: string;
  formats: string[];
}

export interface ProbeResponse {
  video_id: string;
  title: string;
  duration_seconds?: number;
  thumbnail_url?: string;
  manual_captions: CaptionTrack[];
  automatic_captions: CaptionTrack[];
}

export interface CreateTranscriptRequest {
  url: string;
  language: string;
  caption_source: CaptionSource;
  output_format: OutputFormat;
}

export interface CreateTranscriptResponse {
  job_id: string;
  status: "queued";
}

export interface JobResponse {
  job_id: string;
  status: "queued" | "running" | "completed" | "failed" | "expired";
  progress?: number;
  download_url?: string;
  error?: {
    code: string;
    message: string;
  };
}
```

---

## Docker Requirements

Create:

```text
Dockerfile for backend
Dockerfile for frontend
docker-compose.yml
```

The backend container must include:

- Rust binary
- Python 3
- `yt-dlp`
- `ffmpeg`
- CA certificates

---

## Backend Dockerfile

Use this as a starting point:

```dockerfile
FROM rust:1-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y     ca-certificates     python3     python3-pip     ffmpeg     curl     && rm -rf /var/lib/apt/lists/*

RUN python3 -m pip install --break-system-packages yt-dlp

WORKDIR /app

COPY --from=builder /app/target/release/transcript-backend /app/transcript-backend

RUN mkdir -p /app/storage/jobs /app/storage/cache

ENV APP_HOST=0.0.0.0
ENV APP_PORT=8080
ENV STORAGE_DIR=/app/storage
ENV MAX_CONCURRENT_JOBS=2
ENV JOB_TIMEOUT_SECONDS=60
ENV FILE_TTL_MINUTES=60

EXPOSE 8080

CMD ["/app/transcript-backend"]
```

---

## Docker Compose

Use this as a starting point:

```yaml
services:
  backend:
    build:
      context: ./backend
    container_name: transcript-backend
    restart: unless-stopped
    environment:
      APP_HOST: "0.0.0.0"
      APP_PORT: "8080"
      STORAGE_DIR: "/app/storage"
      MAX_CONCURRENT_JOBS: "2"
      JOB_TIMEOUT_SECONDS: "60"
      FILE_TTL_MINUTES: "60"
      CORS_ORIGIN: "http://localhost:3000"
    volumes:
      - transcript_storage:/app/storage
    ports:
      - "8080:8080"

  frontend:
    build:
      context: ./frontend
    container_name: transcript-frontend
    restart: unless-stopped
    ports:
      - "3000:80"
    depends_on:
      - backend

volumes:
  transcript_storage:
```

---

## Implementation Milestones

### Milestone 1 — Backend Skeleton

Tasks:

- Create Rust project.
- Add Axum.
- Add `/health`.
- Add config loader.
- Add error system.
- Add tracing.
- Add CORS.

Acceptance criteria:

```text
GET /health returns { "status": "ok" }.
Backend starts using environment config.
Errors are returned as JSON.
```

---

### Milestone 2 — URL Validation

Tasks:

- Implement `extract_video_id`.
- Support `watch`, `youtu.be`, and `shorts`.
- Reject playlists, channels, handles, and non-YouTube domains.
- Add unit tests.

Acceptance criteria:

```text
Valid URLs return a video ID.
Invalid URLs return INVALID_URL.
Unsupported YouTube URL types return UNSUPPORTED_URL_TYPE.
```

---

### Milestone 3 — Probe Endpoint

Tasks:

- Implement `YtDlpService`.
- Run metadata probe command.
- Parse JSON response.
- Extract manual and automatic captions.
- Add `/api/probe`.

Acceptance criteria:

```text
POST /api/probe returns video metadata and caption tracks.
Videos without captions return empty caption arrays.
```

---

### Milestone 4 — Job System

Tasks:

- Create in-memory job store.
- Add job model.
- Add job status endpoint.
- Spawn background transcript task.
- Add semaphore concurrency limit.

Acceptance criteria:

```text
POST /api/transcripts returns a job ID.
GET /api/jobs/:id shows queued, running, completed, or failed.
Concurrency limit is enforced.
```

---

### Milestone 5 — Transcript Generation

Tasks:

- Run `yt-dlp` for manual captions.
- Run `yt-dlp` for auto captions.
- Implement best mode.
- Store generated file in job directory.
- Detect generated file.
- Return failure if no caption file exists.

Acceptance criteria:

```text
User can generate VTT and SRT transcripts.
No video file is downloaded.
Only subtitle files are stored.
```

---

### Milestone 6 — TXT and JSON Conversion

Tasks:

- Parse VTT files.
- Convert VTT to clean TXT.
- Convert VTT to structured JSON.
- Add tests with sample VTT.

Acceptance criteria:

```text
TXT output is readable.
JSON output contains timestamped segments.
```

---

### Milestone 7 — Frontend MVP

Tasks:

- Create React + Vite app.
- Build URL input.
- Build caption selector.
- Build format selector.
- Build job polling.
- Build download button.
- Show clear error states.

Acceptance criteria:

```text
User can paste a URL, check captions, select options, generate transcript, and download the file.
```

---

### Milestone 8 — Docker Deployment

Tasks:

- Create backend Dockerfile.
- Create frontend Dockerfile.
- Create docker-compose.yml.
- Add persistent storage volume.
- Test full app with Docker Compose.

Acceptance criteria:

```text
docker compose up -d starts the full app.
Frontend can call backend.
Transcript generation works inside Docker.
```

---

## Final Build Order

Follow this exact order:

```text
1. Backend skeleton
2. URL validation
3. yt-dlp probe endpoint
4. Job system
5. Transcript generation
6. TXT/JSON conversion
7. React frontend
8. Docker deployment
9. Optional SQLite cache
```

---

## Quality Bar

Before finishing, verify:

- `cargo fmt` passes.
- `cargo clippy` has no serious warnings.
- Backend unit tests pass.
- Frontend TypeScript build passes.
- Docker Compose build succeeds.
- App works with at least:
  - one video with manual captions
  - one video with auto captions
  - one video without captions

---

## Do Not Skip

The implementation is not complete unless these are working:

- `POST /api/probe`
- `POST /api/transcripts`
- `GET /api/jobs/:job_id`
- `GET /api/downloads/:job_id`
- Safe subprocess execution
- URL validation
- Transcript download
- TXT conversion
- JSON conversion
- Docker Compose deployment
