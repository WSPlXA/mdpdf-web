mod error;
mod model;
mod routes;
mod service;
mod state;

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{
    routing::{get, post},
    Router,
};
use tower_http::{limit::RequestBodyLimitLayer, services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    routes::{convert, download, files, health, job, preview},
    state::AppState,
};

const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            "mdpdf_web=info,tower_http=info,axum=info".into()
        }))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let bind = std::env::var("MDPDF_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let workdir = PathBuf::from(std::env::var("MDPDF_WORKDIR").unwrap_or_else(|_| "workdir".into()));
    let themes_dir =
        PathBuf::from(std::env::var("MDPDF_THEMES").unwrap_or_else(|_| "themes".into()));

    let state = Arc::new(AppState::new(workdir, themes_dir).await?);

    let api = Router::new()
        .route("/healthz", get(health))
        .route("/files", post(files))
        .route("/preview", post(preview))
        .route("/convert", post(convert))
        .route("/jobs/{job_id}", get(job))
        .route("/jobs/{job_id}/download", get(download))
        .layer(RequestBodyLimitLayer::new(MAX_UPLOAD_BYTES));

    let app = Router::new()
        .nest("/api", api)
        .fallback_service(ServeDir::new("public").append_index_html_on_directories(true))
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    let addr: SocketAddr = bind.parse()?;
    tracing::info!(%addr, "mdpdf-web listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
