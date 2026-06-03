use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{RwLock, Semaphore};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::errors::AppResult;
use crate::models::analysis::AnalysisJob;
use crate::models::job::TranscriptJob;
use crate::models::playlist::PlaylistJob;
use crate::services::analyzer::AnalyzerService;
use crate::services::ytdlp::YtDlpService;

#[derive(Clone)]
pub struct AppState {
    pub jobs: Arc<RwLock<HashMap<Uuid, TranscriptJob>>>,
    pub analyses: Arc<RwLock<HashMap<Uuid, AnalysisJob>>>,
    pub playlists: Arc<RwLock<HashMap<Uuid, PlaylistJob>>>,
    pub semaphore: Arc<Semaphore>,
    pub analysis_semaphore: Arc<Semaphore>,
    pub playlist_semaphore: Arc<Semaphore>,
    pub config: AppConfig,
    pub ytdlp: YtDlpService,
    pub analyzer: Option<AnalyzerService>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_jobs.max(1)));
        let analysis_semaphore = Arc::new(Semaphore::new(2));
        let playlist_semaphore = Arc::new(Semaphore::new(2));
        let ytdlp = YtDlpService::new(config.clone());
        let analyzer = AnalyzerService::new(config.clone()).ok();
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            analyses: Arc::new(RwLock::new(HashMap::new())),
            playlists: Arc::new(RwLock::new(HashMap::new())),
            semaphore,
            analysis_semaphore,
            playlist_semaphore,
            config,
            ytdlp,
            analyzer,
        }
    }

    pub fn require_analyzer(&self) -> AppResult<&AnalyzerService> {
        self.analyzer
            .as_ref()
            .ok_or(crate::errors::AppError::AnalysisNotConfigured)
    }
}
