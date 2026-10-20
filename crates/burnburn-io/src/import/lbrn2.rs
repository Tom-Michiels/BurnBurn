//! LightBurn `.lbrn2` import (IM28, P04): an explicit, published subset.
//!
//! Supported (see [`SUPPORTED`]): the project root, cut settings (`CutSetting`,
//! `CutSetting_Img`) with `index`, `name`, `minPower`, `maxPower`, `speed`, `numPasses`,
//! `interval`, and shapes `Rect` (with corner radius), `Ellipse`, `Path` (`VertList`/`PrimList`
//! with lines and cubic Béziers, shared lists via `VertID`/`PrimID`) and `Group`, each with
//! `XForm`. `Text` and `Bitmap` are recognised and reported (font, embedded/linked asset); text
//! is imported as outlines only when the file carries a `BackupPath`. Every other element,
//! attribute and setting is named in the report with its count: nothing is dropped silently.
//! Cut settings become *proposals* (output disabled) — power scales differ per machine. W2-L6
//! adds, without applying any of it: the values of unmapped layer settings
//! ([`CutSettingProposal::unmapped`]), project information ([`ProjectInfo`]: device, mirroring,
//! notes, variable-text and optimisation preferences as written), embedded bitmaps decoded and
//! framed ([`AssetNote`]) and image cut settings ([`ImageProposal`], [`Lbrn2Draft::image_operation`]).

use std::collections::BTreeMap;

use burnburn_document::process::{
    DitherMode, FillSettings, ImageRender, ImageSettings, Kerf, OperationDraft, OperationMode, Passes, PowerPercent,
    PowerRange, ProcessCommand, ProcessSettings, ScanAxis, ScanDirection, Speed,
};
use burnburn_document::{GeometryCommand, ImportItem};
use burnburn_geometry::{Affine, EllipticalArc, Path, PathBuilder, Point, Segment, Shape, Subpath};
use burnburn_types::{Angle, ContentHash, GeometryId, Length};

use super::xml::{Event, Tokenizer};
use super::{ImportError, ParseError, ParseLimits, RasterFormat, RasterLimits, check_size, inspect_raster};

/// Supported elements and, per element, the attributes this importer interprets.
pub const SUPPORTED: &[(&str, &[&str])] = &[
    ("LightBurnProject", &["AppVersion", "FormatVersion"]),
    ("CutSetting", &["type"]),
    ("CutSetting_Img", &["type"]),
    ("index", &["Value"]),
    ("name", &["Value"]),
    ("minPower", &["Value"]),
    ("maxPower", &["Value"]),
    ("speed", &["Value"]),
    ("numPasses", &["Value"]),
    ("interval", &["Value"]),
    ("ditherMode", &["Value"]),
    ("Shape", &["Type", "CutIndex", "W", "H", "Cr", "Rx", "Ry", "VertID", "PrimID", "Font", "Str", "File", "Data"]),
    ("XForm", &[]),
    ("VertList", &[]),
    ("PrimList", &[]),
    ("Children", &[]),
    ("BackupPath", &["Type", "VertID", "PrimID"]),
];

/// The interpreted attributes of a supported element (`None`: element not supported).
pub fn supported_attributes(element: &str) -> Option<&'static [&'static str]> {
    SUPPORTED.iter().find(|(e, _)| *e == element).map(|(_, a)| *a)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum NoteKind {
    /// An element outside [`SUPPORTED`] (its whole subtree is not imported).
    UnknownElement,
    /// `Element@Attribute` not interpreted by this importer.
    UnknownAttribute,
    /// A cut-setting child we do not map (the proposal is made without it).
    UnmappedSetting,
    /// A shape `Type` we do not import.
    UnsupportedShape,
    /// Recognised but not converted (text without outlines, bitmaps).
    NotConverted,
    /// A value outside what we accept (malformed number, unknown primitive, …).
    InvalidValue,
}

/// One reported item, aggregated by kind and name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportNote {
    pub kind: NoteKind,
    pub name: String,
    pub count: u32,
}

/// An assumption the importer had to make (shown with the proposal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assumption {
    pub field: &'static str,
    pub value: String,
    pub context: String,
}

/// A font used by text shapes; whether outlines were available in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontUse {
    pub family: String,
    pub texts: u32,
    pub with_outlines: u32,
}

/// A bitmap shape and where its pixels are (not imported as an object here; handed over for the
/// image step with its placement).
#[derive(Debug, Clone, PartialEq)]
pub struct AssetNote {
    pub entity: String,
    pub linked_file: Option<String>,
    /// Length of the embedded base64 data (0: none).
    pub embedded_chars: usize,
    /// The embedded PNG/JPEG bytes when the base64 data decodes to a readable image.
    pub data: Option<EmbeddedBitmap>,
    /// `W` × `H` of the bitmap shape in mm.
    pub size_mm: Option<(f64, f64)>,
    /// The bitmap's frame in world mm (its size placed by the shape and group transforms).
    pub placement: Option<Path>,
    /// Image adjustments stored on the shape (`Gamma`, `Contrast`, …) as written: carried, not applied.
    pub adjustments: Vec<(String, String)>,
}

/// Decoded pixels of a `Bitmap` shape: the original file bytes, verified by their magic number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedBitmap {
    pub bytes: Vec<u8>,
    pub format: RasterFormat,
}

/// A cut-setting child the importer does not map, with the value it had in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmappedValue {
    pub name: String,
    pub value: Option<String>,
}

