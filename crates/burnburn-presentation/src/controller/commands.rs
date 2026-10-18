//! Menu/toolbar commands (`CommandKey`) that run inside one session.

use burnburn_document::{AlignTarget, CommandError, CommandPayload, GeometryCommand, ImportItem};
use burnburn_editor::{SelectionChange, ToolKind};
use burnburn_geometry::{AlignEdge, BooleanOp, Direction, Spacing};
use burnburn_types::{DocumentId, RequestId};

use crate::commit::boolean_options;

use crate::intent::CommandKey;
use crate::task::TaskKind;
use crate::view_model::MessageKey;

use super::geometry::current_boolean_preview;
use super::{
    AppController, AppUpdate, DEFAULT_SNAP_GRID_MM, HostEffect, Session, after_change, apply_edit, apply_selection,
    error,
};

impl AppController {
    pub(super) fn invoke(&mut self, document: Option<DocumentId>, command: CommandKey) -> AppUpdate {
        if command == CommandKey::New {
            return self.new_document();
        }
        let Some(document) = document.or(self.active) else { return AppUpdate::default() };
        match command {
            CommandKey::Save | CommandKey::SaveAs => self.save(document, None),
            CommandKey::Close => self.close(document),
            CommandKey::Measure => self.submit_measure(document),
            CommandKey::Compile => self.submit_compile(document),
            CommandKey::PrepareJob => self.submit_prepare(document),
            CommandKey::ApproveJob => self.with_session(document, |s| match s.prepared.as_mut() {
                Some(p) => {
                    p.approved = true;
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                }
                None => AppUpdate::default(),
            }),
            CommandKey::ExportRd => match self.sessions.get(&document) {
                Some(s) if s.compiled.is_some() => AppUpdate {
                    effects: vec![HostEffect::ShowExportPanel { document, suggested_name: format!("{}.rd", s.title) }],
                    ..AppUpdate::default()
                },
                _ => AppUpdate::default(),
            },
            CommandKey::BooleanUnion => self.submit_boolean(document, BooleanOp::Union),
            CommandKey::BooleanDifference => self.submit_boolean(document, BooleanOp::Difference),
            CommandKey::BooleanIntersection => self.submit_boolean(document, BooleanOp::Intersection),
            _ => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| invoke_in_session(s, document, command, request))
            }
        }
    }
}

