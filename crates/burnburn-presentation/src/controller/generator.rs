//! The generator dialog (E10, G6-editor): a finger-jointed box or a living hinge. Every edit
//! re-runs the domain generator on the typed parameters; whatever it refuses is shown at its
//! field, nothing is clamped. Insert is one `GeneratorCommand` (one history entry, outputs in the
//! same transaction); a generated object opens again with its definition's parameters and Apply is
//! `GeneratorCommand::Set` (panels keep their ids). The generator never applies kerf: thickness is
//! the material thickness (fit) and the compiler applies the kerf once.

use std::collections::BTreeMap;
use std::sync::Arc;

use burnburn_document::engine::{EngineRecipe, ParamValue};
use burnburn_document::generator::{GeneratorCommand, GeneratorDefinition, GeneratorId};
use burnburn_document::{CommandPayload, DerivedOwner, GeometrySource};
use burnburn_editor::Overlay;
use burnburn_generators::{
    BoxParams, GENERATOR_ENGINE, GenError, Lid, LivingHinge, PanelRole, finger_box, living_hinge,
};
use burnburn_geometry::{Affine, Path, Vector};
use burnburn_types::{DocumentId, RequestId};

use crate::field::{format_mm, parse_position};
use crate::generator_panel::{
    GeneratorDialogViewModel, GeneratorEdit, GeneratorField, GeneratorFieldRow, GeneratorIssue, GeneratorKind,
    GeneratorPreview, GeneratorProblem,
};

use super::{AppUpdate, Session, apply_edit};

const BOX_FIELDS: [GeneratorField; 6] = [
    GeneratorField::Width,
    GeneratorField::Depth,
    GeneratorField::Height,
    GeneratorField::Thickness,
    GeneratorField::FingerWidth,
    GeneratorField::PanelSpacing,
];
const HINGE_FIELDS: [GeneratorField; 5] = [
    GeneratorField::Width,
    GeneratorField::Height,
    GeneratorField::CutLength,
    GeneratorField::Bridge,
    GeneratorField::ColumnSpacing,
];

const BOX_PANELS: [PanelRole; 6] =
    [PanelRole::Bottom, PanelRole::Top, PanelRole::Front, PanelRole::Back, PanelRole::Left, PanelRole::Right];

fn fields_of(kind: GeneratorKind) -> &'static [GeneratorField] {
    match kind {
        GeneratorKind::FingerBox => &BOX_FIELDS,
        GeneratorKind::LivingHinge => &HINGE_FIELDS,
    }
}

fn default_text(kind: GeneratorKind, field: GeneratorField) -> &'static str {
    match (kind, field) {
        (GeneratorKind::FingerBox, GeneratorField::Width) => "100",
        (GeneratorKind::FingerBox, GeneratorField::Depth) => "60",
        (GeneratorKind::FingerBox, GeneratorField::Height) => "40",
        (_, GeneratorField::Thickness) => "3",
        (_, GeneratorField::FingerWidth) => "10",
        (_, GeneratorField::PanelSpacing) => "5",
        (GeneratorKind::LivingHinge, GeneratorField::Width) => "60",
        (GeneratorKind::LivingHinge, GeneratorField::Height) => "40",
        (_, GeneratorField::CutLength) => "10",
        (_, GeneratorField::Bridge) => "2",
        (_, GeneratorField::ColumnSpacing) => "2",
        _ => "",
    }
}

/// What an existing generated object is being changed from.
#[derive(Debug, Clone)]
struct Editing {
    generator: GeneratorId,
    definition: GeneratorDefinition,
}

/// What the parameters make, in generator-local millimetres.
#[derive(Debug, Clone)]
struct Made {
    /// Output key, display name and outline of each part.
    parts: Vec<(u64, String, Path)>,
    cuts: usize,
}

#[derive(Debug)]
struct Dialog {
    kind: GeneratorKind,
    texts: BTreeMap<GeneratorField, String>,
    lid_open: bool,
    editing: Option<Editing>,
    problems: Vec<GeneratorProblem>,
    made: Option<Made>,
    /// Where new parts go: the middle of the view when the dialog opened.
    placement: Affine,
}

#[derive(Debug, Default)]
pub(super) struct GeneratorState {
    dialog: Option<Dialog>,
}