/// Project-level data of the file, carried as information. None of it is applied to the
/// document (each item is also named in the report notes): the device, origin mirroring and
/// optimisation preferences belong to the machine and job review.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProjectInfo {
    pub device_name: Option<String>,
    pub material_height_mm: Option<f64>,
    pub mirror_x: Option<bool>,
    pub mirror_y: Option<bool>,
    pub notes: Option<ProjectNotes>,
    /// `VariableText` children (`Start`, `End`, `Current`, `Increment`, `AutoAdvance`) as written.
    pub variable_text: Vec<(String, String)>,
    /// `UIPrefs` children (`Optimize_*`) as written.
    pub optimize: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectNotes {
    pub text: String,
    pub show_on_load: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutKind {
    Line,
    Fill,
    Image,
}

/// A reviewable operation proposal from one cut setting.
#[derive(Debug, Clone, PartialEq)]
pub struct CutSettingProposal {
    pub index: u32,
    pub kind: CutKind,
    /// `None` when the mode cannot be mapped (image): the shapes still import.
    pub draft: Option<OperationDraft>,
    pub assumptions: Vec<Assumption>,
    /// Indices into [`Lbrn2Draft::items`] that use this setting.
    pub items: Vec<usize>,
    /// Setting children that were not mapped, with their values (`priority`, `dotTime`, …).
    pub unmapped: Vec<UnmappedValue>,
    /// The image-mode part of an image cut setting; see [`Lbrn2Draft::image_operation`].
    pub image: Option<ImageProposal>,
}

/// An image cut setting translated as far as the process model goes. Needs the bitmap's asset
/// hash before it becomes an operation.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageProposal {
    pub name: String,
    /// `ditherMode` as written in the file.
    pub dither_name: Option<String>,
    /// The render mode it maps to (`None`: unknown or unsupported name, listed in the assumptions).
    pub render: Option<ImageRender>,
    pub interval_mm: f64,
    pub speed_mm_s: f64,
    pub min_power: f64,
    pub max_power: f64,
    pub passes: u32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Lbrn2Report {
    pub app_version: Option<String>,
    pub notes: Vec<ImportNote>,
    pub fonts: Vec<FontUse>,
    pub assets: Vec<AssetNote>,
    pub project: ProjectInfo,
}

/// One imported shape in world mm.
#[derive(Debug, Clone, PartialEq)]
pub struct Lbrn2Item {
    pub name: String,
    /// Source reference, for example `Shape#3` or `Shape#1/Shape#2` inside a group.
    pub entity: String,
    pub cut_index: Option<u32>,
    pub path: Path,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lbrn2Draft {
    pub items: Vec<Lbrn2Item>,
    pub cut_settings: Vec<CutSettingProposal>,
    pub report: Lbrn2Report,
}

/// Fuzz entry point with default limits.
pub fn parse_lbrn2(bytes: &[u8]) -> Result<Lbrn2Draft, ParseError> {
    parse_lbrn2_with(bytes, &ParseLimits::default())
}

/// Parses LBRN2 bytes within `limits`. Never panics.
pub fn parse_lbrn2_with(bytes: &[u8], limits: &ParseLimits) -> Result<Lbrn2Draft, ParseError> {
    check_size(bytes, limits)?;
    let text = std::str::from_utf8(bytes).map_err(|_| ParseError::NotUtf8)?;
    let root = build_tree(text, limits)?;
    if root.name != "LightBurnProject" {
        return Err(ParseError::Unsupported("not a LightBurn project (root is not LightBurnProject)"));
    }
    let mut cx = Ctx {
        limits,
        notes: BTreeMap::new(),
        items: Vec::new(),
        settings: BTreeMap::new(),
        fonts: BTreeMap::new(),
        assets: Vec::new(),
        verts: BTreeMap::new(),
        prims: BTreeMap::new(),
        vertices: 0,
        app_version: None,
        project: ProjectInfo::default(),
    };
    cx.check_attrs(&root);
    cx.app_version = root.attr("AppVersion").map(str::to_owned);
    cx.project.device_name = root.attr("DeviceName").map(str::to_owned);
    cx.project.material_height_mm =
        root.attr("MaterialHeight").and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite());
    cx.project.mirror_x = root.attr("MirrorX").and_then(parse_bool);
    cx.project.mirror_y = root.attr("MirrorY").and_then(parse_bool);
    if let Some(v) = root.attr("FormatVersion")
        && v != "1"
    {
        cx.note(NoteKind::InvalidValue, format!("LightBurnProject@FormatVersion={v}"));
    }
    let mut shape_no = 0usize;
    for child in &root.children {
        match child.name.as_str() {
            "CutSetting" | "CutSetting_Img" => cx.cut_setting(child),
            "Shape" => {
                shape_no += 1;
                cx.shape(child, Affine::IDENTITY, &format!("Shape#{shape_no}"))?;
            }
            other => {
                // Carried as information; still named in the report because nothing is applied.
                match other {
                    "UIPrefs" => cx.project.optimize = value_children(child),
                    "VariableText" => cx.project.variable_text = value_children(child),
                    "Notes" => {
                        cx.project.notes = Some(ProjectNotes {
                            text: child.attr("Notes").unwrap_or("").to_owned(),
                            show_on_load: child.attr("ShowOnLoad").and_then(parse_bool).unwrap_or(false),
                        });
                    }
                    _ => {}
                }
                cx.unknown(child);
            }
        }
    }
    Ok(cx.finish())
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.trim() {
        "True" | "true" | "1" | "-1" => Some(true),
        "False" | "false" | "0" => Some(false),
        _ => None,
    }
}

/// `(child name, Value)` for each child that has a `Value` attribute, in file order.
fn value_children(node: &Node) -> Vec<(String, String)> {
    node.children.iter().filter_map(|c| Some((c.name.clone(), c.attr("Value")?.to_owned()))).collect()
}

/// Standard base64 (RFC 4648), whitespace ignored, padding optional.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    let mut padding = false;
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                padding = true;
                continue;
            }
            c if c.is_ascii_whitespace() => continue,
            _ => return None,
        };
        if padding {
            return None;
        }
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    (!out.is_empty()).then_some(out)
}

