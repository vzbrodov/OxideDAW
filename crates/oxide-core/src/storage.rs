//! Atomic file publication for project documents and rendered audio.
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// Write and sync a sibling temporary file before publishing it. With
/// `replace` disabled, publication fails if the destination already exists.
pub fn write_file(
    path: &Path,
    replace: bool,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let parent = parent_directory(path);
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Expected a file path"))?;
    let mut temporary_name = std::ffi::OsString::from(".");
    temporary_name.push(name);
    temporary_name.push(format!(
        ".{}.{}.tmp",
        std::process::id(),
        TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let temporary_path = parent.join(temporary_name);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_path)?;
    let temporary = TemporaryFile(temporary_path);
    // Close the file before cleanup on errors, including on Windows.
    (|mut file: File| {
        write(&mut file)?;
        file.sync_all()
    })(file)?;
    if replace {
        fs::rename(&temporary.0, path)?;
    } else {
        fs::hard_link(&temporary.0, path)?;
    }
    drop(temporary);
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

pub(crate) fn parent_directory(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

struct TemporaryFile(PathBuf);

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
