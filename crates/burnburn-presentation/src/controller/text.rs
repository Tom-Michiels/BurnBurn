//! Text input: fonts, `InsertText` and the shared font source of every session's evaluator.

use burnburn_geometry::{Affine, Path, ProductionTolerance, Vector};
use std::sync::Arc;

use burnburn_document::text::PathBinding;
use burnburn_document::text::{TextCommand, TextId, TextMode, TextObject};
use burnburn_document::{
    CommandPayload, CompoundCommand, CompoundLabel, CompoundStep, DerivedOwner, DocumentSnapshot, GeometryCommand,
    GeometrySource,
};
use burnburn_generators::{GenError, PathTextParams, guide_frame_at, text_on_path, text_room};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId};

use burnburn_text::{FontData, FontError, FontRef, FontSize, FontSource, FontStyle, switch_style};

use crate::field::{FieldError, format_mm, parse_length, parse_position};
use crate::text_panel::{
    Distort, PathBindingStatus, TextEdit, TextField, TextIssue, TextMixed, TextOnPathViewModel, TextPathEdit,
    TextPathView, TextToggle, TextToolbarViewModel,
};
use crate::view_model::MessageKey;

use super::{AppController, AppUpdate, DEFAULT_TEXT_MM, Session, apply_edit, error};

/// Chord tolerance of the text on a path, like the compile tolerance (mm).
const PATH_TEXT_TOLERANCE_MM: f64 = 0.01;

/// Window state of the text panels: the last refused toolbar input and the text-on-path dialog.
#[derive(Debug, Default)]
pub(super) struct TextState {
    /// The text the last refusal belongs to, and why.
    issue: Option<(TextId, TextIssue)>,
    path: Option<PathDialog>,
}

/// The text-on-path dialog's values as typed.
#[derive(Debug, Clone)]
struct PathDialog {
    distort: Distort,
    start: String,
    offset: String,
    reverse: bool,
    /// Keep the text bound to the guide (E08) instead of making ordinary paths.
    live: bool,
    issue: Option<TextIssue>,
}

impl Default for PathDialog {
    fn default() -> Self {
        PathDialog {
            distort: Distort::Rigid,
            start: "0".into(),
            offset: "0".into(),
            reverse: false,
            live: true,
            issue: None,
        }
    }
}

/// The text object a selected object is an outline of.
fn text_of(snapshot: &DocumentSnapshot, id: burnburn_types::GeometryId) -> Option<TextId> {
    match snapshot.object(id)?.source() {
        GeometrySource::Derived(k) => match k.owner {
            DerivedOwner::Text(t) => Some(t),
            _ => None,
        },
        _ => None,
    }
}

/// The one text object all selected objects are outlines of (a text of several lines has
/// several outlines); `None` for no selection, other objects or several texts.
pub(super) fn single_text(s: &Session, snapshot: &DocumentSnapshot) -> Option<TextId> {
    let mut found = None;
    for id in s.selection.ids() {
        let t = text_of(snapshot, *id)?;
        if found.is_some_and(|f| f != t) {
            return None;
        }
        found = Some(t);
    }
    found
}

/// Every distinct text object the selection is made of, in id order (a text of several lines has
/// several outlines but counts once); empty for no selection, or when anything else than text
/// outlines is selected (G7-editor, E07).
pub(super) fn selected_texts(s: &Session, snapshot: &DocumentSnapshot) -> Vec<TextId> {
    let mut found: Vec<TextId> = Vec::new();
    for id in s.selection.ids() {
        let Some(t) = text_of(snapshot, *id) else { return Vec::new() };
        if !found.contains(&t) {
            found.push(t);
        }
    }
    found.sort();
    found
}

fn invalid(field: TextField) -> TextIssue {
    TextIssue::InvalidNumber(field)
}

fn number(text: &str, field: TextField, signed: bool) -> Result<f64, TextIssue> {
    let parsed: Result<Length, FieldError> = if signed { parse_position(text) } else { parse_length(text) };
    parsed.map(Length::mm).map_err(|_| invalid(field))
}