/// A parsed element: name, attributes, character data, children.
#[derive(Debug, Default)]
struct Node {
    name: String,
    attrs: Vec<(String, String)>,
    text: String,
    children: Vec<Node>,
}

impl Node {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }
}

/// Builds the element tree within the entity and depth limits.
fn build_tree(text: &str, limits: &ParseLimits) -> Result<Node, ParseError> {
    let mut tok = Tokenizer::with_text(text);
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;
    let mut elements = 0usize;
    while let Some(ev) = tok.next_event()? {
        match ev {
            Event::Start { name, attrs, self_closing } => {
                elements += 1;
                if elements > limits.max_entities {
                    return Err(ParseError::TooManyEntities { limit: limits.max_entities });
                }
                if stack.len() >= limits.max_depth {
                    return Err(ParseError::TooDeep { limit: limits.max_depth });
                }
                if root.is_some() {
                    return Err(ParseError::Malformed { at: 0, reason: "content after the root element" });
                }
                let node = Node {
                    name: name.to_owned(),
                    attrs: attrs.into_iter().map(|a| (a.name.to_owned(), a.value)).collect(),
                    ..Node::default()
                };
                if self_closing {
                    close(&mut stack, &mut root, node);
                } else {
                    stack.push(node);
                }
            }
            Event::End { name } => {
                let node = stack.pop().ok_or(ParseError::Malformed { at: 0, reason: "unmatched end tag" })?;
                if node.name != name {
                    return Err(ParseError::Malformed { at: 0, reason: "end tag does not match the open element" });
                }
                close(&mut stack, &mut root, node);
            }
            Event::Text { text } => match stack.last_mut() {
                Some(top) => top.text.push_str(&text),
                None => return Err(ParseError::Malformed { at: 0, reason: "text outside the root element" }),
            },
        }
    }
    if !stack.is_empty() {
        return Err(ParseError::Malformed { at: text.len(), reason: "unterminated element" });
    }
    root.ok_or(ParseError::Malformed { at: text.len(), reason: "empty document" })
}

fn close(stack: &mut [Node], root: &mut Option<Node>, node: Node) {
    match stack.last_mut() {
        Some(parent) => parent.children.push(node),
        None => *root = Some(node),
    }
}

#[derive(Debug, Clone, Copy)]
struct Vertex {
    at: Point,
    c0: Option<Point>,
    c1: Option<Point>,
}

#[derive(Debug, Clone, Copy)]
enum Prim {
    Line(usize, usize),
    Bezier(usize, usize),
}

#[derive(Debug, Default)]
struct SettingAcc {
    kind: Option<CutKind>,
    values: BTreeMap<&'static str, String>,
    /// Children not mapped, with their `Value`, in file order.
    others: Vec<UnmappedValue>,
}

struct Ctx<'l> {
    limits: &'l ParseLimits,
    notes: BTreeMap<(NoteKind, String), u32>,
    items: Vec<Lbrn2Item>,
    settings: BTreeMap<u32, SettingAcc>,
    fonts: BTreeMap<String, (u32, u32)>,
    assets: Vec<AssetNote>,
    verts: BTreeMap<String, Vec<Vertex>>,
    prims: BTreeMap<String, Vec<Prim>>,
    vertices: usize,
    app_version: Option<String>,
    project: ProjectInfo,
}

const SETTING_VALUES: &[&str] =
    &["index", "name", "minPower", "maxPower", "speed", "numPasses", "interval", "ditherMode"];

/// Default fill line interval when a scan setting has none (LightBurn's default, reported).
const DEFAULT_INTERVAL_MM: f64 = 0.1;

