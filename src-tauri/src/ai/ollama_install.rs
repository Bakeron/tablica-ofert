//! "Install Ollama for me": downloads the official installer, runs it only when the file
//! carries Ollama's valid signature, and sets where models go. Nothing runs without the
//! user's click, and the installer never restarts the computer.

use std::path::PathBuf;

use crate::error::{AppError, AppResult};
#[cfg(not(windows))]
use crate::model::OnProgress;

/// Windows paths longer than this break in older installers.
const MAX_PATH: usize = 240;

/// A chosen folder: empty = the default place, otherwise a full path without characters
/// that could change the installer's command line.
pub fn folder(value: &str) -> AppResult<Option<PathBuf>> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let bytes = value.as_bytes();
    let drive = bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\';
    let bad = !drive || value.chars().any(|c| c.is_control() || "\"<>|?*".contains(c)) || value[2..].contains(':');
    if bad || value.chars().count() > MAX_PATH {
        return Err(AppError::Invalid("folder".into()));
    }
    Ok(Some(PathBuf::from(value)))
}

/// `Valid` signature of Ollama Inc. - the subject starts with the common name, so a company
/// with these words elsewhere in its name does not pass.
#[cfg_attr(not(windows), allow(dead_code))]
fn is_ollama(answer: &str) -> bool {
    answer.trim().strip_prefix("Valid|").is_some_and(|subject| subject.starts_with("CN=Ollama Inc.,"))
}

#[cfg(windows)]
mod windows {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use tokio::io::AsyncWriteExt;

    use super::{folder, is_ollama};
    use crate::error::{AppError, AppResult};
    use crate::model::{OnProgress, Progress};
    use crate::net;

    const URL: &str = "https://ollama.com/download/OllamaSetup.exe";
    /// The installer has about 1.7 GB (11.2026); anything far bigger is not what was asked for.
    const MAX_SIZE: u64 = 5 * 1024 * 1024 * 1024;

    static RUNNING: AtomicBool = AtomicBool::new(false);

    struct Running;

    impl Drop for Running {
        fn drop(&mut self) {
            RUNNING.store(false, Ordering::SeqCst);
        }
    }

    async fn download(file: &Path, progress: OnProgress<'_>) -> AppResult<()> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .https_only(true)
            .build()
            .map_err(net::net_error)?;
        let resp = client.get(URL).send().await.map_err(net::net_error)?;
        let mut resp = net::check_status(resp).await?;
        let total = resp.content_length().unwrap_or(0);
        if total > MAX_SIZE {
            return Err(AppError::Net("installer too big".into()));
        }
        let mut out = tokio::fs::File::create(file).await?;
        let mut done: u64 = 0;
        let mut shown = 0;
        while let Some(chunk) = resp.chunk().await.map_err(net::net_error)? {
            done += chunk.len() as u64;
            if done > MAX_SIZE {
                return Err(AppError::Net("installer too big".into()));
            }
            out.write_all(&chunk).await?;
            let percent = (done.min(total) * 100).checked_div(total).unwrap_or(0) as usize;
            if percent != shown {
                shown = percent;
                progress(Some(Progress::new("ollama-download", None, percent, 100)));
            }
        }
        out.flush().await?;
        Ok(())
    }