/// The byte offset in `content` of the position typed in a field (G9-editor, E07): a whole number
/// of characters from the start, a negative one from the end, an empty one the end; beyond either
/// end is that end.
fn byte_at(content: &str, position: &str) -> Result<usize, TextIssue> {
    let position = position.trim();
    if position.is_empty() {
        return Ok(content.len());
    }
    let n = position.parse::<i64>().map_err(|_| invalid(TextField::Position))?;
    let total = content.chars().count();
    let index = if n >= 0 {
        usize::try_from(n).map_or(total, |n| n.min(total))
    } else {
        total.saturating_sub(usize::try_from(n.unsigned_abs()).unwrap_or(usize::MAX))
    };
    Ok(content.char_indices().nth(index).map_or(content.len(), |(i, _)| i))
}

/// What `edit` changes in a copy of the source; an `Err` is shown in the toolbar and sends nothing.
fn apply(next: &mut TextObject, fonts: Option<&SharedFonts>, edit: TextEdit) -> Result<(), TextIssue> {
    match edit {
        TextEdit::Family(name) => {
            let fonts = fonts.ok_or(TextIssue::NoFonts)?;
            if name == next.font.family {
                return Ok(());
            }
            if !fonts.families().contains(&name) {
                return Err(TextIssue::FamilyUnavailable);
            }
            let styles = fonts.available_styles(&name);
            let style = [next.font.style, FontStyle::Regular]
                .into_iter()
                .find(|s| styles.contains(s))
                .ok_or(TextIssue::FamilyUnavailable)?;
            next.font = FontRef::new(name, style);
        }
        TextEdit::Bold(on) => {
            let style = FontStyle::from_flags(on, next.font.style.is_italic());
            set_style(next, fonts, style)?;
        }
        TextEdit::Italic(on) => {
            let style = FontStyle::from_flags(next.font.style.is_bold(), on);
            set_style(next, fonts, style)?;
        }
        TextEdit::Uppercase(on) => next.layout.uppercase = on,
        TextEdit::Align(a) => next.layout.align = a,
        TextEdit::VAlign(a) => next.layout.valign = a,
        TextEdit::HSpace(text) => next.layout.letter_spacing_mm = number(&text, TextField::HSpace, true)?,
        TextEdit::VSpace(text) => next.layout.v_space_mm = number(&text, TextField::VSpace, true)?,
        TextEdit::Height(text) => {
            let mm = number(&text, TextField::Height, false)?;
            next.size = Length::from_mm(mm)
                .ok()
                .and_then(|l| FontSize::from_mm(l).ok())
                .ok_or_else(|| invalid(TextField::Height))?;
        }
        TextEdit::Welded(on) => {
            if on && next.mode != TextMode::Outline {
                return Err(TextIssue::WeldedNeedsOutline);
            }
            next.layout.welded = on;
        }
        TextEdit::SingleLine(on) => {
            next.mode = if on { TextMode::SingleLine } else { TextMode::Outline };
            if on {
                next.layout.welded = false;
            }
        }
        TextEdit::PathStart(text) => {
            let start = number(&text, TextField::PathStart, true)?;
            if start < 0.0 {
                return Err(invalid(TextField::PathStart));
            }
            next.path.as_mut().ok_or(TextIssue::NotOnPath)?.start_mm = start;
        }
        TextEdit::PathOffset(text) => {
            let offset = number(&text, TextField::PathOffset, true)?;
            next.path.as_mut().ok_or(TextIssue::NotOnPath)?.offset_mm = offset;
        }
        TextEdit::PathReverse(on) => next.path.as_mut().ok_or(TextIssue::NotOnPath)?.reverse = on,
        TextEdit::PathDistort(d) => next.path.as_mut().ok_or(TextIssue::NotOnPath)?.warp = d == Distort::Warp,
        TextEdit::UnbindPath => {
            next.path.take().ok_or(TextIssue::NotOnPath)?;
        }
        TextEdit::InsertAt { position, text } => {
            let at = byte_at(&next.content, &position)?;
            next.content.insert_str(at, &text);
        }
        TextEdit::DeleteAt { position, count } => {
            let count = count.trim().parse::<usize>().map_err(|_| invalid(TextField::Count))?;
            let from = byte_at(&next.content, &position)?;
            let end = next.content[from..].char_indices().nth(count).map_or(next.content.len(), |(i, _)| from + i);
            next.content.replace_range(from..end, "");
        }
    }
    Ok(())
}