impl Ctx<'_> {
    fn note(&mut self, kind: NoteKind, name: String) {
        *self.notes.entry((kind, name)).or_insert(0) += 1;
    }

    fn check_attrs(&mut self, node: &Node) {
        let known = supported_attributes(&node.name).unwrap_or(&[]);
        for (a, _) in &node.attrs {
            if !known.contains(&a.as_str()) {
                self.note(NoteKind::UnknownAttribute, format!("{}@{a}", node.name));
            }
        }
    }

    /// An element outside the subset: it and every element below it are reported.
    fn unknown(&mut self, node: &Node) {
        self.note(NoteKind::UnknownElement, node.name.clone());
        for c in &node.children {
            self.unknown(c);
        }
    }

    fn cut_setting(&mut self, node: &Node) {
        self.check_attrs(node);
        let kind = match (node.name.as_str(), node.attr("type")) {
            ("CutSetting_Img", _) | (_, Some("Image")) => Some(CutKind::Image),
            (_, Some("Cut")) => Some(CutKind::Line),
            (_, Some("Scan")) => Some(CutKind::Fill),
            (_, other) => {
                self.note(NoteKind::InvalidValue, format!("{}@type={}", node.name, other.unwrap_or("")));
                None
            }
        };
        let mut acc = SettingAcc { kind, values: BTreeMap::new(), others: Vec::new() };
        for c in &node.children {
            match SETTING_VALUES.iter().find(|v| **v == c.name) {
                Some(key) => {
                    self.check_attrs(c);
                    for g in &c.children {
                        self.unknown(g);
                    }
                    if let Some(v) = c.attr("Value") {
                        acc.values.insert(key, v.to_owned());
                    }
                }
                None => {
                    acc.others.push(UnmappedValue { name: c.name.clone(), value: c.attr("Value").map(str::to_owned) });
                    self.note(NoteKind::UnmappedSetting, c.name.clone());
                    for g in &c.children {
                        self.unknown(g);
                    }
                }
            }
        }
        let index = acc.values.get("index").and_then(|v| v.trim().parse::<u32>().ok());
        match index {
            Some(i) => {
                if self.settings.insert(i, acc).is_some() {
                    self.note(NoteKind::InvalidValue, format!("{}#index={i} (duplicate)", node.name));
                }
            }
            None => self.note(NoteKind::InvalidValue, format!("{}#index", node.name)),
        }
    }

    fn xform(&mut self, node: &Node, parent: Affine) -> Affine {
        let Some(x) = node.child("XForm") else { return parent };
        let nums: Vec<f64> = x.text.split_whitespace().filter_map(|t| t.parse::<f64>().ok()).collect();
        match <[f64; 6]>::try_from(nums.as_slice()).ok().and_then(|c| Affine::new(c).ok()) {
            Some(local) => local.then(&parent),
            None => {
                self.note(NoteKind::InvalidValue, "XForm".to_owned());
                parent
            }
        }
    }

    fn shape(&mut self, node: &Node, parent: Affine, entity: &str) -> Result<(), ParseError> {
        self.check_attrs(node);
        let m = self.xform(node, parent);
        let ty = node.attr("Type").unwrap_or("");
        let handled: &[&str] = match ty {
            "Group" => &["XForm", "Children"],
            "Path" => &["XForm", "VertList", "PrimList"],
            "Text" => &["XForm", "BackupPath"],
            _ => &["XForm"],
        };
        for c in &node.children {
            if !handled.contains(&c.name.as_str()) {
                self.unknown(c);
            }
        }
        let cut = node.attr("CutIndex").and_then(|v| v.trim().parse::<u32>().ok());
        let local = match ty {
            "Rect" => self.rect(node),
            "Ellipse" => self.ellipse(node),
            "Path" => self.vector_path(node, node.attr("VertID"), node.attr("PrimID"))?,
            "Group" => {
                if let Some(children) = node.child("Children") {
                    let mut n = 0usize;
                    for c in &children.children {
                        if c.name == "Shape" {
                            n += 1;
                            self.shape(c, m, &format!("{entity}/Shape#{n}"))?;
                        } else {
                            self.unknown(c);
                        }
                    }
                }
                return Ok(());
            }
            "Text" => {
                let family = node.attr("Font").unwrap_or("").split(',').next().unwrap_or("").trim().to_owned();
                let backup = match node.child("BackupPath") {
                    Some(b) => {
                        self.check_attrs(b);
                        for c in &b.children {
                            if !matches!(c.name.as_str(), "VertList" | "PrimList") {
                                self.unknown(c);
                            }
                        }
                        self.vector_path(b, b.attr("VertID"), b.attr("PrimID"))?
                    }
                    None => None,
                };
                let slot = self.fonts.entry(family).or_insert((0, 0));
                slot.0 += 1;
                if backup.is_some() {
                    slot.1 += 1;
                } else {
                    self.note(NoteKind::NotConverted, "Text".to_owned());
                }
                backup
            }
            "Bitmap" => {
                let data = match node.attr("Data").filter(|d| !d.trim().is_empty()) {
                    None => None,
                    Some(text) => {
                        let decoded = base64_decode(text).and_then(|bytes| {
                            let info = inspect_raster(&bytes, &RasterLimits::default()).ok()?;
                            Some(EmbeddedBitmap { bytes, format: info.format })
                        });
                        if decoded.is_none() {
                            self.note(NoteKind::InvalidValue, "Bitmap@Data".to_owned());
                        }
                        decoded
                    }
                };
                let size = self.number(node, "W").zip(self.number(node, "H")).filter(|(w, h)| *w > 0.0 && *h > 0.0);
                let placement = size.and_then(|(w, h)| {
                    PathBuilder::new()
                        .move_to(Point::new(-w / 2.0, -h / 2.0))
                        .line_to(Point::new(w / 2.0, -h / 2.0))
                        .line_to(Point::new(w / 2.0, h / 2.0))
                        .line_to(Point::new(-w / 2.0, h / 2.0))
                        .close()
                        .build()
                        .ok()
                        .map(|p| p.transform(&m))
                });
                const ADJUST: [&str; 6] =
                    ["Gamma", "Contrast", "Brightness", "EnhanceAmount", "EnhanceRadius", "EnhanceDenoise"];
                let adjustments = node
                    .attrs
                    .iter()
                    .filter(|(k, _)| ADJUST.contains(&k.as_str()))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                self.assets.push(AssetNote {
                    entity: entity.to_owned(),
                    linked_file: node.attr("File").map(str::to_owned),
                    embedded_chars: node.attr("Data").map_or(0, str::len),
                    data,
                    size_mm: size,
                    placement,
                    adjustments,
                });
                self.note(NoteKind::NotConverted, "Bitmap".to_owned());
                None
            }
            other => {
                self.note(NoteKind::UnsupportedShape, other.to_owned());
                None
            }
        };
        if let Some(path) = local {
            let name = if ty == "Text" { node.attr("Str").unwrap_or("Text").to_owned() } else { ty.to_owned() };
            self.items.push(Lbrn2Item { name, entity: entity.to_owned(), cut_index: cut, path: path.transform(&m) });
        }
        Ok(())
    }

    fn number(&mut self, node: &Node, attr: &str) -> Option<f64> {
        let v = node.attr(attr)?.trim().parse::<f64>().ok().filter(|v| v.is_finite());
        if v.is_none() {
            self.note(NoteKind::InvalidValue, format!("Shape@{attr}"));
        }
        v
    }

    fn rect(&mut self, node: &Node) -> Option<Path> {
        let (w, h) = (self.number(node, "W")?, self.number(node, "H")?);
        let r = match node.attr("Cr") {
            Some(_) => self.number(node, "Cr")?,
            None => 0.0,
        };
        if !(w > 0.0 && h > 0.0 && r >= 0.0 && 2.0 * r <= w.min(h)) {
            self.note(NoteKind::InvalidValue, "Shape#Rect".to_owned());
            return None;
        }
        let (x0, y0, x1, y1) = (-w / 2.0, -h / 2.0, w / 2.0, h / 2.0);
        let built = if r == 0.0 {
            PathBuilder::new()
                .move_to(Point::new(x0, y0))
                .line_to(Point::new(x1, y0))
                .line_to(Point::new(x1, y1))
                .line_to(Point::new(x0, y1))
                .close()
                .build()
        } else {
            let quarter = |c: Point, start: f64| {
                Angle::from_degrees(start)
                    .and_then(|s| Angle::from_degrees(90.0).map(|q| (s, q)))
                    .ok()
                    .and_then(|(s, q)| EllipticalArc::circle(c, r, s, q).ok())
            };
            let arcs = [
                quarter(Point::new(x1 - r, y0 + r), -90.0),
                quarter(Point::new(x1 - r, y1 - r), 0.0),
                quarter(Point::new(x0 + r, y1 - r), 90.0),
                quarter(Point::new(x0 + r, y0 + r), 180.0),
            ];
            let [Some(a0), Some(a1), Some(a2), Some(a3)] = arcs else { return None };
            PathBuilder::new()
                .move_to(Point::new(x0 + r, y0))
                .line_to(Point::new(x1 - r, y0))
                .arc(a0)
                .line_to(Point::new(x1, y1 - r))
                .arc(a1)
                .line_to(Point::new(x0 + r, y1))
                .arc(a2)
                .line_to(Point::new(x0, y0 + r))
                .arc(a3)
                .close()
                .build()
        };
        built.ok()
    }

    fn ellipse(&mut self, node: &Node) -> Option<Path> {
        let (rx, ry) = (self.number(node, "Rx")?, self.number(node, "Ry")?);
        let shape = Shape::Ellipse {
            center: Point::ORIGIN,
            rx: Length::from_mm(rx).ok()?,
            ry: Length::from_mm(ry).ok()?,
            rotation: Angle::from_degrees(0.0).ok()?,
        };
        match shape.to_path() {
            Ok(p) => Some(p),
            Err(_) => {
                self.note(NoteKind::InvalidValue, "Shape#Ellipse".to_owned());
                None
            }
        }
    }

    /// A `VertList`/`PrimList` pair, either inline or shared by id with an earlier shape.
    fn vector_path(&mut self, node: &Node, vid: Option<&str>, pid: Option<&str>) -> Result<Option<Path>, ParseError> {
        let verts = match node.child("VertList") {
            Some(v) => match parse_verts(&v.text) {
                Some(list) => {
                    self.vertices += list.len();
                    if self.vertices > self.limits.max_path_commands {
                        return Err(ParseError::TooManyCommands { limit: self.limits.max_path_commands });
                    }
                    if let Some(id) = vid {
                        self.verts.insert(id.to_owned(), list.clone());
                    }
                    Some(list)
                }
                None => {
                    self.note(NoteKind::InvalidValue, "VertList".to_owned());
                    return Ok(None);
                }
            },
            None => vid.and_then(|id| self.verts.get(id).cloned()),
        };
        let Some(verts) = verts else {
            self.note(NoteKind::InvalidValue, "VertList (missing)".to_owned());
            return Ok(None);
        };
        let prims = match node.child("PrimList") {
            Some(p) => match parse_prims(&p.text, verts.len()) {
                Some(list) => {
                    self.vertices += list.len();
                    if self.vertices > self.limits.max_path_commands {
                        return Err(ParseError::TooManyCommands { limit: self.limits.max_path_commands });
                    }
                    if let Some(id) = pid {
                        self.prims.insert(id.to_owned(), list.clone());
                    }
                    Some(list)
                }
                None => {
                    self.note(NoteKind::InvalidValue, "PrimList".to_owned());
                    return Ok(None);
                }
            },
            None => pid.and_then(|id| self.prims.get(id).cloned()),
        };
        let Some(prims) = prims else {
            self.note(NoteKind::InvalidValue, "PrimList (missing)".to_owned());
            return Ok(None);
        };
        let path = build_path(&verts, &prims);
        if path.is_none() {
            self.note(NoteKind::InvalidValue, "PrimList".to_owned());
        }
        Ok(path)
    }

    fn finish(self) -> Lbrn2Draft {
        let mut cut_settings = Vec::new();
        let mut notes: Vec<ImportNote> =
            self.notes.into_iter().map(|((kind, name), count)| ImportNote { kind, name, count }).collect();
        for (index, acc) in &self.settings {
            let items: Vec<usize> =
                self.items.iter().enumerate().filter(|(_, it)| it.cut_index == Some(*index)).map(|(i, _)| i).collect();
            let (draft, mut assumptions) = proposal(*index, acc);
            let image = if acc.kind == Some(CutKind::Image) {
                let (image, extra, invalid) = image_proposal(*index, acc);
                assumptions.extend(extra);
                if let Some(name) = invalid {
                    notes.push(ImportNote { kind: NoteKind::InvalidValue, name, count: 1 });
                }
                image
            } else {
                None
            };
            if draft.is_none() && acc.kind != Some(CutKind::Image) {
                notes.push(ImportNote { kind: NoteKind::InvalidValue, name: format!("CutSetting#{index}"), count: 1 });
            }
            if acc.kind == Some(CutKind::Image) {
                notes.push(ImportNote {
                    kind: NoteKind::NotConverted,
                    name: format!("CutSetting_Img#{index}"),
                    count: 1,
                });
            }
            cut_settings.push(CutSettingProposal {
                index: *index,
                kind: acc.kind.unwrap_or(CutKind::Line),
                draft,
                assumptions,
                items,
                unmapped: acc.others.clone(),
                image,
            });
        }
        for it in &self.items {
            if let Some(c) = it.cut_index
                && !self.settings.contains_key(&c)
            {
                notes.push(ImportNote {
                    kind: NoteKind::InvalidValue,
                    name: format!("{}@CutIndex={c}", it.entity),
                    count: 1,
                });
            }
        }
        let fonts = self
            .fonts
            .into_iter()
            .map(|(family, (texts, with_outlines))| FontUse { family, texts, with_outlines })
            .collect();
        Lbrn2Draft {
            items: self.items,
            cut_settings,
            report: Lbrn2Report {
                app_version: self.app_version,
                notes,
                fonts,
                assets: self.assets,
                project: self.project,
            },
        }
    }
}

