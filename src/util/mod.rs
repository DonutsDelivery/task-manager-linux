mod format;
pub use format::*;

/// Path to run this program again. Inside an AppImage the process starts through
/// the bundled dynamic loader, so `current_exe` points at the loader. The AppImage
/// runtime exports the real file in `APPIMAGE`.
pub fn own_executable() -> std::io::Result<std::path::PathBuf> {
    match std::env::var_os("APPIMAGE") {
        Some(path) if std::path::Path::new(&path).is_file() => Ok(path.into()),
        _ => std::env::current_exe(),
    }
}
