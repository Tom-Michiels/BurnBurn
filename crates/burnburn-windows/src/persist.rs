//! The recent lists on disk (`recent-projects.json`, `recent-templates.json` in the BurnBurn
//! folder), as on the Mac: the controller owns the lists and announces every change as a host
//! effect; the host stores it and gives the stored lists back at the next start.

use std::io;
use std::path::{Path, PathBuf};

use burnburn_presentation::{FilesIntent, HostEffect, UiIntent};

/// `<dir>/recent-projects.json`.
pub fn recent_projects_file(dir: &Path) -> PathBuf {
    dir.join("recent-projects.json")
}

/// `<dir>/recent-templates.json`.
pub fn recent_templates_file(dir: &Path) -> PathBuf {
    dir.join("recent-templates.json")
}

/// The intents that hand the stored lists to the controller (a missing or damaged file is an
/// empty list).
pub fn load_intents(dir: &Path) -> Vec<UiIntent> {
    vec![
        UiIntent::SetRecentProjects { paths: crate::recent::read_recent_projects(&recent_projects_file(dir)) },
        UiIntent::Files(FilesIntent::SetRecentTemplates {
            paths: crate::recent::read_recent_projects(&recent_templates_file(dir)),
        }),
    ]
}

/// Stores the list an effect announces. `None`: the effect is not about a recent list.
pub fn persist_effect(dir: &Path, effect: &HostEffect) -> Option<io::Result<()>> {
    let (file, paths) = match effect {
        HostEffect::RecentProjectsChanged { paths } => (recent_projects_file(dir), paths),
        HostEffect::RecentTemplatesChanged { paths } => (recent_templates_file(dir), paths),
        _ => return None,
    };
    Some(crate::recent::write_recent_projects(&file, paths))
}