impl Dialog {
    fn new(kind: GeneratorKind) -> Self {
        let texts = fields_of(kind).iter().map(|f| (*f, default_text(kind, *f).to_owned())).collect();
        let mut d = Dialog {
            kind,
            texts,
            lid_open: false,
            editing: None,
            problems: Vec::new(),
            made: None,
            placement: Affine::IDENTITY,
        };
        d.recompute();
        d
    }

    /// A dialog on the parameters of a stored definition, or `None` when it is not one of ours.
    fn from_definition(generator: GeneratorId, definition: GeneratorDefinition) -> Option<Self> {
        let kind = match definition.generator.as_str() {
            "finger-box" => GeneratorKind::FingerBox,
            "living-hinge" => GeneratorKind::LivingHinge,
            _ => return None,
        };
        let name_of = |f: GeneratorField| match f {
            GeneratorField::Width => "width",
            GeneratorField::Depth => "depth",
            GeneratorField::Height => "height",
            GeneratorField::Thickness => "thickness",
            GeneratorField::FingerWidth => "finger_width",
            GeneratorField::PanelSpacing | GeneratorField::ColumnSpacing => "spacing",
            GeneratorField::CutLength => "cut_length",
            GeneratorField::Bridge => "bridge",
        };
        let mut texts = BTreeMap::new();
        for f in fields_of(kind) {
            let text = match definition.recipe.get(name_of(*f)) {
                Some(ParamValue::Number(n)) => format!("{n}"),
                Some(ParamValue::Integer(i)) => i.to_string(),
                _ => default_text(kind, *f).to_owned(),
            };
            texts.insert(*f, text);
        }
        let lid_open = matches!(definition.recipe.get("lid"), Some(ParamValue::Text(t)) if t == "open");
        let placement = definition.placement;
        let mut d = Dialog {
            kind,
            texts,
            lid_open,
            editing: Some(Editing { generator, definition }),
            problems: Vec::new(),
            made: None,
            placement,
        };
        d.recompute();
        Some(d)
    }

    fn problem(&mut self, field: GeneratorField, issue: GeneratorIssue) {
        self.problems.push(GeneratorProblem { field, issue });
    }

    /// The typed value of `field`, or a problem at it.
    fn number(&mut self, field: GeneratorField) -> Option<f64> {
        let text = self.texts.get(&field).cloned().unwrap_or_default();
        match parse_position(&text) {
            Ok(l) => Some(l.mm()),
            Err(_) => {
                self.problem(field, GeneratorIssue::InvalidNumber);
                None
            }
        }
    }

    fn recompute(&mut self) {
        self.problems.clear();
        self.made = None;
        match self.kind {
            GeneratorKind::FingerBox => self.recompute_box(),
            GeneratorKind::LivingHinge => self.recompute_hinge(),
        }
    }

    fn box_params(&mut self) -> Option<BoxParams> {
        let (w, d, h) = (self.number(GeneratorField::Width), self.number(GeneratorField::Depth), self.number(GeneratorField::Height));
        let (t, f, s) = (
            self.number(GeneratorField::Thickness),
            self.number(GeneratorField::FingerWidth),
            self.number(GeneratorField::PanelSpacing),
        );
        Some(BoxParams {
            width: w?,
            depth: d?,
            height: h?,
            thickness: t?,
            finger_width: f?,
            lid: if self.lid_open { Lid::Open } else { Lid::Closed },
            spacing: s?,
        })
    }

    fn recompute_box(&mut self) {
        let Some(params) = self.box_params() else { return };
        match finger_box(&params) {
            Ok(panels) => {
                let parts = panels
                    .into_iter()
                    .map(|panel| {
                        let local = BOX_PANELS.iter().position(|r| *r == panel.role).unwrap_or(BOX_PANELS.len()) as u64;
                        (local, format!("{:?}", panel.role), panel.path)
                    })
                    .collect();
                self.made = Some(Made { parts, cuts: 0 });
            }
            Err(e) => self.refuse(&e, GeneratorKind::FingerBox),
        }
    }

