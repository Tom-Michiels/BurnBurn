//! Archief ▸ Recente projecten (W2-L1-bewerken): the list lives here, the host stores it.

use std::path::{Path, PathBuf};

use super::{AppController, HostEffect};

/// How many projects the menu remembers.
pub(super) const MAX_RECENT_PROJECTS: usize = 10;

impl AppController {
    /// Recent projects, most recent first (the host shows them under Archief).
    pub fn recent_projects(&self) -> Vec<PathBuf> {
        self.recent.clone()
    }

    /// `path` was opened or saved: it is now the most recent. The effect tells the host to persist.
    pub(super) fn note_recent(&mut self, path: &Path) -> Vec<HostEffect> {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(MAX_RECENT_PROJECTS);
        vec![HostEffect::RecentProjectsChanged { paths: self.recent.clone() }]
    }

    /// `path` cannot be opened any more (moved or deleted): the menu stops offering it.
    pub(super) fn forget_recent(&mut self, path: &Path) -> Vec<HostEffect> {
        if !self.recent.iter().any(|p| p == path) {
            return Vec::new();
        }
        self.recent.retain(|p| p != path);
        vec![HostEffect::RecentProjectsChanged { paths: self.recent.clone() }]
    }

    /// The list the host stored: order kept, duplicates dropped, at most [`MAX_RECENT_PROJECTS`].
    pub(super) fn restore_recent(&mut self, paths: Vec<PathBuf>) {
        let mut list: Vec<PathBuf> = Vec::new();
        for p in paths {
            if !list.contains(&p) {
                list.push(p);
            }
        }
        list.truncate(MAX_RECENT_PROJECTS);
        self.recent = list;
    }

    pub(super) fn clear_recent(&mut self) -> Vec<HostEffect> {
        self.recent.clear();
        vec![HostEffect::RecentProjectsChanged { paths: Vec::new() }]
    }
}

impl AppController {
    /// Recent templates, most recent first (the host shows them under Archief ▸ Nieuw van template).
    pub fn recent_templates(&self) -> Vec<PathBuf> {
        self.recent_templates.clone()
    }

    /// `path` was used or written: it is now the most recent template.
    pub(super) fn note_recent_template(&mut self, path: &Path) -> Vec<HostEffect> {
        self.recent_templates.retain(|p| p != path);
        self.recent_templates.insert(0, path.to_path_buf());
        self.recent_templates.truncate(MAX_RECENT_PROJECTS);
        vec![HostEffect::RecentTemplatesChanged { paths: self.recent_templates.clone() }]
    }

    /// `path` cannot be read any more: the menu stops offering it.
    pub(super) fn forget_recent_template(&mut self, path: &Path) -> Vec<HostEffect> {
        if !self.recent_templates.iter().any(|p| p == path) {
            return Vec::new();
        }
        self.recent_templates.retain(|p| p != path);
        vec![HostEffect::RecentTemplatesChanged { paths: self.recent_templates.clone() }]
    }

    /// The list the host stored: order kept, duplicates dropped, at most [`MAX_RECENT_PROJECTS`].
    pub(super) fn restore_recent_templates(&mut self, paths: Vec<PathBuf>) {
        let mut list: Vec<PathBuf> = Vec::new();
        for p in paths {
            if !list.contains(&p) {
                list.push(p);
            }
        }
        list.truncate(MAX_RECENT_PROJECTS);
        self.recent_templates = list;
    }

    pub(super) fn clear_recent_templates(&mut self) -> Vec<HostEffect> {
        self.recent_templates.clear();
        vec![HostEffect::RecentTemplatesChanged { paths: Vec::new() }]
    }
}
