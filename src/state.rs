use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};

use crate::theme_assets::MERMAID_RUNTIME;

pub struct AppState {
    pub workspace: RwLock<Option<PathBuf>>,
    pub export_limiter: tokio::sync::Semaphore,
    mermaid_runtime: Arc<str>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            workspace: RwLock::new(None),
            export_limiter: tokio::sync::Semaphore::new(1),
            mermaid_runtime: Arc::from(MERMAID_RUNTIME),
        }
    }

    pub fn set_workspace(&self, path: PathBuf) {
        *self.workspace.write().expect("workspace lock poisoned") = Some(path);
    }

    pub fn workspace(&self) -> Option<PathBuf> {
        self.workspace
            .read()
            .expect("workspace lock poisoned")
            .clone()
    }

    pub fn mermaid_runtime(&self) -> std::io::Result<Arc<str>> {
        Ok(self.mermaid_runtime.clone())
    }
}
