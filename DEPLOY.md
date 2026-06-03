# Deploying `youtube_knowledge`

Single source of truth for shipping changes to this app. The repo is a
monorepo with a Rust + Axum backend and a React + Vite frontend. The
backend runs on a private Linux VPS behind a Cloudflare Tunnel; the
frontend is on Vercel.

```text
dev (this machine)                               VPS (185.245.183.179)
┌──────────────────────────────┐    ssh         ┌─────────────────────────────────┐
│  edit + cargo test           │ ──────────────▶│  /opt/transcript-backend/src    │
│  git push origin master      │                │  /opt/transcript-backend/deploy.sh
│                              │                │  /opt/transcript-backend/target/release/
│  scripts/deploy-backend.sh   │                │    transcript-backend-<version>  │
│   (gitignored, local only)   │                │    transcript-backend.current    │
│                              │                │  systemd: transcript-backend.service
└──────────────────────────────┘                └─────────────────────────────────┘
                  │
                  │  git push origin master
                  ▼
        ┌──────────────────────┐
        │  GitHub: shergutt/   │
        │  youtube_knowledge   │──▶ Vercel auto-deploys frontend
        └──────────────────────┘    on every push to `master`
```

---

## 1. First-time setup (only run once per environment)

### 1.1. VPS prerequisites

The VPS is a Debian/Ubuntu host running `systemd`. The backend is deployed
as a non-Docker systemd service owned by a dedicated `transcript` user.

One-time actions on the VPS (all automated by `scripts/bootstrap_vps.py`
on the dev machine; see "Replaying the bootstrap" below):

| Step | What | Where on the VPS |
|---|---|---|
| 1 | Install rustup user-local for `root` (only the `transcript-backend` service uses the resulting toolchain for rebuilds) | `~/.cargo`, `~/.rustup` |
| 2 | `git clone` the repo | `/opt/transcript-backend/src/` |
| 3 | First `cargo build --release --bin transcript-backend` (1–3 min) | `/opt/transcript-backend/src/backend/target/release/transcript-backend` |
| 4 | Stage the binary under a versioned name | `/opt/transcript-backend/target/release/transcript-backend-<version>` |
| 5 | Symlink the live binary | `/opt/transcript-backend/target/release/transcript-backend.current` |
| 6 | Install the systemd drop-in that points ExecStart at the symlink | `/etc/systemd/system/transcript-backend.service.d/override.conf` |
| 7 | Write the VPS-side deploy script | `/opt/transcript-backend/deploy.sh` |
| 8 | Start / verify | `systemctl restart transcript-backend.service`; `curl http://127.0.0.1:38765/health` |

The bootstrap script touches **only**:

- `/opt/transcript-backend/` (new directory tree)
- `/etc/systemd/system/transcript-backend.service.d/override.conf` (new drop-in)
- `systemctl daemon-reload` and a single `restart` of `transcript-backend.service`

It does **not** touch `/etc/transcript-backend.env`, `/var/lib/transcript-backend/`,
or any other service, container, or config on the host.

### 1.2. VPS secrets and config

Secrets and runtime config are kept in a single env file loaded by systemd:

```ini
# /etc/transcript-backend.env  (mode 0600, owned by root)
APP_HOST=0.0.0.0
APP_PORT=38765
STORAGE_DIR=/var/lib/transcript-backend/storage
MAX_CONCURRENT_JOBS=2
JOB_TIMEOUT_SECONDS=120
FILE_TTL_MINUTES=60
MAX_URL_LENGTH=2048
MAX_TRANSCRIPT_FILE_MB=25
CORS_ORIGIN=*
ANALYSIS_TIMEOUT_SECONDS=1200
ANALYSIS_MAX_OUTPUT_TOKENS=16384
ANALYSIS_TEMPERATURE=1.0
ANALYSIS_TOP_P=0.95
MINIMAX_API_KEY=...               # required for AI analysis
MINIMAX_BASE_URL=https://api.minimax.io/anthropic
MINIMAX_MODEL=MiniMax-M3
YTDLP_COOKIES_FILE=/var/lib/transcript-backend/cookies.txt
```

The binary itself reads this file via systemd's `EnvironmentFile=`. **It is
not part of the deploy flow** — changes here require a manual edit and
`systemctl restart transcript-backend.service`, not a `deploy-backend.sh`
run.

### 1.3. SSH key auth from dev to VPS

The local deploy script uses SSH key auth (no password). One-time:

```bash
# dev machine
ssh-copy-id root@185.245.183.179
# or, if ssh-copy-id is not available:
ssh root@185.245.183.179 'mkdir -p ~/.ssh && chmod 700 ~/.ssh'
cat ~/.ssh/id_ed25519.pub | ssh root@185.245.183.179 \
  'cat >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys'
```

Verify:

```bash
ssh -o BatchMode=yes root@185.245.183.179 'echo ok'
```

### 1.4. Vercel