    fn recompute_hinge(&mut self) {
        let (w, h) = (self.number(GeneratorField::Width), self.number(GeneratorField::Height));
        let (c, b, s) = (
            self.number(GeneratorField::CutLength),
            self.number(GeneratorField::Bridge),
            self.number(GeneratorField::ColumnSpacing),
        );
        let (Some(width), Some(height), Some(cut_length), Some(bridge), Some(spacing)) = (w, h, c, b, s) else {
            return;
        };
        match living_hinge(&LivingHinge { width, height, cut_length, bridge, spacing }) {
            Ok(cuts) => {
                let subpaths = cuts.iter().flat_map(|c| c.subpaths().iter().cloned()).collect();
                self.made = Some(Made { parts: vec![(0, String::new(), Path::from_subpaths(subpaths))], cuts: cuts.len() });
            }
            Err(e) => self.refuse(&e, GeneratorKind::LivingHinge),
        }
    }

    fn refuse(&mut self, e: &GenError, kind: GeneratorKind) {
        let field_of = |param: &str| match (kind, param) {
            (_, "width") => GeneratorField::Width,
            (_, "depth") => GeneratorField::Depth,
            (_, "height") => GeneratorField::Height,
            (_, "thickness") => GeneratorField::Thickness,
            (_, "finger_width") => GeneratorField::FingerWidth,
            (GeneratorKind::FingerBox, "spacing") => GeneratorField::PanelSpacing,
            (_, "spacing") => GeneratorField::ColumnSpacing,
            (_, "cut_length") => GeneratorField::CutLength,
            (_, "bridge") => GeneratorField::Bridge,
            _ => GeneratorField::Width,
        };
        let (field, issue) = match e {
            GenError::NotFinite { param } => (field_of(param), GeneratorIssue::InvalidNumber),
            GenError::NotPositive { param } => (field_of(param), GeneratorIssue::NotPositive),
            GenError::Negative { param } => (field_of(param), GeneratorIssue::Negative),
            GenError::TooSmall { param, min_mm, value_mm } => {
                (field_of(param), GeneratorIssue::TooSmall { min_mm: *min_mm, value_mm: *value_mm })
            }
            GenError::Count { param, min, max, value } => {
                (field_of(param), GeneratorIssue::Count { min: *min, max: *max, value: *value })
            }
            _ => (GeneratorField::Width, GeneratorIssue::Engine),
        };
        self.problem(field, issue);
    }

    fn bounds_centre(&self) -> Option<(f64, f64)> {
        let made = self.made.as_ref()?;
        let mut union: Option<(f64, f64, f64, f64)> = None;
        for (_, _, path) in &made.parts {
            let b = path.bounds()?;
            let (x0, y0, x1, y1) = (b.min().x, b.min().y, b.max().x, b.max().y);
            union = Some(match union {
                None => (x0, y0, x1, y1),
                Some((a, c, d, e)) => (a.min(x0), c.min(y0), d.max(x1), e.max(y1)),
            });
        }
        union.map(|(x0, y0, x1, y1)| ((x0 + x1) / 2.0, (y0 + y1) / 2.0))
    }

    /// The definition the typed parameters make.
    fn definition(&mut self, name: String) -> Option<GeneratorDefinition> {
        let recipe = EngineRecipe::new(GENERATOR_ENGINE);
        let n = |d: &mut Dialog, f| d.number(f).map(ParamValue::Number);
        let (generator, recipe) = match self.kind {
            GeneratorKind::FingerBox => (
                "finger-box",
                recipe
                    .with("width", n(self, GeneratorField::Width)?)
                    .with("depth", n(self, GeneratorField::Depth)?)
                    .with("height", n(self, GeneratorField::Height)?)
                    .with("thickness", n(self, GeneratorField::Thickness)?)
                    .with("finger_width", n(self, GeneratorField::FingerWidth)?)
                    .with("lid", ParamValue::Text(if self.lid_open { "open" } else { "closed" }.into()))
                    .with("spacing", n(self, GeneratorField::PanelSpacing)?),
            ),
            GeneratorKind::LivingHinge => (
                "living-hinge",
                recipe
                    .with("width", n(self, GeneratorField::Width)?)
                    .with("height", n(self, GeneratorField::Height)?)
                    .with("cut_length", n(self, GeneratorField::CutLength)?)
                    .with("bridge", n(self, GeneratorField::Bridge)?)
                    .with("spacing", n(self, GeneratorField::ColumnSpacing)?),
            ),
        };
        Some(GeneratorDefinition { name, generator: generator.into(), recipe, placement: self.placement })
    }
}