/// The operation proposal of one setting: output **disabled**, every assumption listed.
fn proposal(index: u32, acc: &SettingAcc) -> (Option<OperationDraft>, Vec<Assumption>) {
    let context = format!("CutSetting#{index}");
    let mut assumptions = Vec::new();
    let mut assume = |field: &'static str, value: String| {
        assumptions.push(Assumption { field, value, context: context.clone() });
    };
    let num = |k: &str| acc.values.get(k).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite());
    let kind = match acc.kind {
        Some(CutKind::Line) => CutKind::Line,
        Some(CutKind::Fill) => CutKind::Fill,
        _ => return (None, assumptions),
    };
    let (Some(speed), Some(max)) = (num("speed"), num("maxPower")) else { return (None, assumptions) };
    assume("speed unit", "mm/s (LightBurn file unit; not verified per device)".to_owned());
    let min = match num("minPower") {
        Some(v) => v,
        None => {
            assume("minPower", format!("{max} (= maxPower)"));
            max
        }
    };
    let passes = match num("numPasses") {
        Some(v) if v >= 1.0 && v.fract() == 0.0 && v <= f64::from(u32::MAX) => v as u32,
        Some(_) => return (None, assumptions),
        None => {
            assume("numPasses", "1".to_owned());
            1
        }
    };
    let name = acc.values.get("name").cloned().unwrap_or_else(|| format!("C{index:02}"));
    let settings = (|| {
        Some(ProcessSettings {
            speed: Speed::mm_per_s(speed).ok()?,
            power: PowerRange::new(PowerPercent::new(min).ok()?, PowerPercent::new(max).ok()?).ok()?,
            passes: Passes::new(passes).ok()?,
            air_assist: false,
        })
    })();
    let Some(settings) = settings else { return (None, assumptions) };
    assume("air assist", "off".to_owned());
    let mode = match kind {
        CutKind::Fill => {
            let interval = match num("interval") {
                Some(v) => v,
                None => {
                    assume("interval", format!("{DEFAULT_INTERVAL_MM} mm"));
                    DEFAULT_INTERVAL_MM
                }
            };
            assume("scan", "X axis, bidirectional, no overscan".to_owned());
            let fill = Length::from_mm(interval)
                .ok()
                .zip(Length::from_mm(0.0).ok())
                .and_then(|(i, o)| FillSettings::new(i, ScanAxis::X, ScanDirection::Bidirectional, o).ok());
            match fill {
                Some(f) => OperationMode::Fill(f),
                None => return (None, assumptions),
            }
        }
        _ => OperationMode::Line { kerf: Kerf::NONE },
    };
    (Some(OperationDraft { name, mode, settings, output_enabled: false }), assumptions)
}

