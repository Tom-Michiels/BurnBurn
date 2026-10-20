//! Project templates (File ▸ Save As Template, new from template; M018/M024).
//!
//! A template is an ordinary project container saved under the [`TEMPLATE_EXTENSION`]; the
//! extension is the marker, and neither route accepts the other's extension, so opening a
//! template never offers to overwrite it and "Save" on a new project always needs a fresh path.
//! A new project made from a template has a fresh identity (supplied by the caller), no frozen
//! revisions, and nothing is written to the template.

use std::path::Path;

use burnburn_document::DocumentSnapshot;
use burnburn_document::process::{Operation, OperationMode, ProcessLayer};

use crate::error::{LoadError, SaveError};
use crate::fs::FileSystem;
use crate::project::{AssetStore, Project, ProjectUuid};
use crate::store::{load_project, save_project};

pub const PROJECT_EXTENSION: &str = "burnburn";
pub const TEMPLATE_EXTENSION: &str = "burnburntemplate";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateContent {
    /// The whole design: geometry, operations, recipes, assets and frozen machine profiles.
    Everything,
    /// Operations (without targets), order constraints, recipes and machine profiles only.
    SettingsOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// The path does not end in the template extension.
    WrongExtension {
        expected: &'static str,
    },
    Save(SaveError),
    Load(LoadError),
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateError::WrongExtension { expected } => write!(f, "a template file must end in .{expected}"),
            TemplateError::Save(e) => write!(f, "{e}"),
            TemplateError::Load(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for TemplateError {}

fn check_extension(path: &Path) -> Result<(), TemplateError> {
    let ok = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case(TEMPLATE_EXTENSION));
    if ok { Ok(()) } else { Err(TemplateError::WrongExtension { expected: TEMPLATE_EXTENSION }) }
}

fn invalid(what: &str, e: impl std::fmt::Display) -> TemplateError {
    TemplateError::Save(SaveError::Invalid(format!("{what}: {e}")))
}

/// The project a template stores for `project` (pure; no I/O). Frozen revisions never belong
/// to a template.
pub fn template_of(project: &Project, content: TemplateContent) -> Result<Project, TemplateError> {
    match content {
        TemplateContent::Everything => Ok(Project {
            uuid: project.uuid,
            document: project.document.clone(),
            assets: project.assets.clone(),
            profiles: project.profiles.clone(),
            revisions: Vec::new(),
            library_links: project.library_links.clone(),
        }),
        TemplateContent::SettingsOnly => {
            let layer = project.document.process();
            let mut operations = Vec::new();
            let mut keep = Vec::new();
            for op in layer.operations() {
                collect_sources(op.mode(), &mut keep);
                for sub in &op.advanced().sub_operations {
                    collect_sources(sub.mode, &mut keep);
                }
                let bare = Operation::new(op.id(), op.draft(), Vec::new(), op.recipe())
                    .and_then(|o| o.with_advanced(op.advanced().clone()))
                    .map_err(|e| invalid("operation", e))?;
                operations.push(bare);
            }
            let process =
                ProcessLayer::from_parts(operations, layer.constraints().to_vec(), layer.recipes().cloned().collect())
                    .map_err(|e| invalid("process layer", e))?;
            let document = DocumentSnapshot::from_parts(Vec::new(), project.document.next_id())
                .and_then(|d| d.with_process(process))
                .map_err(|e| invalid("document", e))?;
            let mut assets = AssetStore::default();
            for hash in keep {
                if let Some(a) = project.assets.get(&hash) {
                    assets.add(&a.media, &a.bytes);
                }
            }
            Ok(Project {
                uuid: project.uuid,
                document,
                assets,
                profiles: project.profiles.clone(),
                revisions: Vec::new(),
                // The recipes survive in a settings-only template, so their provenance does too.
                library_links: project.library_links.clone(),
            })
        }
    }
}

fn collect_sources(mode: OperationMode, out: &mut Vec<burnburn_types::ContentHash>) {
    if let OperationMode::Image(settings) = mode {
        out.push(settings.source());
    }
}

/// File ▸ Save As Template.
pub fn save_template(
    fs: &dyn FileSystem,
    path: &Path,
    project: &Project,
    content: TemplateContent,
) -> Result<(), TemplateError> {
    check_extension(path)?;
    save_project(fs, path, &template_of(project, content)?).map_err(TemplateError::Save)
}

/// New project from a template: the template's content with the caller's fresh `uuid`. The
/// template file is only read.
pub fn new_from_template(fs: &dyn FileSystem, path: &Path, uuid: ProjectUuid) -> Result<Project, TemplateError> {
    check_extension(path)?;
    let mut project = load_project(fs, path).map_err(TemplateError::Load)?;
    project.uuid = uuid;
    project.revisions.clear();
    Ok(project)
}
