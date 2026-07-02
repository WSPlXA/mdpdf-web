use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use tokio::sync::RwLock;

use crate::model::{ConvertJob, UploadedFile};

#[derive(Clone)]
pub struct AppState {
    pub files_dir: PathBuf,
    pub jobs_dir: PathBuf,
    pub themes_dir: PathBuf,
    pub files: Arc<RwLock<HashMap<String, UploadedFile>>>,
    pub jobs: Arc<RwLock<HashMap<String, ConvertJob>>>,
}

impl AppState {
    pub async fn new(workdir: PathBuf, themes_dir: PathBuf) -> std::io::Result<Self> {
        let files_dir = workdir.join("files");
        let jobs_dir = workdir.join("jobs");
        tokio::fs::create_dir_all(&files_dir).await?;
        tokio::fs::create_dir_all(&jobs_dir).await?;
        Ok(Self {
            files_dir,
            jobs_dir,
            themes_dir,
            files: Arc::new(RwLock::new(HashMap::new())),
            jobs: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn theme_dir(&self, name: &str) -> PathBuf {
        self.themes_dir.join(name)
    }

    pub fn job_dir(&self, job_id: &str) -> PathBuf {
        self.jobs_dir.join(job_id)
    }
}

pub fn is_path_inside(base: &Path, candidate: &Path) -> bool {
    match (base.canonicalize(), candidate.canonicalize()) {
        (Ok(base), Ok(candidate)) => candidate.starts_with(base),
        _ => false,
    }
}