pub(super) fn invoke_in_session(
    s: &mut Session,
    document: DocumentId,
    command: CommandKey,
    request: Option<RequestId>,
) -> AppUpdate {
    let align_edge = match command {
        CommandKey::AlignLeft => Some(AlignEdge::Left),
        CommandKey::AlignRight => Some(AlignEdge::Right),
        CommandKey::AlignTop => Some(AlignEdge::Top),
        CommandKey::AlignBottom => Some(AlignEdge::Bottom),
        CommandKey::AlignCenterX => Some(AlignEdge::CenterX),
        CommandKey::AlignCenterY => Some(AlignEdge::CenterY),
        _ => None,
    };
    let direction = match command {
        CommandKey::DistributeHorizontal => Some(Direction::Horizontal),
        CommandKey::DistributeVertical => Some(Direction::Vertical),
        _ => None,
    };
    let targets = s.selection.ids().to_vec();
    let arrange = match (align_edge, direction, s.selection.reference()) {
        // E04: the reference (first selected) stays in place and is visible in the view model.
        (Some(edge), _, Some(reference)) if targets.len() >= 2 => {
            Some(GeometryCommand::Align { targets, edge, to: AlignTarget::Object(reference) })
        }
        (_, Some(direction), _) if targets.len() >= 3 => {
            Some(GeometryCommand::Distribute { targets, direction, spacing: Spacing::Gaps })
        }
        (Some(_), ..) | (_, Some(_), _) => return AppUpdate::default(),
        _ => None,
    };
    if let Some(edit) = arrange {
        // Selection order (and so the reference) is kept across the change.
        let keep = s.selection.clone();
        let update = apply_edit(s, document, CommandPayload::Edit(edit), request);
        s.selection = keep;
        s.selection.retain_existing(&s.doc.snapshot());
        return update;
    }
    match command {
        CommandKey::Undo | CommandKey::Redo => {
            // A gesture in progress is cancelled first; it never reaches history (A4).
            s.editor.cancel();
            let expected = s.doc.revision();
            let result = if command == CommandKey::Undo { s.doc.undo(expected) } else { s.doc.redo(expected) };
            match result {
                Err(CommandError::NothingToUndo | CommandError::NothingToRedo) => AppUpdate::redraw(document),
                other => after_change(s, document, other),
            }
        }
        CommandKey::Delete => {
            if s.selection.is_empty() {
                return AppUpdate::default();
            }
            let payload = CommandPayload::Delete { targets: s.selection.ids().to_vec() };
            apply_edit(s, document, payload, request)
        }
        CommandKey::SelectAll => {
            let all = s.doc.snapshot().objects().map(|(id, _)| id).collect();
            apply_selection(s, &SelectionChange::Replace(all));
            AppUpdate::redraw(document)
        }
        CommandKey::SnapToObjects => {
            s.snap.objects = !s.snap.objects;
            AppUpdate::redraw(document)
        }
        CommandKey::SnapToGrid => {
            s.snap.grid_mm = if s.snap.grid_mm.is_some() { None } else { Some(DEFAULT_SNAP_GRID_MM) };
            AppUpdate::redraw(document)
        }
        CommandKey::ApplyTrace => {
            let Some(t) = s.tracer.as_ref() else { return AppUpdate::default() };
            let Some(result) = t.result.clone() else { return AppUpdate::default() };
            // The source image travels with the project; objects keep it as provenance.
            let asset = s.assets.add(t.media, &t.bytes);
            let items: Vec<ImportItem> = (0..result.components.len())
                .filter_map(|i| {
                    let path = result.component_path(i)?;
                    Some(ImportItem {
                        name: format!("Trace {}", i + 1),
                        entity: Some(format!("trace/component/{i}")),
                        path,
                    })
                })
                .collect();
            apply_edit(s, document, CommandPayload::Edit(GeometryCommand::Import { asset, items }), request)
        }
        CommandKey::CommitImport => {
            let Some(pending) = s.import.as_ref() else { return AppUpdate::default() };
            let Ok(draft) = pending.draft() else { return AppUpdate::default() };
            let asset = s.assets.add(pending.media, &pending.bytes);
            let Ok(edit) = draft.into_command(asset) else { return error(Some(document), MessageKey::ImportFailed) };
            let update = apply_edit(s, document, CommandPayload::Edit(edit), request);
            if s.doc.undo_label() == Some(burnburn_document::UndoLabel::Edit(burnburn_document::EditLabel::Import)) {
                s.import = None;
            }
            update
        }
        CommandKey::CancelImport => {
            s.import = None;
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        }
        CommandKey::CloseTracer => {
            for (kind, src) in s.pending.values() {
                if matches!(kind, TaskKind::Trace | TaskKind::LoadImage) {
                    src.cancel();
                }
            }
            s.tracer = None;
            AppUpdate::redraw(document)
        }
        CommandKey::ApplyPreview => {
            let Some(preview) = current_boolean_preview(s).cloned() else { return AppUpdate::default() };
            let Some(options) = boolean_options() else { return error(Some(document), MessageKey::BooleanFailed) };
            let edit =
                GeometryCommand::Boolean { op: preview.op, targets: preview.operands, options, keep_sources: false };
            s.boolean_preview = None;
            apply_edit(s, document, CommandPayload::Edit(edit), request)
        }
        CommandKey::ToolTrim => {
            s.editor.cancel();
            s.editor.select_tool(ToolKind::Select);
            s.trim_mode = true;
            AppUpdate::redraw(document)
        }
        other => match tool_for(other) {
            Some(tool) => {
                s.trim_mode = false;
                s.editor.select_tool(tool);
                s.tool_size_error = None;
                AppUpdate::redraw(document)
            }
            None => AppUpdate::default(),
        },
    }
}

pub(super) fn tool_for(command: CommandKey) -> Option<ToolKind> {
    Some(match command {
        CommandKey::ToolSelect => ToolKind::Select,
        CommandKey::ToolNode => ToolKind::Node,
        CommandKey::ToolRectangle => ToolKind::Rectangle,
        CommandKey::ToolEllipse => ToolKind::Ellipse,
        CommandKey::ToolCircle => ToolKind::Circle,
        CommandKey::ToolLine => ToolKind::Line,
        CommandKey::ToolPolyline => ToolKind::Polyline,
        CommandKey::ToolPen => ToolKind::Pen,
        _ => return None,
    })
}