/// Bold and italic pick the face of the same family; a family without it refuses (shaping would
/// report the font missing, it never swaps the face).
fn set_style(next: &mut TextObject, fonts: Option<&SharedFonts>, style: FontStyle) -> Result<(), TextIssue> {
    if style == next.font.style {
        return Ok(());
    }
    let fonts = fonts.ok_or(TextIssue::NoFonts)?;
    if !fonts.available_styles(&next.font.family).contains(&style) {
        return Err(TextIssue::StyleMissing);
    }
    next.font = switch_style(&next.font, style.is_bold(), style.is_italic());
    Ok(())
}

fn is_path_edit(edit: &TextEdit) -> bool {
    matches!(
        edit,
        TextEdit::PathStart(_) | TextEdit::PathOffset(_) | TextEdit::PathReverse(_) | TextEdit::PathDistort(_)
    )
}

/// A move of text that follows a guide (G8-editor, E08). The outline of a bound text is derived,
/// so the move is made on its definition: a pure translation of the text alone changes where it
/// starts on the guide (the displacement along the guide's direction there, kept within the
/// room the guide has for the text); a text whose guide moves with it simply follows. Anything
/// else stays as it was and the document refuses it. One command, so one undo step.
pub(super) fn retarget_move(
    snapshot: &DocumentSnapshot,
    fonts: Option<&SharedFonts>,
    payload: CommandPayload,
) -> CommandPayload {
    let CommandPayload::Transform { targets, transform } = &payload else { return payload };
    let mut bound: Vec<(TextId, TextObject)> = Vec::new();
    let mut rest: Vec<GeometryId> = Vec::new();
    for id in targets {
        match text_of(snapshot, *id).and_then(|t| snapshot.text(t).filter(|o| o.path.is_some()).map(|o| (t, o))) {
            Some((t, obj)) => {
                if !bound.iter().any(|(b, _)| *b == t) {
                    bound.push((t, obj.clone()));
                }
            }
            None => rest.push(*id),
        }
    }
    if bound.is_empty() {
        return payload;
    }
    let mut sets: Vec<(TextId, TextObject)> = Vec::new();
    for (id, obj) in bound {
        let Some(binding) = obj.path else { return payload };
        if targets.contains(&binding.guide) {
            continue; // the guide moves too: the text follows it
        }
        let [a, b, c, d, e, f] = transform.coefficients();
        let pure_move = (a - 1.0).abs() < 1e-12 && b.abs() < 1e-12 && c.abs() < 1e-12 && (d - 1.0).abs() < 1e-12;
        let (Some(fonts), true) = (fonts, pure_move) else { return payload };
        let Some(guide) = snapshot.object(binding.guide) else { return payload };
        let Some(tolerance) =
            Length::from_mm(PATH_TEXT_TOLERANCE_MM).ok().and_then(|l| ProductionTolerance::new(l).ok())
        else {
            return payload;
        };
        let Ok((_, direction)) = guide_frame_at(guide.path(), &tolerance, binding.reverse, binding.start_mm) else {
            return payload;
        };
        let Ok(room) = text_room(&obj, fonts, guide.path(), &tolerance, binding.reverse) else { return payload };
        let start = (binding.start_mm + e * direction.x + f * direction.y).clamp(0.0, room);
        if (start - binding.start_mm).abs() < 1e-9 {
            continue;
        }
        let mut next = obj.clone();
        next.path = Some(PathBinding { start_mm: start, ..binding });
        sets.push((id, next));
    }
    let mut steps: Vec<CompoundStep> = Vec::new();
    if !rest.is_empty() {
        steps.push(CompoundStep::Run(CommandPayload::Transform { targets: rest, transform: *transform }));
    }
    steps.extend(sets.into_iter().map(|(id, next)| {
        CompoundStep::Run(CommandPayload::Text(TextCommand::Set { text: id, source: Box::new(next) }))
    }));
    match steps.len() {
        0 => payload,
        1 => match steps.remove(0) {
            CompoundStep::Run(only) => only,
            _ => payload,
        },
        _ => CommandPayload::Compound(CompoundCommand { label: CompoundLabel::EditTexts, steps }),
    }
}