    fn powershell(script: &str, name: &str, value: &Path) -> AppResult<String> {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // The path goes in the environment, never into the script text.
        let out = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
            .env(name, value)
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;
        if !out.status.success() {
            return Err(AppError::Invalid("powershell".into()));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn signed_by_ollama(file: &Path) -> AppResult<bool> {
        const SCRIPT: &str = "$s = Get-AuthenticodeSignature -LiteralPath $env:TABLICA_FILE; \
            Write-Output (\"$($s.Status)|$($s.SignerCertificate.Subject)\")";
        Ok(is_ollama(&powershell(SCRIPT, "TABLICA_FILE", file)?))
    }

    /// Saved for the user's account, so Ollama finds it after every start.
    fn remember_models(dir: &Path) -> AppResult<()> {
        const SCRIPT: &str = "[Environment]::SetEnvironmentVariable('OLLAMA_MODELS', $env:TABLICA_DIR, 'User')";
        powershell(SCRIPT, "TABLICA_DIR", dir).map(|_| ())
    }

    fn run_installer(file: &Path, program: Option<&Path>, models: Option<&Path>) -> AppResult<()> {
        // Own progress window, no questions, and never a restart of the computer.
        let mut setup = std::process::Command::new(file);
        setup.args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART"]);
        if let Some(dir) = program {
            setup.arg(format!("/DIR={}", dir.display()));
        }
        // Ollama started by the installer gets the folder at once.
        if let Some(dir) = models {
            setup.env("OLLAMA_MODELS", dir);
        }
        let status = setup.status()?;
        if status.success() {
            Ok(())
        } else {
            Err(AppError::Invalid(format!("installer ended with code {}", status.code().unwrap_or(-1))))
        }
    }

    async fn fetch_and_run(file: &Path, program: Option<PathBuf>, models: Option<PathBuf>, progress: OnProgress<'_>) -> AppResult<()> {
        download(file, progress).await?;
        progress(Some(Progress::new("ollama-run", None, 0, 0)));
        let path = file.to_path_buf();
        tokio::task::spawn_blocking(move || -> AppResult<()> {
            if !signed_by_ollama(&path)? {
                return Err(AppError::Invalid("the installer is not signed by Ollama".into()));
            }
            if let Some(dir) = &models {
                std::fs::create_dir_all(dir)?;
                remember_models(dir)?;
            }
            run_installer(&path, program.as_deref(), models.as_deref())
        })
        .await
        .map_err(|e| AppError::Invalid(e.to_string()))?
    }

    /// Downloads, checks and runs the installer; `program` and `models` empty = default places.
    pub async fn install(program: &str, models: &str, progress: OnProgress<'_>) -> AppResult<()> {
        let program = folder(program)?;
        let models = folder(models)?;
        if RUNNING.swap(true, Ordering::SeqCst) {
            return Err(AppError::Busy);
        }
        let _guard = Running;
        let dir = std::env::temp_dir().join(format!("tablica-ofert-ollama-{}", std::process::id()));
        tokio::fs::create_dir_all(&dir).await?;
        let result = fetch_and_run(&dir.join("OllamaSetup.exe"), program, models, progress).await;
        let _ = tokio::fs::remove_dir_all(&dir).await;
        result
    }
}

#[cfg(windows)]
pub use windows::install;

#[cfg(not(windows))]
pub async fn install(_program: &str, _models: &str, _progress: OnProgress<'_>) -> AppResult<()> {
    Err(AppError::Invalid("only on Windows for now".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_takes_only_a_plain_full_path() {
        assert_eq!(folder("  ").unwrap(), None);
        assert_eq!(folder(r"D:\AI\modele").unwrap(), Some(PathBuf::from(r"D:\AI\modele")));
        assert_eq!(folder(r"D:\Moje Programy\Ollama").unwrap(), Some(PathBuf::from(r"D:\Moje Programy\Ollama")));
        for bad in ["modele", r"\\serwer\udzial", "D:modele", "D:\\a\" /VERYSILENT \"", "D:\\a\nb", r"D:\a:b", r"D:\a*", "ą"] {
            assert!(folder(bad).is_err(), "{bad}");
        }
        assert!(folder(&format!(r"D:\{}", "a".repeat(300))).is_err());
    }

    #[test]
    fn only_a_valid_signature_of_ollama_passes() {
        assert!(is_ollama("Valid|CN=Ollama Inc., O=Ollama Inc., L=Toronto, S=Ontario, C=CA\r\n"));
        assert!(!is_ollama("NotSigned|"));
        assert!(!is_ollama("HashMismatch|CN=Ollama Inc., O=Ollama Inc."));
        assert!(!is_ollama("Valid|CN=Ollama Inc. Fake, O=Other"));
        assert!(!is_ollama("Valid|CN=Other, O=CN=Ollama Inc., L=X"));
    }
}
