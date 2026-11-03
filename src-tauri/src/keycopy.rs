//! Second copy of the database key, next to the database, protected by Windows DPAPI
//! (bound to the Windows user account - local or Microsoft; Credential Manager itself uses
//! the same protection). If the key ever disappears from Credential Manager, the app opens
//! the database with this copy and puts the key back, instead of locking the user out.
//! Only Windows - on Android the key lives in Android Keystore.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

pub const FILE: &str = "tablica.key";

pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE)
}

/// The key from the copy; `None` when there is no copy (or not on Windows).
pub fn read(data_dir: &Path) -> AppResult<Option<String>> {
    let file = path(data_dir);
    if !cfg!(windows) || !file.exists() {
        return Ok(None);
    }
    let plain = unprotect(&fs::read(&file)?)?;
    match String::from_utf8(plain) {
        Ok(key) if is_key(&key) => Ok(Some(key)),
        _ => Err(AppError::Secret("the key copy is damaged".into())),
    }
}

/// Saves the copy (temporary file + rename, so a crash never leaves half a file).
pub fn write(data_dir: &Path, key: &str) -> AppResult<()> {
    if !cfg!(windows) {
        return Ok(());
    }
    let blob = protect(key.as_bytes())?;
    let file = path(data_dir);
    let tmp = file.with_extension("key.tmp");
    fs::write(&tmp, blob)?;
    fs::rename(&tmp, &file)?;
    Ok(())
}

/// Keys are 32 random bytes as hex (`secrets::random_hex(32)`).
fn is_key(key: &str) -> bool {
    key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Ties the protected blob to this app: DPAPI asks for the same bytes to unprotect it.
#[cfg(windows)]
const ENTROPY: &[u8] = b"pl.prywatne.tablicaofert/database-key/v1";

#[cfg(windows)]
fn protect(data: &[u8]) -> AppResult<Vec<u8>> {
    dpapi(data, true)
}

#[cfg(windows)]
fn unprotect(data: &[u8]) -> AppResult<Vec<u8>> {
    dpapi(data, false)
}

#[cfg(windows)]
fn dpapi(data: &[u8], encrypt: bool) -> AppResult<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
    let entropy = CRYPT_INTEGER_BLOB { cbData: ENTROPY.len() as u32, pbData: ENTROPY.as_ptr() as *mut u8 };
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    // SAFETY: both input blobs point to live slices for the whole call (DPAPI only reads them),
    // `out` receives a buffer allocated by Windows, copied below and freed with LocalFree.
    let ok = unsafe {
        if encrypt {
            CryptProtectData(&input, std::ptr::null(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        } else {
            CryptUnprotectData(&input, std::ptr::null_mut(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        }
    };
    if ok == 0 || out.pbData.is_null() {
        let what = if encrypt { "protect" } else { "read" };
        return Err(AppError::Secret(format!("Windows could not {what} the key copy")));
    }
    // SAFETY: on success Windows filled `out` with `cbData` bytes at `pbData`.
    let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    // SAFETY: the buffer was allocated by DPAPI with LocalAlloc and is not used after this.
    unsafe { LocalFree(out.pbData as _) };
    Ok(bytes)
}

#[cfg(not(windows))]
fn protect(_data: &[u8]) -> AppResult<Vec<u8>> {
    Err(AppError::Secret("the key copy is only made on Windows".into()))
}

#[cfg(not(windows))]
fn unprotect(_data: &[u8]) -> AppResult<Vec<u8>> {
    Err(AppError::Secret("the key copy is only made on Windows".into()))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::db::tests::{temp_dir, TEST_KEY};

    #[test]
    fn copy_round_trips_and_is_not_plain_text() {
        let dir = temp_dir("keycopy");
        assert_eq!(read(&dir).unwrap(), None);
        write(&dir, TEST_KEY).unwrap();
        let raw = fs::read(path(&dir)).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains(TEST_KEY));
        assert_eq!(read(&dir).unwrap().as_deref(), Some(TEST_KEY));
    }

    #[test]
    fn damaged_copy_is_an_error_not_a_key() {
        let dir = temp_dir("keycopy-bad");
        fs::write(path(&dir), b"not a dpapi blob").unwrap();
        assert!(read(&dir).is_err());
    }
}
