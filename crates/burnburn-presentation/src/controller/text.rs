//! Text input: fonts, `InsertText` and the shared font source of every session's evaluator.

use burnburn_geometry::{Affine, Path, ProductionTolerance, Vector};
use std::sync::Arc;

use burnburn_document::text::{TextCommand, TextId, TextMode, TextObject};
use burnburn_document::{CommandPayload, DerivedOwner, DocumentSnapshot, GeometryCommand, GeometrySource};
use burnburn_generators::{GenError, PathTextParams, text_on_path};
use burnburn_types::{DocumentId, Length, RequestId};

use burnburn_text::{FontData, FontError, FontRef, FontSize, FontSource, FontStyle, switch_style};

use crate::field::{FieldError, format_mm, parse_length, parse_position};
use crate::text_panel::{
    Distort, TextEdit, TextField, TextIssue, TextOnPathViewModel, TextPathEdit, TextToggle, TextToolbarViewModel,
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
    issue: Option<TextIssue>,
}

impl Default for PathDialog {
    fn default() -> Self {
        PathDialog { distort: Distort::Rigid, start: "0".into(), offset: "0".into(), reverse: false, issue: None }
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

fn invalid(field: TextField) -> TextIssue {
    TextIssue::InvalidNumber(field)
}

fn number(text: &str, field: TextField, signed: bool) -> Result<f64, TextIssue> {
    let parsed: Result<Length, FieldError> = if signed { parse_position(text) } else { parse_length(text) };
    parsed.map(Length::mm).map_err(|_| invalid(field))
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

/// One toolbar edit of the selected text: one `TextCommand::Set`, or a reason in the toolbar.
pub(super) fn edit(
    s: &mut Session,
    document: DocumentId,
    fonts: Option<&SharedFonts>,
    edit: TextEdit,
    request: Option<RequestId>,
) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let Some(id) = single_text(s, &snapshot) else { return AppUpdate::default() };
    let Some(current) = snapshot.text(id) else { return AppUpdate::default() };
    let mut next = current.clone();
    if let Err(issue) = apply(&mut next, fonts, edit) {
        s.text.issue = Some((id, issue));
        return AppUpdate { view_changed: true, ..AppUpdate::default() };
    }
    let had_issue = s.text.issue.take().is_some();
    if next == *current {
        return AppUpdate { view_changed: had_issue, ..AppUpdate::default() };
    }
    apply_edit(s, document, CommandPayload::Text(TextCommand::Set { text: id, source: Box::new(next) }), request)
}

/// Whether Convert to Path can act: one text with outlines is selected.
pub(super) fn convertible(s: &Session) -> bool {
    let snapshot = s.doc.snapshot();
    single_text(s, &snapshot).is_some_and(|id| !s.selection.is_empty() && snapshot.text(id).is_some())
}

/// Convert to Path of the selected text: its outlines become ordinary objects (one undo step).
pub(super) fn convert(s: &mut Session, document: DocumentId, request: Option<RequestId>) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let Some(id) = single_text(s, &snapshot) else { return AppUpdate::default() };
    apply_edit(s, document, CommandPayload::Text(TextCommand::ConvertToPath { text: id }), request)
}

pub(super) fn toolbar_view(s: &Session) -> Option<TextToolbarViewModel> {
    let snapshot = s.doc.snapshot();
    let id = single_text(s, &snapshot)?;
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
        issue: s.text.issue.filter(|(t, _)| *t == id).map(|(_, i)| i),
    })
}

// ---------------------------------------------------------------------------------------------
// Text on a path
// ---------------------------------------------------------------------------------------------

/// The text and the guide of the selection: exactly one text (with all its outlines) and one
/// other object, whose path is the guide.
fn text_and_guide(s: &Session, snapshot: &DocumentSnapshot) -> Option<(TextId, Arc<Path>)> {
    let mut text = None;
    let mut guide = None;
    for id in s.selection.ids() {
        match text_of(snapshot, *id) {
            Some(t) if text.is_none_or(|f| f == t) => text = Some(t),
            Some(_) => return None,
            None if guide.is_none() => guide = Some(Arc::clone(snapshot.object(*id)?.path())),
            None => return None,
        }
    }
    Some((text?, guide?))
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
    let Some((id, guide)) = text_and_guide(s, &snapshot) else { return refuse(s, TextIssue::PathNeedsTextAndGuide) };
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
    let name = format!("Tekst op pad: {}", source.content.lines().next().unwrap_or_default());
    let before = s.doc.revision();
    let update =
        apply_edit(s, document, CommandPayload::Edit(GeometryCommand::CreatePath { path: placed.path, name }), request);
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
            let selected: Vec<TextId> = s
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
            let edit = match selected.as_slice() {
                [first, rest @ ..] if rest.iter().all(|t| t == first) => {
                    let Some(current) = snapshot.text(*first) else { return AppUpdate::default() };
                    let mut source = current.clone();
                    source.content.push_str(&text);
                    TextCommand::Set { text: *first, source: Box::new(source) }
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