/// Whether `obj` (bound to a guide) can be bent along its guide now, or why not.
fn check_fit(snapshot: &DocumentSnapshot, fonts: Option<&SharedFonts>, obj: &TextObject) -> Result<(), TextIssue> {
    let fonts = fonts.ok_or(TextIssue::NoFonts)?;
    let binding = obj.path.ok_or(TextIssue::NotOnPath)?;
    let guide = snapshot.object(binding.guide).ok_or(TextIssue::PathFailed)?;
    let tolerance = Length::from_mm(PATH_TEXT_TOLERANCE_MM)
        .ok()
        .and_then(|l| ProductionTolerance::new(l).ok())
        .ok_or(TextIssue::PathFailed)?;
    let params = PathTextParams {
        distort: if binding.warp { Distort::Warp } else { Distort::Rigid },
        start: binding.start_mm,
        offset: binding.offset_mm,
        reverse: binding.reverse,
        tolerance,
    };
    text_on_path(obj, fonts, guide.path(), &params).map(|_| ()).map_err(|e| path_issue(&e))
}

/// One toolbar edit of the selected text(s): one `TextCommand::Set` for one text that changes, one
/// compound (one undo step) for several, or a reason in the toolbar. When any of the texts cannot
/// take the edit, none of them changes.
pub(super) fn edit(
    s: &mut Session,
    document: DocumentId,
    fonts: Option<&SharedFonts>,
    edit: TextEdit,
    request: Option<RequestId>,
) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let ids = selected_texts(s, &snapshot);
    let Some(first) = ids.first().copied() else { return AppUpdate::default() };
    let mut changes: Vec<(TextId, TextObject)> = Vec::new();
    for id in &ids {
        let Some(current) = snapshot.text(*id) else { return AppUpdate::default() };
        let mut next = current.clone();
        let fitted = apply(&mut next, fonts, edit.clone()).and_then(|()| {
            // A changed binding must still carry the text: the guide is looked at before the command.
            if is_path_edit(&edit) && next.path.is_some() { check_fit(&snapshot, fonts, &next) } else { Ok(()) }
        });
        if let Err(issue) = fitted {
            s.text.issue = Some((first, issue));
            return AppUpdate { view_changed: true, ..AppUpdate::default() };
        }
        if next != *current {
            changes.push((*id, next));
        }
    }
    let had_issue = s.text.issue.take().is_some();
    let payload = match changes.len() {
        0 => return AppUpdate { view_changed: had_issue, ..AppUpdate::default() },
        1 => {
            let (id, next) = changes.remove(0);
            CommandPayload::Text(TextCommand::Set { text: id, source: Box::new(next) })
        }
        _ => CommandPayload::Compound(CompoundCommand {
            label: CompoundLabel::EditTexts,
            steps: changes
                .into_iter()
                .map(|(id, next)| {
                    CompoundStep::Run(CommandPayload::Text(TextCommand::Set { text: id, source: Box::new(next) }))
                })
                .collect(),
        }),
    };
    apply_edit(s, document, payload, request)
}

/// Whether Convert to Path can act: the selection is made of the outlines of one or more texts.
pub(super) fn convertible(s: &Session) -> bool {
    let snapshot = s.doc.snapshot();
    let ids = selected_texts(s, &snapshot);
    !ids.is_empty() && ids.iter().all(|id| snapshot.text(*id).is_some())
}

/// Convert to Path of the selected text(s): their outlines become ordinary objects. One text is
/// one `ConvertToPath`, several are one compound (G8-editor, E07): one undo step either way.
pub(super) fn convert(s: &mut Session, document: DocumentId, request: Option<RequestId>) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let mut ids = selected_texts(s, &snapshot);
    let payload = match ids.len() {
        0 => return AppUpdate::default(),
        1 => CommandPayload::Text(TextCommand::ConvertToPath { text: ids.remove(0) }),
        _ => CommandPayload::Compound(CompoundCommand {
            label: CompoundLabel::ConvertTexts,
            steps: ids
                .into_iter()
                .map(|text| CompoundStep::Run(CommandPayload::Text(TextCommand::ConvertToPath { text })))
                .collect(),
        }),
    };
    apply_edit(s, document, payload, request)
}