/// `ditherMode` names of LightBurn's image modes → our render modes. The *names* are LightBurn's
/// UI vocabulary and only `grayscale` is seen in a research fixture; the parameters of the
/// thresholded and screened modes are our defaults. Every use is listed as an assumption.
fn dither_render(name: &str) -> Option<(ImageRender, Option<&'static str>)> {
    Some(match name {
        "grayscale" => (ImageRender::Grayscale, None),
        "threshold" => (ImageRender::Dither(DitherMode::Threshold { level: 128 }), Some("threshold level 128")),
        "dither" => (ImageRender::Dither(DitherMode::FloydSteinberg), Some("Floyd-Steinberg error diffusion")),
        "atkinson" => (ImageRender::Dither(DitherMode::Atkinson), Some("Atkinson error diffusion")),
        "stucki" => (ImageRender::Dither(DitherMode::Stucki), Some("Stucki error diffusion")),
        "jarvis" => (ImageRender::Dither(DitherMode::Jarvis), Some("Jarvis error diffusion")),
        "ordered" => (ImageRender::Dither(DitherMode::Ordered { size: 4 }), Some("ordered dithering, 4 x 4 matrix")),
        "newsprint" => (ImageRender::Dither(DitherMode::Newsprint), Some("45 degree round-dot screen")),
        "halftone" => (ImageRender::Dither(DitherMode::Halftone), Some("round-dot halftone screen")),
        "sketch" => (ImageRender::Dither(DitherMode::Sketch { level: 128 }), Some("edge lines, strength 128")),
        _ => return None,
    })
}

