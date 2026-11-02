//! "Can my computer run AI locally?" - reads the graphics card, its memory and RAM from
//! Windows (plain code, no AI) and suggests whether Ollama makes sense and how big a model.

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    pub name: String,
    /// Dedicated memory in MB (0 = unknown or shared with RAM).
    pub vram_mb: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Machine {
    pub gpus: Vec<Gpu>,
    pub ram_mb: u64,
    /// `good` - runs well, `ok` - a small model runs, `slow` - only on the processor, `no` - not worth it.
    pub verdict: &'static str,
    /// Suggested Ollama model (orientacyjnie, stan na 11.2026).
    pub model: Option<&'static str>,
}

/// Verdict from the numbers - kept separate so it can be tested without Windows.
pub fn judge(gpus: Vec<Gpu>, ram_mb: u64) -> Machine {
    let vram = gpus.iter().map(|g| g.vram_mb).max().unwrap_or(0);
    let (verdict, model) = if vram >= 12_000 {
        ("good", Some("qwen3:14b"))
    } else if vram >= 7_500 {
        ("good", Some("llama3.1:8b"))
    } else if vram >= 3_800 {
        ("ok", Some("llama3.2:3b"))
    } else if ram_mb >= 15_000 {
        ("slow", Some("llama3.2:3b"))
    } else {
        ("no", None)
    };
    Machine { gpus, ram_mb, verdict, model }
}

#[cfg(windows)]
pub fn check() -> AppResult<Machine> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    // Win32_VideoController reports at most 4 GB; the driver's registry entry has the real size.
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
$ram = (Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory
$gpus = @()
$keys = Get-ChildItem 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}' |
  Where-Object { $_.PSChildName -match '^\d{4}$' }
foreach ($k in $keys) {
  $p = Get-ItemProperty $k.PSPath
  if ($p.DriverDesc) {
    $mem = [uint64]0
    if ($p.'HardwareInformation.qwMemorySize') { $mem = [uint64]$p.'HardwareInformation.qwMemorySize' }
    elseif ($p.'HardwareInformation.MemorySize') { $mem = [uint64]($p.'HardwareInformation.MemorySize' | Select-Object -First 1) }
    $gpus += [pscustomobject]@{ name = [string]$p.DriverDesc; vramMb = [uint64]($mem / 1MB) }
  }
}
[pscustomobject]@{ ramMb = [uint64]($ram / 1MB); gpus = @($gpus) } | ConvertTo-Json -Compress -Depth 3
"#;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", SCRIPT])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(AppError::Io)?;
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Raw {
        ram_mb: u64,
        #[serde(default)]
        gpus: Vec<Gpu>,
    }
    let raw: Raw = serde_json::from_slice(&out.stdout).map_err(|e| AppError::Invalid(format!("cannot read the hardware: {e}")))?;
    let gpus = raw
        .gpus
        .into_iter()
        .map(|g| Gpu { name: crate::text::one_line(&g.name, 120), vram_mb: g.vram_mb })
        .collect();
    Ok(judge(gpus, raw.ram_mb))
}

#[cfg(not(windows))]
pub fn check() -> AppResult<Machine> {
    Err(AppError::Invalid("only on Windows for now".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(vram_mb: u64) -> Vec<Gpu> {
        vec![Gpu { name: "Karta".into(), vram_mb }]
    }

    #[test]
    fn verdicts() {
        assert_eq!(judge(gpu(16_384), 32_000).verdict, "good");
        assert_eq!(judge(gpu(8_192), 16_000).model, Some("llama3.1:8b"));
        assert_eq!(judge(gpu(4_096), 8_000).verdict, "ok");
        assert_eq!(judge(gpu(0), 16_000).verdict, "slow");
        assert_eq!(judge(Vec::new(), 8_000).verdict, "no");
    }
}
