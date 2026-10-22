//! Project search over one folder (IM28, P08) through the `FileSystem` port.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use burnburn_geometry::Path as Outline;
use burnburn_types::ContentHash;

use crate::fs::{FileSystem, FsError};
use crate::{LoadError, Project, ProjectUuid, load_project};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum MatchField {
    FileName,
    Object,
    Operation,
    Material,
    Part,
    Plate,
    Revision,
}

/// What is searchable about one project file.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexEntry {
    pub path: PathBuf,
    pub uuid: ProjectUuid,
    pub project_hash: Option<ContentHash>,
    pub file_name: String,
    pub objects: Vec<String>,
    pub operations: Vec<String>,
    /// Recipe and plate materials.
    pub materials: Vec<String>,
    pub parts: Vec<String>,
    pub plates: Vec<String>,
    /// Frozen revisions (number, label): a production revision stays findable next to the design.
    pub revisions: Vec<(u32, String)>,
    /// When the file was last saved (seconds since the Unix epoch), if the file system says.
    pub modified: Option<u64>,
    /// The outlines of the first [`MAX_OUTLINE_OBJECTS`] objects, for a preview next to a hit.
    pub outline: Vec<Arc<Outline>>,
}

/// How many objects of a project its search preview is made of.
pub const MAX_OUTLINE_OBJECTS: usize = 400;

impl IndexEntry {
    fn of(path: &Path, p: &Project, modified: Option<u64>) -> Self {
        let doc = &p.document;
        let st = doc.structure();
        let mut materials: Vec<String> = doc.process().recipes().map(|r| r.material().to_owned()).collect();
        materials.extend(st.plates().map(|(_, pl)| pl.material.clone()));
        IndexEntry {
            path: path.to_path_buf(),
            uuid: p.uuid,
            project_hash: crate::project_hash(p).ok(),
            file_name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            objects: doc.objects().map(|(_, o)| o.name().to_owned()).collect(),
            operations: doc.process().operations().map(|o| o.name().to_owned()).collect(),
            materials,
            parts: st.definitions().map(|(_, d)| d.name.clone()).collect(),
            plates: st.plates().map(|(_, pl)| pl.name.clone()).collect(),
            revisions: p.revisions.iter().map(|r| (r.number, r.label.clone())).collect(),
            modified,
            outline: doc.objects().take(MAX_OUTLINE_OBJECTS).map(|(_, o)| o.path().clone()).collect(),
        }
    }
}

/// A `.burnburn` file that could not be indexed, with why.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexIssue {
    pub path: PathBuf,
    pub error: LoadError,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProjectIndex {
    entries: Vec<IndexEntry>,
    issues: Vec<IndexIssue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit<'a> {
    pub entry: &'a IndexEntry,
    pub fields: Vec<MatchField>,
}

impl ProjectIndex {
    pub fn entries(&self) -> &[IndexEntry] {
        &self.entries
    }
    pub fn issues(&self) -> &[IndexIssue] {
        &self.issues
    }
    /// Case-insensitive substring search; each hit names the fields that matched, in
    /// [`MatchField`] order. An empty query finds nothing.
    pub fn search(&self, query: &str) -> Vec<Hit<'_>> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let has = |items: &mut dyn Iterator<Item = &String>| {
            for s in items {
                if s.to_lowercase().contains(&q) {
                    return true;
                }
            }
            false
        };
        self.entries
            .iter()
            .filter_map(|entry| {
                let mut fields = Vec::new();
                let checks: [(MatchField, bool); 7] = [
                    (MatchField::FileName, entry.file_name.to_lowercase().contains(&q)),
                    (MatchField::Object, has(&mut entry.objects.iter())),
                    (MatchField::Operation, has(&mut entry.operations.iter())),
                    (MatchField::Material, has(&mut entry.materials.iter())),
                    (MatchField::Part, has(&mut entry.parts.iter())),
                    (MatchField::Plate, has(&mut entry.plates.iter())),
                    (MatchField::Revision, has(&mut entry.revisions.iter().map(|(_, l)| l))),
                ];
                fields.extend(checks.into_iter().filter(|(_, hit)| *hit).map(|(f, _)| f));
                (!fields.is_empty()).then_some(Hit { entry, fields })
            })
            .collect()
    }
}

/// Indexes the `.burnburn` files directly in `dir` (no OS search index; read-only: nothing is
/// written and no production intent is created). Files that do not load are listed in
/// [`ProjectIndex::issues`] with their error. Other files are not projects and are ignored.
pub fn index_projects(fs: &dyn FileSystem, dir: &Path) -> Result<ProjectIndex, FsError> {
    let mut index = ProjectIndex::default();
    for path in fs.list_dir(dir)? {
        if path.extension().and_then(|e| e.to_str()) != Some("burnburn") {
            continue;
        }
        match load_project(fs, &path) {
            Ok(p) => {
                // A file system that cannot say leaves the time unknown (a failure to ask too).
                let modified = fs.modified(&path).ok().flatten();
                index.entries.push(IndexEntry::of(&path, &p, modified));
            }
            Err(error) => index.issues.push(IndexIssue { path, error }),
        }
    }
    Ok(index)
}