/// The image-mode proposal of an image cut setting, the assumptions it needed, and the name of
/// an invalid value to report (`ditherMode=…` for a mode we cannot map).
fn image_proposal(index: u32, acc: &SettingAcc) -> (Option<ImageProposal>, Vec<Assumption>, Option<String>) {
    let context = format!("CutSetting_Img#{index}");
    let mut assumptions = Vec::new();
    let mut assume = |field: &'static str, value: String| {
        assumptions.push(Assumption { field, value, context: context.clone() });
    };
    let num = |k: &str| acc.values.get(k).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite());
    let (Some(speed), Some(max)) = (num("speed"), num("maxPower")) else { return (None, assumptions, None) };
    let min = num("minPower").unwrap_or_else(|| {
        assume("minPower", format!("{max} (= maxPower)"));
        max
    });
    let passes = match num("numPasses") {
        Some(v) if v >= 1.0 && v.fract() == 0.0 && v <= f64::from(u32::MAX) => v as u32,
        _ => {
            assume("numPasses", "1".to_owned());
            1
        }
    };
    let interval = num("interval").filter(|v| *v > 0.0).unwrap_or_else(|| {
        assume("interval", format!("{DEFAULT_INTERVAL_MM} mm"));
        DEFAULT_INTERVAL_MM
    });
    let dither_name = acc.values.get("ditherMode").cloned();
    let mut invalid = None;
    let render = match dither_name.as_deref() {
        Some(name) => match dither_render(name) {
            Some((render, how)) => {
                if let Some(how) = how {
                    assume("ditherMode", format!("{name} = {how} (name table not verified against LightBurn)"));
                }
                Some(render)
            }
            None => {
                invalid = Some(format!("ditherMode={name}"));
                None
            }
        },
        None => {
            assume("ditherMode", "grayscale (not in the file)".to_owned());
            Some(ImageRender::Grayscale)
        }
    };
    let name = acc.values.get("name").cloned().unwrap_or_else(|| format!("C{index:02}"));
    (
        Some(ImageProposal {
            name,
            dither_name,
            render,
            interval_mm: interval,
            speed_mm_s: speed,
            min_power: min,
            max_power: max,
            passes,
        }),
        assumptions,
        invalid,
    )
}

fn parse_verts(text: &str) -> Option<Vec<Vertex>> {
    let mut c = Cursor { s: text.as_bytes(), pos: 0 };
    let mut out = Vec::new();
    loop {
        c.ws();
        if c.done() {
            return Some(out);
        }
        if !c.eat("V") {
            return None;
        }
        let at = Point::new(c.num()?, c.num()?);
        let (mut c0, mut c1) = (None, None);
        loop {
            if c.eat("c0x") {
                let x = c.num()?;
                // `c0x1` without `c0y` is LightBurn's "no control point" marker.
                if c.eat("c0y") {
                    c0 = Some(Point::new(x, c.num()?));
                }
            } else if c.eat("c1x") {
                let x = c.num()?;
                if c.eat("c1y") {
                    c1 = Some(Point::new(x, c.num()?));
                }
            } else {
                break;
            }
        }
        out.push(Vertex { at, c0, c1 });
    }
}

fn parse_prims(text: &str, verts: usize) -> Option<Vec<Prim>> {
    let t = text.trim();
    if t == "LineClosed" || t == "LineOpen" {
        let n = verts;
        if n < 2 {
            return None;
        }
        let mut out: Vec<Prim> = (0..n - 1).map(|i| Prim::Line(i, i + 1)).collect();
        if t == "LineClosed" {
            out.push(Prim::Line(n - 1, 0));
        }
        return Some(out);
    }
    let mut c = Cursor { s: t.as_bytes(), pos: 0 };
    let mut out = Vec::new();
    loop {
        c.ws();
        if c.done() {
            return Some(out);
        }
        let line = if c.eat("L") {
            true
        } else if c.eat("B") {
            false
        } else {
            return None;
        };
        let (a, b) = (c.index()?, c.index()?);
        if a >= verts || b >= verts {
            return None;
        }
        out.push(if line { Prim::Line(a, b) } else { Prim::Bezier(a, b) });
    }
}

