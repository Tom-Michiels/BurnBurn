//! The one tool register (SolidCraft's `CommandSpec`, in BurnBurn form): every tool id with its
//! toolbar panel, icon, shortcut, variant group and what it does. The native toolbars, the
//! tooltips and the shortcut handling all read this register; no host keeps its own list.
//!
//! An id whose domain does not exist yet ([`Action::Unavailable`]) stays in the register but is
//! disabled with a reason: never a button that silently does something else.

use std::collections::BTreeMap;

use burnburn_editor::{PolygonConstruction, ToolKind};

use crate::CommandKey;

/// Toolbar panels, left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PanelId {
    Make,
    Modify,
    /// Selection-wide shape operations (join, cut, round corners, direction, optimize).
    Shapes,
    /// Node editing (delete, break, smooth, line/curve, close, join ends).
    Nodes,
    Pattern,
    Constraints,
    Insert,
    Select,
}

impl PanelId {
    pub const ALL: [PanelId; 8] = [
        PanelId::Make,
        PanelId::Modify,
        PanelId::Shapes,
        PanelId::Nodes,
        PanelId::Pattern,
        PanelId::Constraints,
        PanelId::Insert,
        PanelId::Select,
    ];

    /// Stable key for the panel title in `assets/l10n/tools.toml`.
    pub fn key(self) -> &'static str {
        match self {
            PanelId::Make => "create",
            PanelId::Modify => "modify",
            PanelId::Shapes => "shapes",
            PanelId::Nodes => "nodes",
            PanelId::Pattern => "pattern",
            PanelId::Constraints => "constraints",
            PanelId::Insert => "insert",
            PanelId::Select => "select",
        }
    }
}

/// What a tool needs from its surroundings besides existing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolContext {
    Anywhere,
    /// Only while a sketch is being edited (constraints).
    Sketch,
}

/// What choosing the tool does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Activates this editor tool.
    Tool(ToolKind),
    /// Activates the regular-polygon tool with this construction mode (the SolidCraft modes and
    /// the bounding-box drag are one tool with an editable result).
    Polygon(PolygonConstruction),
    /// Runs this command (for example trim mode, which is a mode and not a `ToolKind`).
    Command(CommandKey),
    /// No domain behind it yet.
    Unavailable,
    /// The tool exists but its result cannot be committed atomically yet (batch command follows).
    Deferred,
}

/// One register entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolSpec {
    pub id: &'static str,
    pub panel: PanelId,
    /// Icon id for `burnburn-icons` (the tool id itself).
    pub icon: &'static str,
    /// Shortcut as shown in tooltips: `R`, `Shift+R`.
    pub shortcut: Option<&'static str>,
    /// Variant group: all specs with the same group share one toolbar button with a drop-down.
    /// The first spec of a group in register order is its default variant.
    pub group: Option<&'static str>,
    pub action: Action,
    pub context: ToolContext,
}

/// Why a tool is disabled; hosts translate it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisabledReason {
    /// "nog niet beschikbaar": the tool has no domain behind it yet.
    NotAvailable,
    /// "volgt": the tool is built but waits for an atomic batch command in the document.
    Follows,
    NeedsSketch,
    NoDocument,
    /// The operator role may not edit.
    OperatorRole,
    /// The command needs a selection it does not have (a path, two shapes, a basic shape).
    NeedsSelection,
    /// The command acts on selected nodes: use the Node tool and select some.
    NeedsNodes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enablement {
    Enabled,
    Disabled(DisabledReason),
}

impl Enablement {
    pub fn is_enabled(self) -> bool {
        self == Enablement::Enabled
    }
}

/// The surroundings a tool's enablement depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolState {
    pub document: bool,
    pub operator: bool,
    pub in_sketch: bool,
}

const fn spec(
    id: &'static str,
    panel: PanelId,
    shortcut: Option<&'static str>,
    group: Option<&'static str>,
    action: Action,
) -> ToolSpec {
    ToolSpec { id, panel, icon: id, shortcut, group, action, context: ToolContext::Anywhere }
}

const fn sketch_only(mut s: ToolSpec) -> ToolSpec {
    s.context = ToolContext::Sketch;
    s
}

