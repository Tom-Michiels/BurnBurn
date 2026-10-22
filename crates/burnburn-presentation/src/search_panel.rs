//! The project search panel (P08, G6-editor): the `.burnburn` files of one folder, searched by file
//! name, object, operation, material, part, plate and the label of a frozen revision. A hit shows a
//! preview and when the file was last saved; a frozen production revision is listed under its
//! project and opens next to the current design as an unsaved copy. Searching and opening only
//! read: nothing here compiles, uploads or records a production intent. Raw field text goes to the
//! controller unparsed (DR-05).

use std::path::PathBuf;
use std::sync::Arc;

use crate::files::ArtThumbnail;

/// What part of a project matched the query, in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum SearchField {
    FileName,
    Object,
    Operation,
    Material,
    Part,
    Plate,
    Revision,
}

/// A frozen production revision of a found project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRevisionRow {
    pub number: u32,
    pub label: String,
}

/// One project of the folder that matches (every project, when the query is empty).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResultRow {
    pub path: PathBuf,
    pub file_name: String,
    /// What matched; empty for the unfiltered list.
    pub matched: Vec<SearchField>,
    /// When the file was last saved (seconds since the Unix epoch), if the file system says.
    pub modified: Option<u64>,
    /// The outline of the first objects, 64 x 64; `None` for an empty project.
    pub thumbnail: Option<Arc<ArtThumbnail>>,
    /// Frozen production revisions, oldest first.
    pub revisions: Vec<SearchRevisionRow>,
    pub objects: usize,
}

/// A project file that could not be read; it is listed, never skipped silently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchIssueRow {
    pub path: PathBuf,
    pub file_name: String,
}

/// The open search panel.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchPanelViewModel {
    /// The folder as typed (and searched).
    pub folder: String,
    pub query: String,
    /// The folder is being read on a worker.
    pub scanning: bool,
    /// Projects read in the last scan (`None` until one finished).
    pub scanned: Option<usize>,
    /// The folder could not be read (it does not exist, or is not a folder).
    pub folder_unreadable: bool,
    pub results: Vec<SearchResultRow>,
    pub issues: Vec<SearchIssueRow>,
}
