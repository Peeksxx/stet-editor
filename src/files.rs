use anyhow::{Context, Result, bail};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_LINE_BYTES: usize = 128 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Revision(pub blake3::Hash);

pub fn read(path: &Path) -> Result<Vec<u8>> {
    let file = File::open(path).context("Cannot open file")?;
    if !file.metadata()?.is_file() {
        bail!("Only ordinary files can be edited");
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        bail!("File exceeds the 64 MiB editing limit");
    }
    Ok(bytes)
}

pub fn revision(path: &Path) -> Result<Option<Revision>> {
    match read(path) {
        Ok(bytes) => Ok(Some(Revision(blake3::hash(&bytes)))),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

pub fn identity(path: &Path) -> Result<PathBuf> {
    #[cfg(windows)]
    validate_windows_path(path)?;
    if path.exists() {
        return fs::canonicalize(path).context("Cannot resolve file path");
    }
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let name = absolute.file_name().context("A filename is required")?;
    let parent = fs::canonicalize(
        absolute
            .parent()
            .context("A parent directory is required")?,
    )
    .context("Parent directory is unavailable")?;
    Ok(parent.join(name))
}

#[cfg(windows)]
fn validate_windows_path(path: &Path) -> Result<()> {
    let value = path
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    if value.starts_with("\\\\.\\")
        || value.starts_with("\\\\?\\globalroot")
        || value.starts_with("\\\\?\\pipe\\")
    {
        bail!("Device paths cannot be edited");
    }
    for component in path.components() {
        if let std::path::Component::Normal(name) = component {
            let name = name.to_string_lossy();
            let base = name
                .split('.')
                .next()
                .unwrap_or("")
                .trim_end_matches(' ')
                .to_ascii_uppercase();
            if name.contains(':')
                || matches!(
                    base.as_str(),
                    "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
                )
                || (base.len() == 4
                    && (base.starts_with("COM") || base.starts_with("LPT"))
                    && base.as_bytes()[3].is_ascii_digit())
            {
                bail!("Device names and alternate stream paths cannot be edited");
            }
        }
    }
    Ok(())
}

pub fn path_key(path: &Path) -> String {
    let value = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    }
}

pub fn parse_path(value: &str) -> Result<PathBuf> {
    let mut value = value.trim();
    if let Some(rest) = value.strip_prefix("& ") {
        value = rest.trim();
    }
    let path = if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
        value[1..value.len() - 1].replace("''", "'")
    } else if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        value[1..value.len() - 1]
            .replace("`\"", "\"")
            .replace("``", "`")
    } else {
        value.to_owned()
    };
    if path.is_empty() || path.chars().any(char::is_control) {
        bail!("Enter one valid file path, without line breaks");
    }
    Ok(PathBuf::from(path))
}

pub fn atomic_write(path: &Path, bytes: &[u8], expected: Option<&Revision>) -> Result<Revision> {
    if revision(path)?.as_ref() != expected {
        bail!("File changed on disk. Save As to a new path, or reload before saving");
    }
    if path.exists() && fs::metadata(path)?.permissions().readonly() {
        bail!("File is read-only");
    }
    let parent = path.parent().context("No parent directory")?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".stet-")
        .tempfile_in(parent)?;
    temporary
        .write_all(bytes)
        .context("Cannot write temporary file")?;
    temporary
        .as_file()
        .sync_all()
        .context("Cannot flush temporary file")?;
    if revision(path)?.as_ref() != expected {
        bail!("File changed during save. Original file was not replaced");
    }
    if expected.is_some() {
        let temporary_path = temporary.into_temp_path();
        replace(&temporary_path, path)?;
    } else {
        temporary
            .persist_noclobber(path)
            .map_err(|e| e.error)
            .context("Cannot create destination without overwriting it")?;
    }
    Ok(Revision(blake3::hash(bytes)))
}

#[cfg(windows)]
fn replace(source: &Path, destination: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{REPLACEFILE_IGNORE_ACL_ERRORS, ReplaceFileW};
    let backup_directory = tempfile::Builder::new()
        .prefix(".stet-save-")
        .tempdir_in(destination.parent().context("No save directory")?)?;
    let backup_path = backup_directory.path().join("original");
    let backup: Vec<u16> = backup_path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let destination_path = destination;
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let flags = if security_descriptor(&source)? == security_descriptor(&destination)? {
        REPLACEFILE_IGNORE_ACL_ERRORS
    } else {
        0
    };
    let result = unsafe {
        ReplaceFileW(
            destination.as_ptr(),
            source.as_ptr(),
            backup.as_ptr(),
            flags,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if result == 0 {
        let error = std::io::Error::last_os_error();
        let recovery = recover_original(backup_directory, destination_path)?;
        return Err(error).context(recovery);
    }
    let backup_name = backup_directory.path().to_owned();
    backup_directory.close().with_context(|| {
        format!(
            "File saved, but old-revision cleanup failed in {}",
            backup_name.display()
        )
    })?;
    Ok(())
}

#[cfg(windows)]
fn recover_original(backup_directory: tempfile::TempDir, destination: &Path) -> Result<String> {
    let recovery = backup_directory.keep();
    let original = recovery.join("original");
    if !original.try_exists().with_context(|| {
        format!(
            "Save failed; inspect recovery directory {}",
            recovery.display()
        )
    })? {
        std::fs::remove_dir(&recovery)?;
        return Ok("Safe replacement failed; original file was not replaced".into());
    }
    if !destination
        .try_exists()
        .with_context(|| format!("Save failed; original preserved at {}", original.display()))?
    {
        let source = recovery.join("original");
        match std::fs::hard_link(&source, destination) {
            Ok(()) => {
                std::fs::remove_file(&source).with_context(|| {
                    format!(
                        "Original restored; recovery copy remains at {}",
                        source.display()
                    )
                })?;
                std::fs::remove_dir(&recovery).with_context(|| {
                    format!(
                        "Original restored; recovery directory remains at {}",
                        recovery.display()
                    )
                })?;
                return Ok("Safe replacement failed; original file restored".into());
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "Save failed; original file is preserved at {}",
                        source.display()
                    )
                });
            }
        }
    }
    Ok(format!(
        "Save failed; original revision preserved at {}",
        recovery.join("original").display()
    ))
}

#[cfg(windows)]
fn security_descriptor(path: &[u16]) -> Result<Vec<u8>> {
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, GetFileSecurityW,
        OWNER_SECURITY_INFORMATION,
    };
    let information =
        OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    let mut length = 0;
    unsafe {
        GetFileSecurityW(
            path.as_ptr(),
            information,
            std::ptr::null_mut(),
            0,
            &mut length,
        );
    }
    if length == 0 || length > 1024 * 1024 {
        return Err(std::io::Error::last_os_error())
            .context("Cannot inspect file permissions before replacement");
    }
    let mut descriptor = vec![0u8; length as usize];
    let result = unsafe {
        GetFileSecurityW(
            path.as_ptr(),
            information,
            descriptor.as_mut_ptr().cast(),
            length,
            &mut length,
        )
    };
    if result == 0 {
        return Err(std::io::Error::last_os_error())
            .context("Cannot verify file permissions before replacement");
    }
    descriptor.truncate(length as usize);
    Ok(descriptor)
}

#[cfg(not(windows))]
fn replace(source: &Path, destination: &Path) -> Result<()> {
    fs::set_permissions(source, fs::metadata(destination)?.permissions())?;
    fs::rename(source, destination).context("Safe replacement failed")
}