use Action::{Command, Polygon, Tool, Unavailable};
use PanelId::{Constraints, Insert, Make, Modify, Nodes, Pattern, Select, Shapes};

static SPECS: &[ToolSpec] = &[
    // Select
    spec("select", Select, Some("V"), None, Tool(ToolKind::Select)),
    spec("node", Select, Some("N"), None, Tool(ToolKind::Node)),
    // Make
    spec("sketch.line", Make, Some("L"), Some("sketch.line"), Tool(ToolKind::Line)),
    spec("sketch.polyline", Make, None, Some("sketch.line"), Tool(ToolKind::Polyline)),
    spec("sketch.line.midpoint", Make, None, Some("sketch.line"), Tool(ToolKind::LineMidpoint)),
    // Disabled until the document has a construction flag / point object (EditCommit has no payload).
    spec("sketch.centerline", Make, None, None, Unavailable),
    spec("sketch.pen", Make, Some("B"), None, Tool(ToolKind::Pen)),
    spec("sketch.rectangle.two_point", Make, Some("R"), Some("sketch.rectangle"), Tool(ToolKind::Rectangle)),
    spec("sketch.rectangle.center", Make, None, Some("sketch.rectangle"), Tool(ToolKind::RectangleCenter)),
    spec("sketch.rectangle.three_point", Make, None, Some("sketch.rectangle"), Tool(ToolKind::RectangleThreePoint)),
    // The rounded rectangle is the basic shape of the same name: two corners in, an editable shape out.
    spec("sketch.rectangle.rounded", Make, None, Some("sketch.rectangle"), Tool(ToolKind::RoundedRectangle)),
    spec("sketch.circle.center", Make, Some("C"), Some("sketch.circle"), Tool(ToolKind::Circle)),
    spec("sketch.circle.two_point", Make, None, Some("sketch.circle"), Tool(ToolKind::CircleTwoPoint)),
    spec("sketch.circle.three_point", Make, None, Some("sketch.circle"), Tool(ToolKind::CircleThreePoint)),
    spec("sketch.circle.two_tangent", Make, None, Some("sketch.circle"), Tool(ToolKind::CircleTwoTangent)),
    spec("sketch.circle.three_tangent", Make, None, Some("sketch.circle"), Tool(ToolKind::CircleThreeTangent)),
    spec("sketch.arc.three_point", Make, Some("A"), Some("sketch.arc"), Tool(ToolKind::ArcThreePoint)),
    spec("sketch.arc.center_point", Make, None, Some("sketch.arc"), Tool(ToolKind::ArcCenterPoint)),
    spec("sketch.arc.tangent", Make, None, Some("sketch.arc"), Tool(ToolKind::ArcTangent)),
    spec("sketch.ellipse", Make, Some("E"), Some("sketch.ellipse"), Tool(ToolKind::EllipseAxes)),
    // The bounding-box ellipse (drag a box) stays reachable as a variant; it has no artwork yet.
    spec("sketch.ellipse.box", Make, None, Some("sketch.ellipse"), Tool(ToolKind::Ellipse)),
    // One polygon tool; the construction mode is the input, the result an editable regular polygon.
    spec("sketch.polygon.inscribed", Make, Some("G"), Some("sketch.polygon"), Polygon(PolygonConstruction::Inscribed)),
    spec(
        "sketch.polygon.circumscribed",
        Make,
        None,
        Some("sketch.polygon"),
        Polygon(PolygonConstruction::Circumscribed),
    ),
    spec("sketch.polygon.edge", Make, None, Some("sketch.polygon"), Polygon(PolygonConstruction::Edge)),
    // Basic shapes with parameters (sides, tips, ratio): the polygon by bounding box and the presets.
    spec("shape.triangle", Make, None, Some("shape"), Tool(ToolKind::Triangle)),
    spec("shape.polygon", Make, None, Some("shape"), Polygon(PolygonConstruction::Box)),
    spec("shape.pentagon", Make, None, Some("shape"), Tool(ToolKind::Pentagon)),
    spec("shape.octagon", Make, None, Some("shape"), Tool(ToolKind::Octagon)),
    spec("shape.star", Make, None, Some("shape"), Tool(ToolKind::Star)),
    spec("shape.double_star", Make, None, Some("shape"), Tool(ToolKind::DoubleStar)),
    spec("sketch.slot.center_to_center", Make, Some("S"), Some("sketch.slot"), Tool(ToolKind::SlotCenterToCenter)),
    spec("sketch.slot.overall", Make, None, Some("sketch.slot"), Tool(ToolKind::SlotOverall)),
    spec("sketch.slot.center_point", Make, None, Some("sketch.slot"), Tool(ToolKind::SlotCenterPoint)),
    spec("sketch.slot.arc_three_point", Make, None, Some("sketch.slot"), Tool(ToolKind::SlotArcThreePoint)),
    spec("sketch.slot.arc_center", Make, None, Some("sketch.slot"), Tool(ToolKind::SlotArcCenter)),
    spec("sketch.spline.fit_point", Make, Some("Y"), Some("sketch.spline"), Tool(ToolKind::SplineFit)),
    spec("sketch.spline.control_point", Make, None, Some("sketch.spline"), Tool(ToolKind::SplineControl)),
    spec("sketch.conic", Make, None, None, Tool(ToolKind::Conic)),
    spec("sketch.point", Make, Some("."), None, Unavailable),
    spec("sketch.text", Make, Some("X"), None, Tool(ToolKind::Text)),
    spec("sketch.dimension", Make, Some("D"), None, Tool(ToolKind::Dimension)),
    // Modify
    spec("sketch.fillet", Modify, Some("F"), None, Tool(ToolKind::Fillet)),
    spec(
        "sketch.chamfer.equal_distance",
        Modify,
        Some("Shift+F"),
        Some("sketch.chamfer"),
        Tool(ToolKind::ChamferEqual),
    ),
    spec("sketch.chamfer.two_distance", Modify, None, Some("sketch.chamfer"), Tool(ToolKind::ChamferTwo)),
    spec("sketch.chamfer.distance_angle", Modify, None, Some("sketch.chamfer"), Tool(ToolKind::ChamferAngle)),
    spec("sketch.trim", Modify, Some("T"), None, Tool(ToolKind::Trim)),
    spec("sketch.extend", Modify, Some("Shift+T"), None, Tool(ToolKind::Extend)),
    spec("sketch.break", Modify, Some("K"), None, Tool(ToolKind::Break)),
    spec("sketch.offset", Modify, Some("O"), None, Tool(ToolKind::Offset)),
    spec("sketch.mirror", Modify, Some("M"), None, Tool(ToolKind::Mirror)),
    spec("sketch.move", Modify, Some("Shift+M"), None, Tool(ToolKind::Move)),
    spec("sketch.scale", Modify, Some("Shift+S"), None, Tool(ToolKind::Scale)),
    sketch_only(spec("sketch.construction", Modify, Some("Q"), None, Tool(ToolKind::Construction))),
    // Shapes: operations on the whole selection or on a clicked corner or node.
    spec("geometry.join", Shapes, None, None, Command(CommandKey::ShapesJoin)),
    spec("geometry.cut", Shapes, None, None, Command(CommandKey::ShapesCut)),
    spec("geometry.round_corners", Shapes, None, Some("geometry.round"), Tool(ToolKind::RoundCorner)),
    spec("geometry.round_corners_all", Shapes, None, Some("geometry.round"), Command(CommandKey::ShapesRoundCorners)),
    spec("geometry.start_point", Shapes, None, None, Tool(ToolKind::StartPoint)),
    spec("geometry.reverse", Shapes, None, Some("geometry.direction"), Command(CommandKey::ShapesReverse)),
    spec("geometry.clockwise", Shapes, None, Some("geometry.direction"), Command(CommandKey::ShapesClockwise)),
    spec(
        "geometry.counter_clockwise",
        Shapes,
        None,
        Some("geometry.direction"),
        Command(CommandKey::ShapesCounterClockwise),
    ),
    spec("geometry.optimize", Shapes, None, None, Command(CommandKey::ShapesOptimize)),
    spec("geometry.convert_to_path", Shapes, None, None, Command(CommandKey::ShapesConvertToPath)),
    // Nodes: act on the nodes selected with the Node tool.
    spec("node.delete", Nodes, None, None, Command(CommandKey::NodeDelete)),
    spec("node.break", Nodes, None, None, Command(CommandKey::NodeBreak)),
    spec("node.smooth", Nodes, None, Some("node.kind"), Command(CommandKey::NodeSmooth)),
    spec("node.symmetric", Nodes, None, Some("node.kind"), Command(CommandKey::NodeSymmetric)),
    spec("node.corner", Nodes, None, Some("node.kind"), Command(CommandKey::NodeCorner)),
    spec("node.to_line", Nodes, None, Some("node.segment"), Command(CommandKey::NodeToLine)),
    spec("node.to_curve", Nodes, None, Some("node.segment"), Command(CommandKey::NodeToCurve)),
    spec("node.close_path", Nodes, None, None, Command(CommandKey::NodeClosePath)),
    spec("node.join_ends", Nodes, None, None, Command(CommandKey::NodeJoinEnds)),
    spec("node.start_point", Nodes, None, None, Command(CommandKey::NodeStartPoint)),
    // Pattern
    spec(
        "sketch.pattern.rectangular",
        Pattern,
        Some("Shift+R"),
        Some("sketch.pattern"),
        Tool(ToolKind::PatternRectangular),
    ),
    spec("sketch.pattern.circular", Pattern, None, Some("sketch.pattern"), Tool(ToolKind::PatternCircular)),
    // Constraints (only while editing a sketch)
    sketch_only(spec("sketch.constraint.coincident", Constraints, None, None, Tool(ToolKind::ConstraintCoincident))),
    sketch_only(spec("sketch.constraint.collinear", Constraints, None, None, Tool(ToolKind::ConstraintCollinear))),
    sketch_only(spec("sketch.constraint.concentric", Constraints, None, None, Tool(ToolKind::ConstraintConcentric))),
    sketch_only(spec("sketch.constraint.equal", Constraints, None, None, Tool(ToolKind::ConstraintEqual))),
    sketch_only(spec("sketch.constraint.fix", Constraints, None, None, Tool(ToolKind::ConstraintFix))),
    sketch_only(spec(
        "sketch.constraint.horizontal_vertical",
        Constraints,
        None,
        None,
        Tool(ToolKind::ConstraintHorizontalVertical),
    )),
    sketch_only(spec("sketch.constraint.midpoint", Constraints, None, None, Tool(ToolKind::ConstraintMidpoint))),
    sketch_only(spec("sketch.constraint.parallel", Constraints, None, None, Tool(ToolKind::ConstraintParallel))),
    sketch_only(spec(
        "sketch.constraint.perpendicular",
        Constraints,
        None,
        None,
        Tool(ToolKind::ConstraintPerpendicular),
    )),
    sketch_only(spec("sketch.constraint.symmetry", Constraints, None, None, Tool(ToolKind::ConstraintSymmetry))),
    sketch_only(spec("sketch.constraint.tangent", Constraints, None, None, Tool(ToolKind::ConstraintTangent))),
    sketch_only(spec("sketch.auto_constrain", Constraints, None, None, Tool(ToolKind::AutoConstrain))),
    // Insert (the import route is a file-menu flow, not a tool)
    spec("sketch.insert_svg", Insert, None, None, Unavailable),
    spec("sketch.insert_dxf", Insert, None, None, Unavailable),
];

