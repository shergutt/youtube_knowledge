use std::path::Path;
use std::time::Duration;

use chrono::Utc;
use tokio::time::interval;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::models::job::JobStatus;
use crate::models::playlist::PlaylistStatus;
use crate::AppState;

pub fn spawn_cleanup_task(state: AppState) {
    let storage_dir = state.config.storage_dir.clone();
    let ttl_minutes = state.config.file_ttl_minutes;
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(60));
        loop {
            ticker.tick().await;
            if let Err(e) = run_once(&state, &storage_dir, ttl_minutes).await {
                warn!(error = %e, "cleanup pass failed");
            }
        }
    });
}

async fn run_once(state: &AppState, storage_dir: &Path, ttl_minutes: i64) -> Result<(), String> {
    let jobs_dir = storage_dir.join("jobs");
    if !jobs_dir.exists() {
        return Ok(());
    }
    let now = Utc::now();
    let mut to_delete: Vec<(Uuid, std::path::PathBuf)> = Vec::new();
    {
        let guard = state.jobs.read().await;
        for (id, job) in guard.iter() {
            let expired = job
                .expires_at
                .map(|ts| now > ts)
                .unwrap_or_else(|| now > job.created_at + chrono::Duration::minutes(ttl_minutes));
            if expired && job.status == JobStatus::Completed {
                to_delete.push((*id, std::path::PathBuf::from(&job.job_dir)));
            }
        }
    }
    if to_delete.is_empty() {
        debug!("cleanup: no expired jobs");
    } else {
        info!(count = to_delete.len(), "cleanup: deleting expired jobs");
        for (id, dir) in to_delete {
            if dir.exists() {
                if let Err(e) = std::fs::remove_dir_all(&dir) {
                    warn!(error = %e, path = %dir.display(), "failed to remove expired job dir");
                    continue;
                }
            }
            let mut guard = state.jobs.write().await;
            if let Some(job) = guard.get_mut(&id) {
                job.status = JobStatus::Expired;
                job.output_filename = None;
                job.updated_at = Utc::now();
            }
        }
    }

    // Also expire completed playlist jobs. The playlist's job_dir lives under
    // <storage>/playlists/<uuid>, while each child transcript and analysis is
    // stored under <storage>/jobs/<uuid> (and is expired by the pass above).
    let mut to_delete_playlists: Vec<(Uuid, std::path::PathBuf)> = Vec::new();
    {
        let guard = state.playlists.read().await;
        for (id, p) in guard.iter() {
            let expired = p
                .expires_at
                .map(|ts| now > ts)
                .unwrap_or_else(|| now > p.created_at + chrono::Duration::minutes(ttl_minutes));
            if expired && p.status == PlaylistStatus::Completed {
                to_delete_playlists.push((*id, std::path::PathBuf::from(&p.job_dir)));
            }
        }
    }
    if !to_delete_playlists.is_empty() {
        info!(
            count = to_delete_playlists.len(),
            "cleanup: deleting expired playlists"
        );
        for (id, dir) in to_delete_playlists {
            if dir.exists() {
                if let Err(e) = std::fs::remove_dir_all(&dir) {
                    warn!(error = %e, path = %dir.display(), "failed to remove expired playlist dir");
                    continue;
                }
            }
            let mut guard = state.playlists.write().await;
            if let Some(p) = guard.get_mut(&id) {
                p.status = PlaylistStatus::Expired;
                p.updated_at = Utc::now();
            }
        }
    }
    Ok(())
}
