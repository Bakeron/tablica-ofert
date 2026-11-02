//! Collecting offers while the app is closed: a task in Windows Task Scheduler starts the app
//! every hour without a window (`--background`). Each source still keeps its own interval and
//! daily limit; the hourly run also checks reminders. No autostart with Windows.
//! Turning the option off (or uninstalling - see `windows/hooks.nsh`) removes the task.

use serde::Serialize;

use crate::error::{AppError, AppResult};

pub const BACKGROUND_FLAG: &str = "--background";

/// Development builds get their own task, so they never replace the installed app's one.
pub const TASK_NAME: &str = if cfg!(debug_assertions) { r"TablicaOfert\Pobieranie ofert (dev)" } else { r"TablicaOfert\Pobieranie ofert" };

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatus {
    /// The task really exists in Task Scheduler (read from Windows, not from the settings).
    pub exists: bool,
}

#[cfg(windows)]
fn schtasks(args: &[&str]) -> AppResult<std::process::Output> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("schtasks.exe")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(AppError::Io)
}

#[cfg(windows)]
pub fn status() -> AppResult<TaskStatus> {
    Ok(TaskStatus { exists: schtasks(&["/Query", "/TN", TASK_NAME])?.status.success() })
}

/// Creates (or replaces) the hourly task for the current program file.
#[cfg(windows)]
pub fn create() -> AppResult<()> {
    let exe = std::env::current_exe().map_err(AppError::Io)?;
    let exe = exe.to_str().ok_or_else(|| AppError::Invalid("unusual program path".into()))?;
    if exe.contains('"') {
        return Err(AppError::Invalid("unusual program path".into()));
    }
    let command = format!("\"{exe}\" {BACKGROUND_FLAG}");
    let out = schtasks(&["/Create", "/TN", TASK_NAME, "/TR", &command, "/SC", "HOURLY", "/MO", "1", "/F"])?;
    if out.status.success() {
        Ok(())
    } else {
        Err(AppError::Invalid(format!(
            "Task Scheduler refused: {}",
            crate::text::one_line(&String::from_utf8_lossy(&out.stderr), 300)
        )))
    }
}

#[cfg(windows)]
pub fn remove() -> AppResult<()> {
    if status()?.exists {
        schtasks(&["/Delete", "/TN", TASK_NAME, "/F"])?;
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn status() -> AppResult<TaskStatus> {
    Ok(TaskStatus { exists: false })
}

#[cfg(not(windows))]
pub fn create() -> AppResult<()> {
    Err(AppError::Invalid("only on Windows for now".into()))
}

#[cfg(not(windows))]
pub fn remove() -> AppResult<()> {
    Ok(())
}

/// Makes Task Scheduler match the setting.
pub fn sync(enabled: bool) -> AppResult<TaskStatus> {
    if enabled { create()? } else { remove()? }
    status()
}

pub fn is_background(args: &[String]) -> bool {
    args.iter().any(|a| a == BACKGROUND_FLAG)
}