The frontend project is `shergutts-projects/yt-transcripts` and is already
linked to the GitHub repo `shergutt/youtube_knowledge` on `master`, with
`rootDirectory: frontend`. To verify or re-link:

1. Vercel → Project Settings → Git → "Connected Git Repository":
   `shergutt/youtube_knowledge`, Production Branch: `master`
2. Project Settings → General → Root Directory: `frontend`
3. Project Settings → Environment Variables: `VITE_API_BASE` = `https://yt-api.valorix.lat`
   (Production)

---

## 2. Day-to-day deploy

### 2.1. The happy path

```bash
# 1. local dev
cargo test                                            # backend tests
cd frontend && npm run build && cd ..                 # type-check + build

# 2. commit + push (frontend is auto-deployed by Vercel)
git add -A
git commit -m "feat: …"
git push origin master

# 3. deploy the backend (one command, runs on this machine)
./scripts/deploy-backend.sh
```

The script SSHes into the VPS and runs the VPS-side `deploy.sh`, which:

1. `git fetch --tags --prune` in `/opt/transcript-backend/src`
2. `cargo build --release --bin transcript-backend`
3. Copies the new binary to `target/release/transcript-backend-<git describe>`
4. Updates the `current` symlink atomically
5. `systemctl restart transcript-backend.service`
6. Polls `/health` for up to 10 s; on failure, restores the previous binary
7. Garbage-collects everything except the last 5 release binaries and the one
   currently pointed to by `current`

### 2.2. Deploying a specific ref

```bash
./scripts/deploy-backend.sh v0.1.2
./scripts/deploy-backend.sh feat/some-branch
./scripts/deploy-backend.sh 976a7d9
```

### 2.2.1. Tagging releases

The VPS-side `deploy.sh` uses `git describe --tags --always --dirty` to
label each release binary. After the first `git tag v0.1.0 && git push
--tags`, subsequent deploys produce clean labels like
`v0.1.0-1-g<sha>` and `v0.1.0` itself.

### 2.3. Dry-run and log tail

```bash
./scripts/deploy-backend.sh --dry-run    # prints the ssh command, does nothing
./scripts/deploy-backend.sh --logs       # also tails the journalctl after deploy
```

### 2.4. Environment overrides

The local script honours three env vars:

| Var | Default | Use |
|---|---|---|
| `DEPLOY_VPS` | `root@185.245.183.179` | target a different host |
| `DEPLOY_SSH_KEY` | `$HOME/.ssh/id_ed25519` | use a different key |

---

## 3. Verification

After any deploy, run:

```bash
# 1. backend health (public)
curl -fsS https://yt-api.valorix.lat/health
#   expect: {"status":"ok","analyzer":true,"analyzer_model":"MiniMax-M3"}

# 2. backend binary path (on the VPS, via SSH)
ssh root@185.245.183.179 \
  'readlink -f /proc/$(pgrep -f transcript-backend | head -1)/exe'
#   expect: /opt/transcript-backend/target/release/transcript-backend-<version>

# 3. the live binary's git ref
ssh root@185.245.183.179 'ls -la /opt/transcript-backend/target/release/'
#   expect: transcript-backend.current -> ...transcript-backend-<ref>

# 4. frontend (public)
curl -fsS https://yt-transcripts-six.vercel.app | head -c 200
```

For a real-data smoke test, probe a known-good YouTube video and download
the transcript:

```bash
curl -fsS -X POST https://yt-api.valorix.lat/api/probe \
  -H 'Content-Type: application/json' \
  -d '{"url":"https://www.youtube.com/watch?v=jNQXAC9IVRw"}' | head -c 400
```

---

## 4. Rollback

The deploy script is self-healing: on a failed health gate, it restores
the previous binary and restarts automatically. If a bad release sneaks
past (e.g. health is OK but probes break), roll back manually:

```bash
# On the VPS, point the symlink at the previous versioned binary and restart.
# List the candidates first:
ssh root@185.245.183.179 \
  'ls -1t /opt/transcript-backend/target/release/transcript-backend-* \
   | grep -v "\.current$" | head -5'
# Then point at one of them:
ssh root@185.245.183.179 '
  PREV=/opt/transcript-backend/target/release/transcript-backend-0.1.0+g976a7d9
  ln -sfn "$PREV" /opt/transcript-backend/target/release/transcript-backend.current
  systemctl restart transcript-backend.service
  curl -fsS http://127.0.0.1:38765/health
'
```

**Last-resort back-out** (drops the symlink-rolled system entirely and
goes back to the original pre-deploy binary):

```bash
ssh root@185.245.183.179 '
  rm /etc/systemd/system/transcript-backend.service.d/override.conf
  systemctl daemon-reload
  systemctl restart transcript-backend.service
'
# This uses the OLD ExecStart=/usr/local/bin/transcript-backend.
# The binary at that path is left untouched by deploys, so it always works.
```

---

## 5. Replaying the bootstrap on a new VPS