/// The toolbar for the selected text(s): the first text's values, with the differing ones marked.
pub(super) fn toolbar_view(s: &Session) -> Option<TextToolbarViewModel> {
    let snapshot = s.doc.snapshot();
    let ids = selected_texts(s, &snapshot);
    let views: Vec<TextToolbarViewModel> = ids.iter().filter_map(|id| toolbar_of(s, &snapshot, *id)).collect();
    let (first, rest) = views.split_first()?;
    let mut out = first.clone();
    out.count = views.len();
    let mut m = TextMixed::default();
    for v in rest {
        m.family |= v.family != first.family;
        m.bold |= v.bold.on != first.bold.on;
        m.italic |= v.italic.on != first.italic.on;
        m.uppercase |= v.uppercase != first.uppercase;
        m.align |= v.align != first.align;
        m.valign |= v.valign != first.valign;
        m.h_space |= v.h_space != first.h_space;
        m.v_space |= v.v_space != first.v_space;
        m.height |= v.height != first.height;
        m.welded |= v.welded.on != first.welded.on;
        m.single_line |= v.single_line != first.single_line;
        for (mine, theirs) in [(&mut out.bold, &v.bold), (&mut out.italic, &v.italic), (&mut out.welded, &v.welded)] {
            if !theirs.enabled && mine.enabled {
                mine.enabled = false;
                mine.why_off = theirs.why_off;
            }
        }
        out.missing_glyphs += v.missing_glyphs;
        out.font_missing |= v.font_missing;
        if out.missing_font.is_none() {
            out.missing_font.clone_from(&v.missing_font);
        }
        out.variable |= v.variable;
        out.can_convert &= v.can_convert;
        if out.issue.is_none() {
            out.issue = v.issue;
        }
    }
    out.bold.on = views.iter().all(|v| v.bold.on);
    out.italic.on = views.iter().all(|v| v.italic.on);
    out.welded.on = views.iter().all(|v| v.welded.on);
    out.uppercase = views.iter().all(|v| v.uppercase);
    out.single_line = views.iter().all(|v| v.single_line);
    out.mixed = m;
    out.contents = views.iter().flat_map(|v| v.contents.iter().cloned()).collect();
    let mut families: Vec<String> = views.iter().flat_map(|v| v.families.iter().cloned()).collect();
    families.sort();
    families.dedup();
    out.families = families.into();
    // The toolbar speaks for all of them: a refusal belongs to the selection, not to the first text.
    out.issue = s.text.issue.filter(|(t, _)| ids.contains(t)).map(|(_, i)| i);
    Some(out)
}

