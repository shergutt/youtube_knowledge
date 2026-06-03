mod app_state;
mod config;
mod errors;
mod models;
mod routes;
mod services;
mod utils;

use std::net::SocketAddr;

use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::app_state::AppState;
use crate::config::AppConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let cfg = AppConfig::from_env().map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    let cookies_state = match &cfg.yt_dlp_cookies_file {
        Some(p) if p.exists() => format!("loaded ({})", p.display()),
        Some(p) => format!("configured but file not found: {}", p.display()),
        None => "disabled".to_string(),
    };
    info!(
        host = %cfg.host,
        port = cfg.port,
        storage = %cfg.storage_dir.display(),
        max_concurrent_jobs = cfg.max_concurrent_jobs,
        job_timeout_seconds = cfg.job_timeout_seconds,
        file_ttl_minutes = cfg.file_ttl_minutes,
        yt_dlp_cookies = %cookies_state,
        "starting transcript-backend"
    );

    // Ensure storage directories exist
    std::fs::create_dir_all(cfg.jobs_dir())
        .map_err(|e| Box::<dyn std::error::Error>::from(format!("create jobs dir: {}", e)))?;
    std::fs::create_dir_all(cfg.cache_dir())
        .map_err(|e| Box::<dyn std::error::Error>::from(format!("create cache dir: {}", e)))?;

    let state = AppState::new(cfg.clone());

    // Verify yt-dlp is available at startup
    if let Err(e) = state.ytdlp.check_installed().await {
        warn!(error = %e, "yt-dlp check failed at startup; will retry on first job");
    } else {
        info!("yt-dlp is available");
    }

    // Spawn cleanup task
    services::cleanup::spawn_cleanup_task(state.clone());

    // Build router
    let max_body = cfg.max_url_length + 16 * 1024;
    let app = routes::build_router(state, max_body);

    let addr: SocketAddr = format!("{}:{}", cfg.host, cfg.port).parse()?;
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info"));
    fmt().with_env_filter(filter).with_target(false).init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    info!("shutdown signal received");
}