If you ever need to redo the VPS-side setup from scratch (e.g. moving to
a new host), the bootstrap lives at `scripts/bootstrap_vps.py` in the
dev machine's `/tmp/opencode/` (it was used for the original install and
is gitignored by design). It is idempotent and re-runnable.

**Before running it on a new host**, ensure:

- `apt` is healthy (`apt-get update` succeeds; install `build-essential`
  if not already present)
- The `transcript` user exists, with `/home/transcript` and shell
  `/usr/sbin/nologin`
- The env file at `/etc/transcript-backend.env` is in place (copy from
  the old host, mode 0600, owned by `root`)
- The storage dir `/var/lib/transcript-backend/{cookies.txt,storage/}`
  is in place (or symlinked)
- Port `38765` is open on `127.0.0.1` for the cloudflared tunnel
- The dev machine's public key is in `/root/.ssh/authorized_keys`

Then run from the dev machine:

```bash
python3 /tmp/opencode/bootstrap_vps.py
```

---

## 6. Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `./scripts/deploy-backend.sh` hangs at the SSH step | `known_hosts` mismatch or the VPS host key rotated | Re-run `ssh-keyscan -H 185.245.183.179 >> ~/.ssh/known_hosts` |
| VPS-side `cargo: command not found` | `deploy.sh` ran in a non-interactive SSH that did not source rustup's env | Fixed in `deploy.sh` (`. "$HOME/.cargo/env"` at the top). If you see it again, the file was overwritten; re-upload the canonical copy. |
| Health check fails, deploy script rolls back | The new build crashes at startup | Read the journal: `ssh root@185.245.183.179 'journalctl -u transcript-backend.service -n 100 --no-pager'` |
| Vercel build fails on `tsc -b` | Type error in the frontend | Fix locally (`cd frontend && npm run build`) before pushing |
| `/etc/transcript-backend.env` shows wrong key | You edited the file but the service is still using the old one | `systemctl restart transcript-backend.service` (env is read on startup) |
| `cookies.txt` was rotated | yt-dlp auth broke | Re-export cookies from the browser into `/var/lib/transcript-backend/cookies.txt` (mode 0600, owned by `transcript`); restart the service |

---

## 7. Security notes

- **The local deploy script is gitignored** (`scripts/deploy-*.sh` in
  `.gitignore`). It bakes the VPS hostname and is intentionally not in
  the public repo. If you ever want a different filename, edit the
  pattern in `.gitignore`.
- **The VPS-side `deploy.sh` is also not in git** (it lives only at
  `/opt/transcript-backend/deploy.sh` on the VPS). To regenerate it
  from this repo's design, see the script in the dev machine's
  `/tmp/opencode/`.
- **SSH key auth, not password.** The local script uses your ed25519
  key; the password is never in any file or environment.
- **API keys stay in `/etc/transcript-backend.env`** (mode 0600). The
  deploy script does not touch this file.
- **The old binary at `/usr/local/bin/transcript-backend` is preserved**
  as a one-version-deep backstop. To permanently remove it, do so
  only after at least one successful symlink-rolled deploy.

---

## 8. Reference

### Paths on the VPS

| Path | Purpose |
|---|---|
| `/opt/transcript-backend/src` | Git clone of the repo |
| `/opt/transcript-backend/src/backend` | Cargo crate (the only thing cargo touches) |
| `/opt/transcript-backend/target/release/` | Built binaries (one per release + the `current` symlink) |
| `/opt/transcript-backend/deploy.sh` | VPS-side deploy script (not in git) |
| `/etc/systemd/system/transcript-backend.service` | Unit file (untouched by deploys) |
| `/etc/systemd/system/transcript-backend.service.d/override.conf` | Drop-in pointing ExecStart at the symlink |
| `/etc/transcript-backend.env` | Secrets and runtime config (mode 0600) |
| `/var/lib/transcript-backend/storage/` | Job output directories |
| `/var/lib/transcript-backend/cookies.txt` | yt-dlp cookies (mode 0600, owned by `transcript`) |
| `/usr/local/bin/transcript-backend` | Pre-deploy binary, kept as a backstop |
| `/root/.cloudflared/yt-transcripts.yml` | Cloudflare Tunnel config (frontend ↔ `yt-api.valorix.lat` → `http://127.0.0.1:38765`) |

### Backend env vars

See section 1.2 above. All of these are read by the binary at startup;
`file_ttl_minutes`, `max_playlist_*`, and the cookie path affect runtime
behaviour but the running service does not pick up changes until it is
restarted.

### Endpoints

```text
GET  /health
POST /api/probe
POST /api/transcripts
GET  /api/jobs/{job_id}
GET  /api/downloads/{job_id}
POST /api/analyze
GET  /api/analyses/{id}
GET  /api/analyses/{id}/download
POST /api/playlists/probe
POST /api/playlists
GET  /api/playlists/{id}
GET  /api/playlists/{id}/download
```

### Frontend URL

`https://yt-transcripts-six.vercel.app` (production alias).
PR previews are generated automatically per branch.
