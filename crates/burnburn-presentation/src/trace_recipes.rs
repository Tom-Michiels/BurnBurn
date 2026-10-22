//! Own recipes, A/B comparison and batch tracing in the tracer (G7-trace, T10).
//!
//! A saved recipe holds the settings that belong to a kind of image (threshold, filters, fit
//! budget, lens): not the resolution, the crop, the rotation, the plane correction, the masks or
//! the kept pieces, which belong to one picture. Applying one puts its settings on top of those of
//! the picture in the tracer. Recipes live in the user's folder, never in a project.

use std::path::PathBuf;
use std::sync::Arc;

use burnburn_geometry::Path;

use crate::image_panel::{TraceMode, TracePreset};

/// Where the other variant of a comparison comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecipeChoice {
    Saved(String),
    Preset(TracePreset),
}

/// One action on the recipes of the tracer.
#[derive(Debug, Clone, PartialEq)]
pub enum TraceRecipeIntent {
    /// Keep the settings in force under this name; the same name replaces that recipe.
    Save {
        name: String,
    },
    /// Put the saved recipe's settings on the picture in the tracer.
    Apply {
        name: String,
    },
    Delete {
        name: String,
    },
    /// Trace the picture with the other recipe too and show both in one frame (contour mode).
    Compare {
        with: RecipeChoice,
    },
    ClearCompare,
    /// Link (or unlink) the stored local correction `index` to the contours of B although it no longer binds
    /// there (G9): the comparison is traced again with the correction applied to B.
    RelinkCorrection {
        index: usize,
        on: bool,
    },
    /// Make the compared recipe (B) the one in force.
    AdoptCompared,
    /// Trace these files with the settings in force (contour mode), one report line per file.
    RunBatch {
        paths: Vec<PathBuf>,
    },
    CancelBatch,
    ClearBatch,
}

/// Why a recipe action was refused. Nothing changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecipeIssue {
    EmptyName,
    NameTooLong,
    TooMany,
    /// No recipe with that name.
    Unknown,
    /// Comparing and batch tracing are for contour traces.
    NeedsContourMode,
    /// The picture has no declared resolution yet: there is nothing physical to compare.
    NeedsScale,
    /// A batch without files.
    NoFiles,
    /// The recipe could not be made valid for this picture.
    Invalid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SavedRecipeRow {
    pub name: String,
    pub mode: TraceMode,
}

/// Numbers of one traced variant, the same ones the report of a trace shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VariantSummary {
    pub paths: usize,
    pub closed_shapes: usize,
    pub holes: usize,
    pub nodes: usize,
    pub removed: usize,
    pub max_deviation_mm: f64,
}

/// How many of the tracer's local corrections each variant of a comparison carries (G8-trace, T10).
/// A correction that no longer binds on a variant's contours is not applied there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompareCorrectionsView {
    pub stored: usize,
    pub a_applied: usize,
    pub b_applied: usize,
}

/// Where a stored local correction stands on variant B of a comparison (G9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareCorrectionState {
    /// Not applied to B: `lost` contours it was bound to are gone, `new_in_region` new ones lie in its area.
    Unbound { lost: usize, new_in_region: usize },
    /// Linked to B's contours by the operator although it no longer bound.
    Relinked,
}

/// One correction that did not bind on B, or that the operator linked there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompareCorrectionRow {
    /// Position in the tracer's list of corrections.
    pub index: usize,
    pub state: CompareCorrectionState,
}

/// Recipe in force (A) against another (B) on the same picture.
#[derive(Debug, Clone, PartialEq)]
pub struct CompareViewModel {
    /// The local corrections of the tracer, when it has any (applied to both variants).
    pub corrections: Option<CompareCorrectionsView>,
    /// The corrections that do not bind on B (and can be linked to it) or were linked, in list order.
    pub b_corrections: Vec<CompareCorrectionRow>,
    /// Where B comes from: the saved recipe or the built-in.
    pub b_choice: RecipeChoice,
    pub computing: bool,
    pub a: Option<VariantSummary>,
    pub b: Option<VariantSummary>,
    /// Both gave exactly the same geometry.
    pub same_geometry: bool,
    /// Both outlines in the frame of the trace, so a view can lay them over each other at one zoom.
    pub a_outline: Option<Arc<Path>>,
    pub b_outline: Option<Arc<Path>>,
}

/// What to look at for one file of a batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchProblemView {
    /// Nothing was traced.
    NoGeometry,
    /// "Ignore less than" dropped this many parts or holes.
    RemovedFeatures { count: usize },
    /// The file has no resolution and the recipe has none either.
    NeedsScale,
    /// The file could not be read or decoded.
    Unreadable,
    /// The trace itself failed.
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchRowView {
    pub name: String,
    pub summary: Option<VariantSummary>,
    pub problems: Vec<BatchProblemView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchViewModel {
    pub total: usize,
    /// The files are still being traced.
    pub running: bool,
    /// The batch was cancelled; there is no report.
    pub cancelled: bool,
    pub rows: Vec<BatchRowView>,
}

impl BatchViewModel {
    /// Files with something to look at.
    pub fn problem_count(&self) -> usize {
        self.rows.iter().filter(|r| !r.problems.is_empty()).count()
    }
}

/// The recipe panel of the open tracer.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TraceRecipesViewModel {
    pub saved: Vec<SavedRecipeRow>,
    pub issue: Option<RecipeIssue>,
    pub compare: Option<CompareViewModel>,
    pub batch: Option<BatchViewModel>,
    /// The recipe file in the user's folder could not be read; it is left alone.
    pub load_problem: bool,
    /// The last write of the recipe file did not succeed; the recipes of this run are still in use.
    pub save_failed: bool,
}