/// The only generator a selection stands for, when every selected object is an output of it.
pub(super) fn selected_generator(s: &Session) -> Option<(GeneratorId, GeneratorDefinition)> {
    let snapshot = s.doc.snapshot();
    let mut found: Option<GeneratorId> = None;
    for id in s.selection.ids() {
        let object = snapshot.object(*id)?;
        let GeometrySource::Derived(key) = object.source() else { return None };
        let DerivedOwner::Generator(g) = key.owner else { return None };
        if found.is_some_and(|f| f != g) {
            return None;
        }
        found = Some(g);
    }
    let g = found?;
    Some((g, snapshot.generator(g)?.clone()))
}

/// A rigid transform (move, turn, mirror) of every output of one generator (and nothing else) is a
/// change of that generator's placement (C-8: derived geometry is edited through its definition):
/// the whole box, hinge or barcode moves as one object in one history entry. Scaling or shearing
/// would make the size differ from its parameters (a barcode's module, a box's millimetres), a
/// transform of some of the outputs, or of outputs mixed with other objects: those stay what they
/// were and the document refuses them.
pub(super) fn retarget(snapshot: &burnburn_document::DocumentSnapshot, payload: CommandPayload) -> CommandPayload {
    let CommandPayload::Transform { targets, transform } = &payload else { return payload };
    if !is_rigid(transform) {
        return payload;
    }
    let mut owner: Option<GeneratorId> = None;
    for id in targets {
        let Some(GeometrySource::Derived(key)) = snapshot.object(*id).map(|o| o.source()) else { return payload };
        let DerivedOwner::Generator(g) = key.owner else { return payload };
        if owner.is_some_and(|o| o != g) {
            return payload;
        }
        owner = Some(g);
    }
    let Some(g) = owner else { return payload };
    let all_outputs = snapshot.objects().all(|(id, o)| match o.source() {
        GeometrySource::Derived(k) if k.owner == DerivedOwner::Generator(g) => targets.contains(&id),
        _ => true,
    });
    let Some(definition) = snapshot.generator(g).filter(|_| all_outputs) else { return payload };
    let mut moved = definition.clone();
    moved.placement = definition.placement.then(transform);
    CommandPayload::Generator(GeneratorCommand::Set { generator: g, definition: Box::new(moved) })
}

/// Orthonormal columns: lengths and angles are kept (a turn, a mirror, a move).
fn is_rigid(m: &Affine) -> bool {
    const EPS: f64 = 1e-9;
    let [a, b, c, d, _, _] = m.coefficients();
    (a * a + b * b - 1.0).abs() < EPS && (c * c + d * d - 1.0).abs() < EPS && (a * c + b * d).abs() < EPS
}

/// "Bewerk generator" applies to a selected box, hinge or barcode.
pub(super) fn edit_enabled(s: &Session) -> bool {
    selected_generator(s).is_some_and(|(_, def)| matches!(def.generator.as_str(), "finger-box" | "living-hinge" | "symbology"))
}

pub(super) fn open(s: &mut Session, document: DocumentId) -> AppUpdate {
    let mut d = Dialog::new(GeneratorKind::FingerBox);
    d.placement = view_placement(s, &d);
    s.generator.dialog = Some(d);
    AppUpdate::redraw(document)
}

/// Opens the dialog of the selected generated object.
pub(super) fn open_selected(s: &mut Session, document: DocumentId) -> AppUpdate {
    let Some((g, definition)) = selected_generator(s) else { return AppUpdate::default() };
    if definition.generator == "symbology" {
        return super::barcode::open_edit(s, document, g, definition);
    }
    match Dialog::from_definition(g, definition) {
        Some(d) => {
            s.generator.dialog = Some(d);
            AppUpdate::redraw(document)
        }
        None => AppUpdate::default(),
    }
}

/// Moves the middle of what the dialog makes to the middle of the view.
fn view_placement(s: &Session, d: &Dialog) -> Affine {
    let centre = s.camera.center_mm();
    let Some((cx, cy)) = d.bounds_centre() else { return Affine::IDENTITY };
    Affine::translate(Vector { x: centre.x - cx, y: centre.y - cy }).unwrap_or(Affine::IDENTITY)
}

