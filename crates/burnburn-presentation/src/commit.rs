//! Editor commit → document payload. This is the single place where an interaction result meets
//! the document command set (contract §4: the editor knows no `DocumentCommand`).

use burnburn_document::{CommandPayload, GeometryCommand};
use burnburn_editor::{EditCommit, ShapeKind};
use burnburn_geometry::{BooleanOptions, FillRule, ProductionTolerance, Shape};
use burnburn_types::Length;

/// Default object names (NL; the document stores user data, not localisation keys).
pub(crate) fn base_name(kind: NameKind) -> &'static str {
    match kind {
        NameKind::Rectangle => "Rechthoek",
        NameKind::Shape(ShapeKind::Ellipse) => "Ellips",
        NameKind::Shape(ShapeKind::Circle) => "Cirkel",
        NameKind::Shape(ShapeKind::Line) => "Lijn",
        NameKind::Shape(ShapeKind::Polyline) => "Polylijn",
        NameKind::Shape(_) => "Curve",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum NameKind {
    Rectangle,
    Shape(ShapeKind),
}

/// What a commit creates, for its default name.
pub(crate) fn name_kind(edit: &EditCommit) -> Option<NameKind> {
    match edit {
        EditCommit::CreateRectangle { .. } => Some(NameKind::Rectangle),
        EditCommit::CreateShape { shape } => Some(NameKind::Shape(match shape {
            Shape::Ellipse { .. } => ShapeKind::Ellipse,
            Shape::Circle { .. } => ShapeKind::Circle,
            Shape::Line { .. } => ShapeKind::Line,
            Shape::Polyline { .. } => ShapeKind::Polyline,
            _ => ShapeKind::Curve,
        })),
        EditCommit::CreatePath { .. } => Some(NameKind::Shape(ShapeKind::Curve)),
        _ => None,
    }
}

/// The document payload for one editor commit (one command, one history entry).
pub(crate) fn payload(edit: EditCommit, name: String) -> Option<CommandPayload> {
    Some(match edit {
        EditCommit::CreateRectangle { rect } => CommandPayload::CreateRectangle { rect, name },
        EditCommit::Transform { targets, transform, .. } => CommandPayload::Transform { targets, transform },
        EditCommit::CreateShape { shape } => CommandPayload::Edit(GeometryCommand::CreateShape { shape, name }),
        EditCommit::CreatePath { path } => CommandPayload::Edit(GeometryCommand::CreatePath { path, name }),
        // The document re-applies the same domain edit; the preview path is not trusted input.
        EditCommit::EditNodes { target, edit, .. } => CommandPayload::Edit(GeometryCommand::EditNodes { target, edit }),
        _ => return None,
    })
}

/// Chord tolerance for flattening curves in booleans (the result is polygonal).
const BOOLEAN_TOLERANCE_MM: f64 = 0.01;

/// The options used for both the boolean preview task and the applied command, so the applied
/// result is the previewed one.
pub(crate) fn boolean_options() -> Option<BooleanOptions> {
    let tolerance = ProductionTolerance::new(Length::from_mm(BOOLEAN_TOLERANCE_MM).ok()?).ok()?;
    Some(BooleanOptions { fill: FillRule::EvenOdd, tolerance })
}