/// The register.
#[derive(Debug)]
pub struct ToolCatalog {
    specs: &'static [ToolSpec],
}

impl ToolCatalog {
    /// The application's register.
    pub fn standard() -> &'static ToolCatalog {
        static CATALOG: ToolCatalog = ToolCatalog { specs: SPECS };
        &CATALOG
    }

    /// A register over other specs (tests of the view model rules).
    pub fn from_specs(specs: &'static [ToolSpec]) -> ToolCatalog {
        ToolCatalog { specs }
    }

    pub fn specs(&self) -> &'static [ToolSpec] {
        self.specs
    }

    pub fn get(&self, id: &str) -> Option<&'static ToolSpec> {
        self.specs.iter().find(|s| s.id == id)
    }

    /// The members of one variant group, default variant first.
    pub fn group(&self, group: &str) -> Vec<&'static ToolSpec> {
        self.specs.iter().filter(|s| s.group == Some(group)).collect()
    }

    /// The spec that activates `tool`, if any.
    pub fn for_tool(&self, tool: ToolKind) -> Option<&'static ToolSpec> {
        self.specs.iter().find(|s| s.action == Action::Tool(tool))
    }

    /// The spec whose command is `command`, if any.
    pub fn for_command(&self, command: CommandKey) -> Option<&'static ToolSpec> {
        self.specs.iter().find(|s| s.action == Action::Command(command))
    }
}

