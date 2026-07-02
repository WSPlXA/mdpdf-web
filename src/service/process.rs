use std::{process::Output, time::Duration};

use tokio::{process::Command, time::timeout};

use crate::{
    error::{AppError, Result},
};

pub async fn run_with_timeout(mut command: Command, seconds: u64) -> Result<Output> {
    command.kill_on_drop(true);
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());

    let child = command.spawn().map_err(|err| {
        AppError::Conversion(format!("failed to spawn process: {err}"))
    })?;

    match timeout(Duration::from_secs(seconds), child.wait_with_output()).await {
        Ok(result) => result.map_err(AppError::from),
        Err(_) => Err(AppError::Conversion(format!(
            "process timed out after {seconds}s"
        ))),
    }
}