/// The toolbar of one text object.
fn toolbar_of(s: &Session, snapshot: &DocumentSnapshot, id: TextId) -> Option<TextToolbarViewModel> {
    let t = snapshot.text(id)?;
    let styles = s.fonts.as_ref().map(|f| f.available_styles(&t.font.family)).unwrap_or_default();
    let toggle = |on: bool, target: FontStyle| {
        let enabled = styles.contains(&target);
        TextToggle { on, enabled, why_off: (!enabled).then_some(TextIssue::StyleMissing) }
    };
    let (bold, italic) = (t.font.style.is_bold(), t.font.style.is_italic());
    let mut families = s.fonts.as_ref().map(|f| f.families()).unwrap_or_default();
    // A project opened on a computer without the font still holds the stored outlines as "valid":
    // the family itself is what is missing (only knowable once a font source is attached).
    let family_absent = s.fonts.is_some() && !families.contains(&t.font.family);
    let installed = families.clone();
    if !families.contains(&t.font.family) {
        families.push(t.font.family.clone());
        families.sort();
    }
    let welded_ok = t.mode == TextMode::Outline;
    let font_missing = family_absent
        || matches!(
            snapshot.derived_record(DerivedOwner::Text(id)).map(|r| r.status.clone()),
            Some(burnburn_document::EvalStatus::Failed {
                diagnosis: burnburn_document::EvalDiagnosis::FontMissing { .. }
            })
        );
    Some(TextToolbarViewModel {
        count: 1,
        mixed: TextMixed::default(),
        families: families.into(),
        family: t.font.family.clone(),
        bold: toggle(bold, FontStyle::from_flags(!bold, italic)),
        italic: toggle(italic, FontStyle::from_flags(bold, !italic)),
        uppercase: t.layout.uppercase,
        align: t.layout.align,
        valign: t.layout.valign,
        h_space: format_mm(t.layout.letter_spacing_mm),
        v_space: format_mm(t.layout.v_space_mm),
        height: format_mm(t.size.em_mm()),
        welded: TextToggle {
            on: t.layout.welded,
            enabled: welded_ok,
            why_off: (!welded_ok).then_some(TextIssue::WeldedNeedsOutline),
        },
        single_line: t.mode == TextMode::SingleLine,
        missing_glyphs: snapshot.text_layer().missing(id).len(),
        font_missing,
        missing_font: font_missing.then(|| crate::text_panel::MissingFontChoice {
            requested: t.font.family.clone(),
            replacements: installed.into_iter().filter(|f| *f != t.font.family).collect(),
        }),
        can_convert: !s.selection.is_empty(),
        variable: t.variable.is_some(),
        contents: vec![t.content.clone()],
        issue: s.text.issue.filter(|(t, _)| *t == id).map(|(_, i)| i),
        path: t.path.map(|b| {
            let guide = snapshot.object(b.guide);
            let failed = matches!(
                snapshot.derived_record(DerivedOwner::Text(id)).map(|r| &r.status),
                Some(burnburn_document::EvalStatus::Failed { .. })
            );
            TextPathView {
                guide: guide.map(|g| g.name().to_owned()),
                start: format_mm(b.start_mm),
                offset: format_mm(b.offset_mm),
                reverse: b.reverse,
                distort: if b.warp { Distort::Warp } else { Distort::Rigid },
                status: match (guide, failed) {
                    (None, _) => PathBindingStatus::GuideMissing,
                    (Some(_), true) => PathBindingStatus::DoesNotFit,
                    (Some(_), false) => PathBindingStatus::Following,
                },
            }
        }),
    })
}

// ---------------------------------------------------------------------------------------------
// Text on a path
// ---------------------------------------------------------------------------------------------

/// The text and the guide of the selection: exactly one text (with all its outlines) and one
/// other object, whose path is the guide.
fn text_and_guide(s: &Session, snapshot: &DocumentSnapshot) -> Option<(TextId, GeometryId, Arc<Path>)> {
    let mut text = None;
    let mut guide = None;
    for id in s.selection.ids() {
        match text_of(snapshot, *id) {
            Some(t) if text.is_none_or(|f| f == t) => text = Some(t),
            Some(_) => return None,
            None if guide.is_none() => guide = Some((*id, Arc::clone(snapshot.object(*id)?.path()))),
            None => return None,
        }
    }
    let (guide_id, path) = guide?;
    Some((text?, guide_id, path))
}

/// Whether Tekst op pad can act on the selection.
pub(super) fn path_ready(s: &Session) -> bool {
    text_and_guide(s, &s.doc.snapshot()).is_some()
}

pub(super) fn open_path_dialog(s: &mut Session, document: DocumentId) -> AppUpdate {
    if !path_ready(s) {
        return AppUpdate::default();
    }
    s.text.path = Some(PathDialog::default());
    AppUpdate::redraw(document)
}

pub(super) fn path_view(s: &Session) -> Option<TextOnPathViewModel> {
    let d = s.text.path.as_ref()?;
    Some(TextOnPathViewModel {
        distort: d.distort,
        start: d.start.clone(),
        offset: d.offset.clone(),
        reverse: d.reverse,
        live: d.live,
        ready: path_ready(s),
        issue: d.issue,
    })
}

