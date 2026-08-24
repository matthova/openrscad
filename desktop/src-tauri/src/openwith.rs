//! "Open in application" support: enumerate the apps the OS knows can open a
//! given file type, and launch a file in a chosen one. This backs the Export
//! popover's "Open in" section, which lets a user send the current model
//! straight into a slicer, CAD tool, or viewer instead of the Save dialog.
//!
//! Only macOS enumerates apps (via Launch Services / `NSWorkspace`); other
//! platforms return an empty list, so the frontend simply hides the section.

use serde::Serialize;

/// One application that can open a file type, for the "Open in" menu.
#[derive(Serialize, Clone)]
pub struct AppEntry {
    /// Display name shown in the menu (e.g. "Bambu Studio").
    pub name: String,
    /// Absolute path to the `.app` bundle, used to launch it.
    pub path: String,
}

/// Apps that can open files with the given extension (no leading dot), ranked
/// by the OS's preference (default handler first). Empty when nothing is found
/// or on platforms without enumeration support.
#[cfg(target_os = "macos")]
pub fn apps_for_extension(ext: &str) -> Vec<AppEntry> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::NSString;
    use objc2_uniform_type_identifiers::UTType;

    let ext = ext.trim().trim_start_matches('.');
    if ext.is_empty() {
        return Vec::new();
    }

    let ext_ns = NSString::from_str(ext);
    let Some(ut) = UTType::typeWithFilenameExtension(&ext_ns) else {
        return Vec::new();
    };
    let workspace = NSWorkspace::sharedWorkspace();
    let urls = workspace.URLsForApplicationsToOpenContentType(&ut);

    let mut out: Vec<AppEntry> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for url in urls.iter() {
        let Some(path) = url.path() else { continue };
        let path = path.to_string();
        if !seen.insert(path.clone()) {
            continue; // Launch Services can list the same bundle twice
        }
        out.push(AppEntry {
            name: display_name(&path),
            path,
        });
    }
    out
}

#[cfg(not(target_os = "macos"))]
pub fn apps_for_extension(_ext: &str) -> Vec<AppEntry> {
    Vec::new()
}

/// The user-facing app name from a bundle path: the last path component with a
/// trailing `.app` removed (e.g. `/Applications/Bambu Studio.app` → "Bambu Studio").
#[cfg(target_os = "macos")]
fn display_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Launch `path` in the application at `app` (an `.app` bundle path). macOS uses
/// `open -a`, which brings the app forward and hands it the file.
#[cfg(target_os = "macos")]
pub fn open_path_with(path: &str, app: &str) -> Result<(), String> {
    let status = std::process::Command::new("/usr/bin/open")
        .arg("-a")
        .arg(app)
        .arg(path)
        .status()
        .map_err(|e| format!("failed to launch app: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("app exited with status {status}"))
    }
}

#[cfg(not(target_os = "macos"))]
pub fn open_path_with(_path: &str, _app: &str) -> Result<(), String> {
    Err("opening in a chosen application is only supported on macOS".to_string())
}