fn build_path(verts: &[Vertex], prims: &[Prim]) -> Option<Path> {
    let mut subpaths = Vec::new();
    let mut current: Option<(usize, Point, Vec<Segment>, usize)> = None; // start index, start, segs, end index
    let flush = |cur: Option<(usize, Point, Vec<Segment>, usize)>, subpaths: &mut Vec<Subpath>| -> Option<()> {
        if let Some((start_i, start, segs, end_i)) = cur
            && !segs.is_empty()
        {
            subpaths.push(Subpath::new(start, segs, start_i == end_i).ok()?);
        }
        Some(())
    };
    for p in prims {
        let (a, b) = match *p {
            Prim::Line(a, b) | Prim::Bezier(a, b) => (a, b),
        };
        let (va, vb) = (verts.get(a)?, verts.get(b)?);
        let seg = match *p {
            Prim::Line(..) => Segment::Line { to: vb.at },
            Prim::Bezier(..) => Segment::Cubic { c1: va.c0.unwrap_or(va.at), c2: vb.c1.unwrap_or(vb.at), to: vb.at },
        };
        match current.as_mut() {
            Some((start_i, _, segs, end_i)) if *end_i == a && *end_i != *start_i => {
                segs.push(seg);
                *end_i = b;
            }
            _ => {
                flush(current.take(), &mut subpaths)?;
                current = Some((a, va.at, vec![seg], b));
            }
        }
    }
    flush(current, &mut subpaths)?;
    (!subpaths.is_empty()).then(|| Path::from_subpaths(subpaths))
}

struct Cursor<'a> {
    s: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn done(&self) -> bool {
        self.pos >= self.s.len()
    }

    fn ws(&mut self) {
        while self.s.get(self.pos).is_some_and(|b| b.is_ascii_whitespace() || *b == b',') {
            self.pos += 1;
        }
    }

    fn eat(&mut self, lit: &str) -> bool {
        if self.s.get(self.pos..self.pos + lit.len()) == Some(lit.as_bytes()) {
            self.pos += lit.len();
            true
        } else {
            false
        }
    }

    fn num(&mut self) -> Option<f64> {
        self.ws();
        let start = self.pos;
        while self.s.get(self.pos).is_some_and(|b| b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.' | b'e' | b'E'))
        {
            // An exponent sign only directly after 'e'; another sign starts the next number.
            if matches!(self.s.get(self.pos), Some(b'-' | b'+'))
                && self.pos > start
                && !matches!(self.s.get(self.pos - 1), Some(b'e' | b'E'))
            {
                break;
            }
            self.pos += 1;
        }
        let v: f64 = std::str::from_utf8(self.s.get(start..self.pos)?).ok()?.parse().ok()?;
        v.is_finite().then_some(v)
    }

    fn index(&mut self) -> Option<usize> {
        self.ws();
        let start = self.pos;
        while self.s.get(self.pos).is_some_and(u8::is_ascii_digit) {
            self.pos += 1;
        }
        std::str::from_utf8(self.s.get(start..self.pos)?).ok()?.parse().ok()
    }
}

impl Lbrn2Draft {
    /// The operation for an image cut setting once its bitmap exists as an asset (`source` is
    /// the stored image's hash). Output stays disabled; `None` for an unknown layer, an image
    /// mode we cannot map or values the process model rejects.
    pub fn image_operation(&self, index: u32, source: ContentHash) -> Option<OperationDraft> {
        let p = self.cut_settings.iter().find(|p| p.index == index)?;
        let img = p.image.as_ref()?;
        let interval = Length::from_mm(img.interval_mm).ok()?;
        let settings = ImageSettings::new(source, interval, interval, img.render?).ok()?;
        let process = ProcessSettings {
            speed: Speed::mm_per_s(img.speed_mm_s).ok()?,
            power: PowerRange::new(PowerPercent::new(img.min_power).ok()?, PowerPercent::new(img.max_power).ok()?)
                .ok()?,
            passes: Passes::new(img.passes).ok()?,
            air_assist: false,
        };
        Some(OperationDraft {
            name: img.name.clone(),
            mode: OperationMode::Image(settings),
            settings: process,
            output_enabled: false,
        })
    }

    /// The geometry commit (one undo step), like the SVG/DXF route. `asset` holds the original
    /// file bytes.
    pub fn geometry_command(&self, asset: ContentHash) -> Result<GeometryCommand, ImportError> {
        if self.items.is_empty() {
            return Err(ImportError::Empty);
        }
        let items = self
            .items
            .iter()
            .map(|i| ImportItem { name: i.name.clone(), entity: Some(i.entity.clone()), path: i.path.clone() })
            .collect();
        Ok(GeometryCommand::Import { asset, items })
    }

    /// One `AddOperation` per mappable proposal, targeting the objects the geometry commit
    /// created (`created[i]` is item `i`). Operations arrive with output disabled.
    pub fn operation_commands(&self, created: &[GeometryId]) -> Result<Vec<ProcessCommand>, ImportError> {
        if created.len() != self.items.len() {
            return Err(ImportError::ItemCountMismatch);
        }
        Ok(self
            .cut_settings
            .iter()
            .filter_map(|p| {
                let draft = p.draft.clone()?;
                let targets: Vec<GeometryId> = p.items.iter().filter_map(|i| created.get(*i).copied()).collect();
                (!targets.is_empty()).then_some(ProcessCommand::AddOperation { draft, targets })
            })
            .collect())
    }
}
