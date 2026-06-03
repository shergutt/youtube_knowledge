FROM rust:1-bookworm AS builder

WORKDIR /app

# Cache dependencies separately
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && \
    echo "fn main() { println!(\"placeholder\"); }" > src/main.rs && \
    cargo build --release --bin transcript-backend && \
    rm -rf src target/release/deps/transcript_backend*

COPY src ./src
RUN cargo build --release --bin transcript-backend

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        python3 \
        python3-pip \
        ffmpeg \
        curl \
        tini \
    && rm -rf /var/lib/apt/lists/*

RUN python3 -m pip install --break-system-packages --no-cache-dir yt-dlp

WORKDIR /app

COPY --from=builder /app/target/release/transcript-backend /app/transcript-backend

RUN mkdir -p /app/storage/jobs /app/storage/cache

ENV APP_HOST=0.0.0.0 \
    APP_PORT=8080 \
    STORAGE_DIR=/app/storage \
    MAX_CONCURRENT_JOBS=2 \
    JOB_TIMEOUT_SECONDS=60 \
    FILE_TTL_MINUTES=60 \
    MAX_URL_LENGTH=2048 \
    MAX_TRANSCRIPT_FILE_MB=10 \
    CORS_ORIGIN=http://localhost:3000 \
    ANALYSIS_TIMEOUT_SECONDS=1200 \
    ANALYSIS_MAX_OUTPUT_TOKENS=16384 \
    ANALYSIS_TEMPERATURE=1.0 \
    ANALYSIS_TOP_P=0.95

EXPOSE 8080

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/app/transcript-backend"]