fn changed() -> AppUpdate {
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

pub(super) fn edit(s: &mut Session, document: DocumentId, edit: GeneratorEdit, request: Option<RequestId>) -> AppUpdate {
    if s.generator.dialog.is_none() {
        return AppUpdate::default();
    }
    match edit {
        GeneratorEdit::Cancel => {
            s.generator.dialog = None;
            return AppUpdate::redraw(document);
        }
        GeneratorEdit::Insert => return insert(s, document, request),
        GeneratorEdit::Kind(kind) => {
            let placement = view_placement(s, &Dialog::new(kind));
            let mut d = Dialog::new(kind);
            d.placement = placement;
            s.generator.dialog = Some(d);
            return changed();
        }
        GeneratorEdit::Field(field, text) => {
            if let Some(d) = s.generator.dialog.as_mut()
                && fields_of(d.kind).contains(&field)
            {
                d.texts.insert(field, text);
                d.recompute();
            }
        }
        GeneratorEdit::LidOpen(open) => {
            if let Some(d) = s.generator.dialog.as_mut()
                && d.kind == GeneratorKind::FingerBox
            {
                d.lid_open = open;
                d.recompute();
            }
        }
    }
    changed()
}

fn insert(s: &mut Session, document: DocumentId, request: Option<RequestId>) -> AppUpdate {
    let Some(d) = s.generator.dialog.as_mut() else { return AppUpdate::default() };
    if d.made.is_none() {
        return AppUpdate::default();
    }
    let name = match &d.editing {
        Some(e) => e.definition.name.clone(),
        None => match d.kind {
            GeneratorKind::FingerBox => {
                let size = |f| d.texts.get(&f).cloned().unwrap_or_default();
                format!(
                    "Doos {} x {} x {}",
                    size(GeneratorField::Width),
                    size(GeneratorField::Depth),
                    size(GeneratorField::Height)
                )
            }
            GeneratorKind::LivingHinge => "Scharnier".to_owned(),
        },
    };
    let Some(definition) = d.definition(name) else { return AppUpdate::default() };
    let command = match &d.editing {
        Some(e) if e.definition == definition => {
            // Nothing differs from what the object already is: no history entry.
            s.generator.dialog = None;
            return AppUpdate::redraw(document);
        }
        Some(e) => GeneratorCommand::Set { generator: e.generator, definition: Box::new(definition) },
        None => GeneratorCommand::Create { definition: Box::new(definition) },
    };
    let before = s.doc.revision();
    let update = apply_edit(s, document, CommandPayload::Generator(command), request);
    if s.doc.revision() != before {
        s.generator.dialog = None;
    }
    update
}

pub(super) fn dialog_view(s: &Session) -> Option<GeneratorDialogViewModel> {
    let d = s.generator.dialog.as_ref()?;
    let preview = d.made.as_ref().map(|made| {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for (_, _, path) in &made.parts {
            if let Some(b) = path.bounds() {
                (x0, y0, x1, y1) = (x0.min(b.min().x), y0.min(b.min().y), x1.max(b.max().x), y1.max(b.max().y));
            }
        }
        GeneratorPreview {
            parts: made.parts.len(),
            cuts: made.cuts,
            width: format_mm(x1 - x0),
            height: format_mm(y1 - y0),
        }
    });
    Some(GeneratorDialogViewModel {
        kind: d.kind,
        editing: d.editing.is_some(),
        fields: fields_of(d.kind)
            .iter()
            .map(|f| GeneratorFieldRow { field: *f, text: d.texts.get(f).cloned().unwrap_or_default() })
            .collect(),
        lid_open: (d.kind == GeneratorKind::FingerBox).then_some(d.lid_open),
        problems: d.problems.clone(),
        preview,
        can_insert: d.made.is_some(),
    })
}

/// The parts the dialog would make, drawn where they would go.
pub(super) fn scene_overlays(s: &Session) -> Vec<Overlay> {
    let Some(d) = s.generator.dialog.as_ref() else { return Vec::new() };
    let Some(made) = &d.made else { return Vec::new() };
    made.parts
        .iter()
        .map(|(_, _, path)| Overlay::DragPreview { path: Arc::new(path.clone()), transform: d.placement })
        .collect()
}
