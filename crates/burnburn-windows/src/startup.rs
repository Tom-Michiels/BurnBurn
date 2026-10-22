//! Starting the host with a file (`burnburn-win.exe "<pad>"`, the MSIX file association): the
//! command-line arguments become intents for the shared controller, without the `windows` crate.

use std::ffi::OsString;
use std::path::PathBuf;

use burnburn_presentation::UiIntent;

/// The intents a start with these arguments (the program name already skipped) performs: one
/// `Open` per project file, or a new document when no argument names one. Anything that is not a
/// `.burnburn` file (flags, other files, empty strings) is ignored, like the macOS
/// `lifecycle::open_requests`; import has its own dialog.
pub fn startup_intents(args: impl IntoIterator<Item = OsString>) -> Vec<UiIntent> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for arg in args {
        let text = arg.to_string_lossy();
        // Explorer and the MSIX association quote paths that contain spaces.
        let path = PathBuf::from(text.trim().trim_matches('"'));
        let is_project = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("burnburn"));
        if is_project && !paths.contains(&path) {
            paths.push(path);
        }
    }
    if paths.is_empty() {
        return vec![UiIntent::NewDocument];
    }
    paths.into_iter().map(|path| UiIntent::Open { path }).collect()
}