/// Whether `spec` can be chosen right now, and if not why not.
pub fn enablement(spec: &ToolSpec, state: ToolState) -> Enablement {
    match spec.action {
        Action::Unavailable => return Enablement::Disabled(DisabledReason::NotAvailable),
        Action::Deferred => return Enablement::Disabled(DisabledReason::Follows),
        Action::Tool(_) | Action::Polygon(_) | Action::Command(_) => {}
    }
    if !state.document {
        return Enablement::Disabled(DisabledReason::NoDocument);
    }
    if state.operator {
        return Enablement::Disabled(DisabledReason::OperatorRole);
    }
    if spec.context == ToolContext::Sketch && !state.in_sketch {
        return Enablement::Disabled(DisabledReason::NeedsSketch);
    }
    Enablement::Enabled
}

/// The last chosen variant per group.
pub type VariantMemory = BTreeMap<&'static str, &'static str>;

/// What the toolbar needs to know to mark the selected tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveTool {
    pub tool: Option<ToolKind>,
    /// Trim mode is on (it is a mode on top of the tool).
    pub trim: bool,
    /// The construction mode of the polygon tool (`Some` while that tool is active).
    pub polygon: Option<PolygonConstruction>,
}

/// One button (or one drop-down entry).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolButton {
    pub id: &'static str,
    pub icon: &'static str,
    pub shortcut: Option<&'static str>,
    pub selected: bool,
    pub enablement: Enablement,
}