fn path_issue(e: &GenError) -> TextIssue {
    match e {
        GenError::MultiLineText => TextIssue::PathMultiline,
        GenError::TextLongerThanPath { .. } => TextIssue::PathTextTooLong,
        GenError::EmptyGuide => TextIssue::PathEmptyGuide,
        _ => TextIssue::PathFailed,
    }
}

/// One edit of the text-on-path dialog. Apply creates the bent text as an ordinary path object.
pub(super) fn path_edit(
    s: &mut Session,
    document: DocumentId,
    fonts: Option<&SharedFonts>,
    edit: TextPathEdit,
    request: Option<RequestId>,
) -> AppUpdate {
    let Some(dialog) = s.text.path.as_mut() else { return AppUpdate::default() };
    match edit {
        TextPathEdit::Distort(d) => dialog.distort = d,
        TextPathEdit::Start(t) => dialog.start = t,
        TextPathEdit::Offset(t) => dialog.offset = t,
        TextPathEdit::Reverse(r) => dialog.reverse = r,
        TextPathEdit::Live(on) => dialog.live = on,
        TextPathEdit::Cancel => {
            s.text.path = None;
            return AppUpdate::redraw(document);
        }
        TextPathEdit::Apply => return apply_path(s, document, fonts, request),
    }
    dialog.issue = None;
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

fn apply_path(
    s: &mut Session,
    document: DocumentId,
    fonts: Option<&SharedFonts>,
    request: Option<RequestId>,
) -> AppUpdate {
    let Some(dialog) = s.text.path.clone() else { return AppUpdate::default() };
    let refuse = |s: &mut Session, issue: TextIssue| {
        if let Some(d) = s.text.path.as_mut() {
            d.issue = Some(issue);
        }
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    };
    let Some(fonts) = fonts else { return refuse(s, TextIssue::NoFonts) };
    let snapshot = s.doc.snapshot();
    let Some((id, guide_id, guide)) = text_and_guide(s, &snapshot) else {
        return refuse(s, TextIssue::PathNeedsTextAndGuide);
    };
    let Some(source) = snapshot.text(id) else { return refuse(s, TextIssue::PathNeedsTextAndGuide) };
    let start = match number(&dialog.start, TextField::PathStart, true) {
        Ok(v) if v >= 0.0 => v,
        _ => return refuse(s, invalid(TextField::PathStart)),
    };
    let offset = match number(&dialog.offset, TextField::PathOffset, true) {
        Ok(v) => v,
        Err(i) => return refuse(s, i),
    };
    let Some(tolerance) = Length::from_mm(PATH_TEXT_TOLERANCE_MM).ok().and_then(|l| ProductionTolerance::new(l).ok())
    else {
        return refuse(s, TextIssue::PathFailed);
    };
    let params = PathTextParams { distort: dialog.distort, start, offset, reverse: dialog.reverse, tolerance };
    let placed = match text_on_path(source, fonts, &guide, &params) {
        Ok(p) => p,
        Err(e) => return refuse(s, path_issue(&e)),
    };
    let before = s.doc.revision();
    let payload = if dialog.live {
        // The text keeps following the guide: its outlines are the bent text from now on.
        let mut bound = source.clone();
        bound.path = Some(PathBinding {
            guide: guide_id,
            start_mm: start,
            offset_mm: offset,
            reverse: dialog.reverse,
            warp: dialog.distort == Distort::Warp,
        });
        CommandPayload::Text(TextCommand::Set { text: id, source: Box::new(bound) })
    } else {
        let name = format!("Tekst op pad: {}", source.content.lines().next().unwrap_or_default());
        CommandPayload::Edit(GeometryCommand::CreatePath { path: placed.path, name })
    };
    let update = apply_edit(s, document, payload, request);
    if s.doc.revision() == before {
        return refuse(s, TextIssue::Refused);
    }
    s.text.path = None;
    update
}

impl AppController {
    /// Fonts for text input (`UiIntent::InsertText`) and the face new text uses.
    pub fn attach_fonts(&mut self, fonts: Arc<dyn FontSource + Send + Sync>, default_font: FontRef) {
        let shared = SharedFonts(fonts);
        for s in self.sessions.values_mut() {
            s.fonts = Some(shared.clone());
        }
        self.fonts = Some((shared, default_font));
    }
}

impl AppController {
    /// One committed text: extends the selected text object, or creates one at the origin.
    pub(super) fn insert_text(&mut self, document: DocumentId, text: String) -> AppUpdate {
        if text.is_empty() {
            return AppUpdate::default();
        }
        let Some((_, font)) = self.fonts.clone() else { return error(Some(document), MessageKey::NoFontAvailable) };
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| {
            let snapshot = s.doc.snapshot();
            let mut selected: Vec<TextId> = s
                .selection
                .ids()
                .iter()
                .filter_map(|id| match snapshot.object(*id).map(|o| o.source()) {
                    Some(GeometrySource::Derived(k)) => match k.owner {
                        DerivedOwner::Text(t) => Some(t),
                        _ => None,
                    },
                    _ => None,
                })
                .collect();
            selected.sort();
            selected.dedup();
            // Typing extends every selected text (G8-editor, E07): one text is one `Set`, several
            // are one compound and so one undo step.
            let extended = |id: TextId| {
                let mut source = snapshot.text(id)?.clone();
                source.content.push_str(&text);
                Some(TextCommand::Set { text: id, source: Box::new(source) })
            };
            if let [_, _, ..] = selected.as_slice() {
                let Some(steps) = selected
                    .iter()
                    .map(|id| extended(*id).map(|c| CompoundStep::Run(CommandPayload::Text(c))))
                    .collect()
                else {
                    return AppUpdate::default();
                };
                let payload = CommandPayload::Compound(CompoundCommand { label: CompoundLabel::EditTexts, steps });
                return apply_edit(s, document, payload, request);
            }
            let edit = match selected.as_slice() {
                [first] => {
                    let Some(edit) = extended(*first) else { return AppUpdate::default() };
                    edit
                }
                _ => {
                    let Ok(size) = Length::from_mm(DEFAULT_TEXT_MM)
                        .map_err(|_| ())
                        .and_then(|l| FontSize::from_mm(l).map_err(|_| ()))
                    else {
                        return error(Some(document), MessageKey::CommandRejected);
                    };
                    let mut object = TextObject::new(text, font, size);
                    if let Some(at) = s.text_anchor.take()
                        && let Ok(placement) = Affine::translate(Vector { x: at.x, y: at.y })
                    {
                        object.placement = placement;
                    }
                    TextCommand::Create { text: Box::new(object) }
                }
            };
            apply_edit(s, document, CommandPayload::Text(edit), request)
        })
    }

    /// The job clock `{date}` in variable text resolves against (injected, like every clock; `None`
    /// from the closure means the host has no usable time). Without one, variable text with dates
    /// is refused when a job is compiled.
    pub fn attach_clock(&mut self, clock: Arc<dyn Fn() -> Option<burnburn_generators::JobClock> + Send + Sync>) {
        self.clock = Some(clock);
    }

    /// The source of every text object of `document` (tests and accessibility), in id order.
    pub fn text_sources(&self, document: DocumentId) -> Vec<TextObject> {
        self.sessions
            .get(&document)
            .map_or_else(Vec::new, |s| s.doc.snapshot().text_layer().texts().map(|(_, t)| t.clone()).collect())
    }

    /// Content of every text object of `document` (tests and accessibility), in id order.
    pub fn text_contents(&self, document: DocumentId) -> Vec<String> {
        self.sessions
            .get(&document)
            .map_or_else(Vec::new, |s| s.doc.snapshot().text_layer().texts().map(|(_, t)| t.content.clone()).collect())
    }
}

/// The attached font source, shared by every session's text evaluator.
#[derive(Clone)]
pub(super) struct SharedFonts(Arc<dyn FontSource + Send + Sync>);

impl FontSource for SharedFonts {
    fn load(&self, font: &FontRef) -> Result<Arc<FontData>, FontError> {
        self.0.load(font)
    }

    fn available_styles(&self, family: &str) -> Vec<burnburn_text::FontStyle> {
        self.0.available_styles(family)
    }

    fn families(&self) -> Vec<String> {
        self.0.families()
    }
}