/// A button on the toolbar: a single tool, or a variant group with its drop-down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupViewModel {
    pub group: Option<&'static str>,
    /// The face of the button: the selected variant, else the remembered one, else the default.
    pub active: ToolButton,
    /// All variants including the active one; empty for a single tool.
    pub variants: Vec<ToolButton>,
}

impl GroupViewModel {
    pub fn selected(&self) -> bool {
        self.active.selected || self.variants.iter().any(|v| v.selected)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelViewModel {
    pub panel: PanelId,
    pub groups: Vec<GroupViewModel>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolbarViewModel {
    pub panels: Vec<PanelViewModel>,
}

impl ToolbarViewModel {
    pub fn build(
        catalog: &ToolCatalog,
        state: ToolState,
        active: ActiveTool,
        memory: &VariantMemory,
    ) -> ToolbarViewModel {
        let button = |s: &ToolSpec| ToolButton {
            id: s.id,
            icon: s.icon,
            shortcut: s.shortcut,
            selected: match s.action {
                // The legacy trim mode (menu command) is the Trim tool as far as the toolbar shows.
                Action::Tool(t) if active.trim => t == ToolKind::Trim,
                Action::Tool(t) => active.tool == Some(t),
                Action::Polygon(c) => {
                    !active.trim && active.tool == Some(ToolKind::Polygon) && active.polygon == Some(c)
                }
                Action::Command(CommandKey::ToolTrim) => active.trim,
                Action::Command(_) | Action::Unavailable | Action::Deferred => false,
            },
            enablement: enablement(s, state),
        };
        let panels = PanelId::ALL
            .iter()
            .map(|&panel| {
                let mut groups: Vec<GroupViewModel> = Vec::new();
                let mut done: Vec<&str> = Vec::new();
                for s in catalog.specs().iter().filter(|s| s.panel == panel) {
                    let Some(name) = s.group else {
                        groups.push(GroupViewModel { group: None, active: button(s), variants: Vec::new() });
                        continue;
                    };
                    if done.contains(&name) {
                        continue;
                    }
                    done.push(name);
                    let variants: Vec<ToolButton> =
                        catalog.specs().iter().filter(|m| m.group == Some(name)).map(button).collect();
                    let face = variants
                        .iter()
                        .find(|v| v.selected)
                        .or_else(|| variants.iter().find(|v| memory.get(name) == Some(&v.id)))
                        .or_else(|| variants.first())
                        .copied();
                    if let Some(face) = face {
                        groups.push(GroupViewModel { group: Some(name), active: face, variants });
                    }
                }
                PanelViewModel { panel, groups }
            })
            .collect();
        ToolbarViewModel { panels }
    }

    /// Disables the command buttons whose command cannot act on the current selection, with the
    /// reason `why` names (`None`: the command is available). Tools and other states are untouched.
    pub fn refine(&mut self, catalog: &ToolCatalog, why: impl Fn(CommandKey) -> Option<DisabledReason>) {
        let apply = |b: &mut ToolButton| {
            if let (Some(Action::Command(key)), Enablement::Enabled) =
                (catalog.get(b.id).map(|s| s.action), b.enablement)
                && let Some(reason) = why(key)
            {
                b.enablement = Enablement::Disabled(reason);
            }
        };
        for g in self.panels.iter_mut().flat_map(|p| p.groups.iter_mut()) {
            apply(&mut g.active);
            g.variants.iter_mut().for_each(apply);
        }
    }

    /// Every button including drop-down entries, in toolbar order.
    pub fn buttons(&self) -> impl Iterator<Item = &ToolButton> {
        self.panels.iter().flat_map(|p| p.groups.iter()).flat_map(|g| {
            let singles = g.variants.is_empty().then_some(&g.active);
            singles.into_iter().chain(g.variants.iter())
        })
    }

    pub fn button(&self, id: &str) -> Option<&ToolButton> {
        self.buttons().find(|b| b.id == id)
    }

    pub fn group(&self, group: &str) -> Option<&GroupViewModel> {
        self.panels.iter().flat_map(|p| p.groups.iter()).find(|g| g.group == Some(group))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use std::collections::BTreeSet;

    use super::*;

    const OPEN: ToolState = ToolState { document: true, operator: false, in_sketch: false };
    const NONE: ActiveTool = ActiveTool { tool: None, trim: false, polygon: None };

    fn standard() -> &'static ToolCatalog {
        ToolCatalog::standard()
    }

    #[test]
    fn shortcuts_are_unique_and_ids_are_unique() {
        let mut keys = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for s in standard().specs() {
            assert!(ids.insert(s.id), "duplicate id {}", s.id);
            if let Some(k) = s.shortcut {
                assert!(!k.is_empty());
                assert!(keys.insert(k), "shortcut {k} used twice (second: {})", s.id);
            }
        }
    }

    #[test]
    fn a_tool_kind_or_command_is_reachable_through_one_id_only() {
        let mut seen = BTreeSet::new();
        for s in standard().specs() {
            let key = match s.action {
                Action::Tool(t) => format!("{t:?}"),
                Action::Polygon(c) => format!("Polygon/{c:?}"),
                Action::Command(c) => format!("{c:?}"),
                Action::Unavailable | Action::Deferred => continue,
            };
            assert!(seen.insert(key.clone()), "{key} reached through two ids");
        }
    }

    #[test]
    fn variant_groups_stay_within_one_panel_and_have_several_members() {
        let groups: BTreeSet<_> = standard().specs().iter().filter_map(|s| s.group).collect();
        for g in groups {
            let members = standard().group(g);
            assert!(members.len() >= 2, "group {g} has one member");
            assert!(members.iter().all(|m| m.panel == members[0].panel), "group {g} spans panels");
        }
    }

    #[test]
    fn the_rectangle_variants_share_one_group_with_two_point_as_default() {
        let rect = standard().group("sketch.rectangle");
        let ids: Vec<_> = rect.iter().map(|s| s.id).collect();
        assert_eq!(
            ids,
            [
                "sketch.rectangle.two_point",
                "sketch.rectangle.center",
                "sketch.rectangle.three_point",
                "sketch.rectangle.rounded"
            ]
        );
    }

    #[test]
    fn constraints_are_sketch_only() {
        for s in standard().specs().iter().filter(|s| s.panel == PanelId::Constraints) {
            assert_eq!(s.context, ToolContext::Sketch, "{}", s.id);
        }
    }

    #[test]
    fn an_id_without_a_tool_is_disabled_with_a_reason() {
        let spec = standard().get("sketch.insert_svg").expect("registered");
        assert_eq!(spec.action, Action::Unavailable);
        assert_eq!(enablement(spec, OPEN), Enablement::Disabled(DisabledReason::NotAvailable));
    }

    #[test]
    fn the_batch_tools_are_wired_to_their_tools() {
        for (id, tool) in [
            ("sketch.mirror", ToolKind::Mirror),
            ("sketch.pattern.rectangular", ToolKind::PatternRectangular),
            ("sketch.pattern.circular", ToolKind::PatternCircular),
            ("sketch.constraint.collinear", ToolKind::ConstraintCollinear),
            ("sketch.auto_constrain", ToolKind::AutoConstrain),
        ] {
            assert_eq!(standard().get(id).expect("registered").action, Action::Tool(tool), "{id}");
        }
        assert!(standard().specs().iter().all(|s| s.action != Action::Deferred), "nothing waits for a batch command");
    }

    #[test]
    fn one_polygon_tool_has_all_the_constructions_as_variants_of_one_group() {
        let ids: Vec<_> = standard().group("sketch.polygon").iter().map(|s| (s.id, s.action)).collect();
        assert_eq!(
            ids,
            [
                ("sketch.polygon.inscribed", Action::Polygon(PolygonConstruction::Inscribed)),
                ("sketch.polygon.circumscribed", Action::Polygon(PolygonConstruction::Circumscribed)),
                ("sketch.polygon.edge", Action::Polygon(PolygonConstruction::Edge)),
            ]
        );
        assert_eq!(standard().get("shape.polygon").map(|s| s.action), Some(Action::Polygon(PolygonConstruction::Box)));
        assert_eq!(
            standard().get("sketch.rectangle.rounded").map(|s| s.action),
            Some(Action::Tool(ToolKind::RoundedRectangle)),
            "the rounded rectangle is the basic shape"
        );
    }

    #[test]
    fn the_polygon_button_follows_the_construction_mode() {
        let active =
            ActiveTool { tool: Some(ToolKind::Polygon), trim: false, polygon: Some(PolygonConstruction::Edge) };
        let vm = ToolbarViewModel::build(standard(), OPEN, active, &VariantMemory::new());
        let selected: Vec<_> = vm.buttons().filter(|b| b.selected).map(|b| b.id).collect();
        assert_eq!(selected, ["sketch.polygon.edge"]);
        assert_eq!(vm.group("sketch.polygon").expect("group").active.id, "sketch.polygon.edge");
    }

    #[test]
    fn command_buttons_are_refined_with_the_reason_the_selection_does_not_fit() {
        let mut vm = ToolbarViewModel::build(standard(), OPEN, NONE, &VariantMemory::new());
        assert_eq!(vm.button("geometry.join").expect("button").enablement, Enablement::Enabled);
        vm.refine(standard(), |key| (key == CommandKey::ShapesJoin).then_some(DisabledReason::NeedsSelection));
        assert_eq!(
            vm.button("geometry.join").expect("button").enablement,
            Enablement::Disabled(DisabledReason::NeedsSelection)
        );
        assert_eq!(vm.button("geometry.cut").expect("button").enablement, Enablement::Enabled, "others untouched");
        assert_eq!(vm.button("select").expect("button").enablement, Enablement::Enabled, "tools untouched");
    }

    #[test]
    fn an_id_with_a_tool_is_enabled_in_an_open_document() {
        let spec = standard().get("sketch.rectangle.two_point").expect("registered");
        assert_eq!(enablement(spec, OPEN), Enablement::Enabled);
    }

    #[test]
    fn nothing_is_enabled_without_a_document_or_for_the_operator() {
        let spec = standard().get("select").expect("registered");
        assert_eq!(
            enablement(spec, ToolState { document: false, ..OPEN }),
            Enablement::Disabled(DisabledReason::NoDocument)
        );
        assert_eq!(
            enablement(spec, ToolState { operator: true, ..OPEN }),
            Enablement::Disabled(DisabledReason::OperatorRole)
        );
    }

    static CONSTRAINT_WITH_TOOL: &[ToolSpec] = &[sketch_only(spec(
        "sketch.constraint.horizontal_vertical",
        PanelId::Constraints,
        None,
        None,
        Action::Tool(ToolKind::Line),
    ))];

    #[test]
    fn a_sketch_only_tool_needs_a_sketch() {
        let s = &CONSTRAINT_WITH_TOOL[0];
        assert_eq!(enablement(s, OPEN), Enablement::Disabled(DisabledReason::NeedsSketch));
        assert_eq!(enablement(s, ToolState { in_sketch: true, ..OPEN }), Enablement::Enabled);
    }

    #[test]
    fn the_toolbar_lists_every_catalog_id_once_in_panel_order() {
        let vm = ToolbarViewModel::build(standard(), OPEN, NONE, &VariantMemory::new());
        let ids: Vec<_> = vm.buttons().map(|b| b.id).collect();
        let mut expected: Vec<_> = standard().specs().iter().map(|s| s.id).collect();
        expected.sort_by_key(|id| PanelId::ALL.iter().position(|p| *p == standard().get(id).unwrap().panel));
        assert_eq!(ids, expected);
        let panels: Vec<_> = vm.panels.iter().map(|p| p.panel).collect();
        assert_eq!(panels, PanelId::ALL);
    }

    #[test]
    fn a_group_is_one_button_whose_face_is_the_default_variant() {
        let vm = ToolbarViewModel::build(standard(), OPEN, NONE, &VariantMemory::new());
        let g = vm.group("sketch.rectangle").expect("group");
        assert_eq!(g.active.id, "sketch.rectangle.two_point");
        assert_eq!(g.variants.len(), 4);
    }

    #[test]
    fn the_remembered_variant_is_the_face_of_its_group() {
        let mut memory = VariantMemory::new();
        memory.insert("sketch.line", "sketch.polyline");
        let vm = ToolbarViewModel::build(standard(), OPEN, NONE, &memory);
        assert_eq!(vm.group("sketch.line").expect("group").active.id, "sketch.polyline");
        // another group is unaffected
        assert_eq!(vm.group("sketch.rectangle").expect("group").active.id, "sketch.rectangle.two_point");
    }

    #[test]
    fn the_active_tool_is_selected_and_shown_on_its_group_button() {
        let active = ActiveTool { tool: Some(ToolKind::Polyline), trim: false, polygon: None };
        let vm = ToolbarViewModel::build(standard(), OPEN, active, &VariantMemory::new());
        let selected: Vec<_> = vm.buttons().filter(|b| b.selected).map(|b| b.id).collect();
        assert_eq!(selected, ["sketch.polyline"]);
        let g = vm.group("sketch.line").expect("group");
        assert!(g.selected());
        assert_eq!(g.active.id, "sketch.polyline", "the face follows the selected tool");
    }

    #[test]
    fn trim_mode_selects_trim_and_not_the_underlying_tool() {
        let active = ActiveTool { tool: Some(ToolKind::Select), trim: true, polygon: None };
        let vm = ToolbarViewModel::build(standard(), OPEN, active, &VariantMemory::new());
        let selected: Vec<_> = vm.buttons().filter(|b| b.selected).map(|b| b.id).collect();
        assert_eq!(selected, ["sketch.trim"]);
    }

    #[test]
    fn disabled_buttons_carry_their_reason_in_the_view_model() {
        let vm = ToolbarViewModel::build(standard(), OPEN, NONE, &VariantMemory::new());
        assert_eq!(
            vm.button("sketch.insert_dxf").expect("button").enablement,
            Enablement::Disabled(DisabledReason::NotAvailable)
        );
        assert_eq!(vm.button("select").expect("button").enablement, Enablement::Enabled);
    }
}
