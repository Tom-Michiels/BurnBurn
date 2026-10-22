//! Versioned on-disk schema. These structs are the file format; domain types are not.
//!
//! Module `v1` describes schema 1 and its additive successor schema 2 (IM13 process layer):
//! schema 2 writes operations with `targets: [{ "geometry": id }]` and a `process` block, plus
//! optional `constraints` and `recipes`; schema 1 files have bare-number placeholder targets and
//! no process data. The loader checks which form belongs to the declared schema.
//! Mapping goes through validating constructors so NaN/Infinity and inconsistent ids are
//! rejected with the JSON path of the offending value.

pub mod v1 {
    use std::collections::BTreeMap;

    use serde::de::{self, Visitor};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// A JSON number that may be `null` in a damaged file (serde_json writes NaN as `null`).
    /// Non-finite values are rejected when mapping to the domain, with a path.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Num(pub f64);

    impl Serialize for Num {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_f64(self.0)
        }
    }

    impl<'de> Deserialize<'de> for Num {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct V;
            impl<'de> Visitor<'de> for V {
                type Value = Num;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("a number")
                }
                fn visit_f64<E: de::Error>(self, v: f64) -> Result<Num, E> {
                    Ok(Num(v))
                }
                fn visit_i64<E: de::Error>(self, v: i64) -> Result<Num, E> {
                    Ok(Num(v as f64))
                }
                fn visit_u64<E: de::Error>(self, v: u64) -> Result<Num, E> {
                    Ok(Num(v as f64))
                }
                fn visit_unit<E: de::Error>(self) -> Result<Num, E> {
                    Ok(Num(f64::NAN))
                }
            }
            d.deserialize_any(V)
        }
    }

    /// For optional numbers: absent = `None`, but an explicit `null` (a NaN written by serde_json)
    /// is `Some(NaN)`, so it is refused with its path instead of silently meaning "off".
    pub fn present_num<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Num>, D::Error> {
        Num::deserialize(d).map(Some)
    }

    pub type Xy = [Num; 2];

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Manifest {
        pub format: String,
        pub schema: u32,
        pub app: String,
        pub project: String,
        pub document: String,
        pub assets: BTreeMap<String, AssetEntry>,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct AssetEntry {
        pub media: String,
        pub bytes: u64,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Document {
        pub next_id: u64,
        pub objects: Vec<Object>,
        pub operations: Vec<Operation>,
        /// Hard order constraints between operations (schema 2). Omitted when empty.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub constraints: Vec<Constraint>,
        /// Material recipe library (schema 2). Omitted when empty.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub recipes: Vec<Recipe>,
        /// Optimization settings (W2-L3). Omitted when they are the defaults.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub optimization: Option<Optimization>,
        /// Cut priorities (W2-L3): only shapes with a non-default priority. Omitted when empty.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub priorities: Vec<Priority>,
        /// Project notes (G5-UI-gaten, P04). Omitted when empty; absent in older files.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        pub notes: String,
        /// Parametric sketches (IM11). Omitted when empty, so sketch-free files are unchanged.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub sketches: Vec<crate::sketch_dto::Sketch>,
        /// Text objects (IM18, DR-22). Omitted when empty, so text-free files are unchanged.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub texts: Vec<crate::text_dto::Text>,
        /// Modifiers, references, relations, parts and plates (IM21). Omitted when empty.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub structure: Option<crate::structure_dto::Structure>,
        /// Traces (C-9). Omitted when empty.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub traces: Vec<crate::engine_dto::Trace>,
        /// Generators (IM22). Omitted when empty.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub generators: Vec<crate::engine_dto::Generator>,
        /// Draw order and locks (W2-L1-bewerken). Omitted when there are none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub arrange: Option<crate::arrange_dto::Arrange>,
    }

    /// Level of the `Order by` hierarchy.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum OrderBy {
        Layer,
        Group,
        Priority,
    }

    /// Planner options of the document (W2-L3).
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Optimization {
        pub order_by: Vec<OrderBy>,
        pub inner_first: bool,
        pub reduce_travel: bool,
        pub best_start: bool,
        pub best_direction: bool,
        pub hide_backlash: bool,
        /// Overlap tolerance in mm; omitted when overlapping lines are kept.
        #[serde(default, deserialize_with = "present_num", skip_serializing_if = "Option::is_none")]
        pub remove_overlapping_mm: Option<Num>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Priority {
        pub geometry: u64,
        pub priority: i32,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Object {
        pub id: u64,
        pub name: String,
        pub kind: Kind,
        pub source: Source,
        pub subpaths: Vec<Subpath>,
        /// Group chain, outermost first (IM09). Omitted when ungrouped, so older files are unchanged.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub groups: Vec<u64>,
        /// Stable edge keys (DR-20). Omitted when they are the default 0..n.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub edge_keys: Option<crate::structure_dto::EdgeKeys>,
        /// Editable definition of a basic shape (W1-L2b). Omitted for plain paths, so older files
        /// stay byte-identical. The subpaths stay the outline of record.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub parametric: Option<crate::shape_dto::Parametric>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Kind {
        Rectangle,
        Path,
        Line,
        Circle,
        Ellipse,
        Polygon,
        Star,
        Polyline,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
    pub enum Source {
        Drawn,
        Imported {
            asset: String,
            entity: Option<String>,
        },
        /// Output `local` of sketch `sketch` (contract §11 `DerivedKey`).
        DerivedSketch {
            sketch: u64,
            local: u64,
        },
        /// Output `local` (line index) of text object `text` (IM18).
        DerivedText {
            text: u64,
            local: u64,
        },
        /// Output `local` of modifier chain `modifier` (IM21).
        DerivedModifier {
            modifier: u64,
            local: u64,
        },
        /// Output `local` (shape key) of part instance `instance` (IM21).
        DerivedPart {
            instance: u64,
            local: u64,
        },
        /// Output `local` (contour key) of trace `trace` (C-9).
        DerivedTrace {
            trace: u64,
            local: u64,
        },
        /// Output `local` of generator `generator` (IM22).
        DerivedGenerator {
            generator: u64,
            local: u64,
        },
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Subpath {
        pub start: Xy,
        pub closed: bool,
        pub segments: Vec<Segment>,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
    pub enum Segment {
        Line {
            to: Xy,
        },
        Cubic {
            c1: Xy,
            c2: Xy,
            to: Xy,
        },
        /// `p(t) = center + u cos t + v sin t`, `t ∈ [start, start + sweep]` (radians).
        Arc {
            center: Xy,
            u: Xy,
            v: Xy,
            start: Num,
            sweep: Num,
        },
    }

    /// A process operation. Schema 1 (placeholder): bare-number targets, no `process`.
    /// Schema 2: object targets and a `process` block.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Operation {
        pub id: u64,
        pub name: String,
        pub targets: Vec<Target>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub process: Option<OperationProcess>,
    }

    /// A geometry reference. An object (not a bare number) so a later sub-reference
    /// (`GeometryRef`, IM21) is an additive field (DR-15).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum Target {
        Ref(TargetRef),
        /// Schema 1 placeholder form.
        Legacy(u64),
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct TargetRef {
        pub geometry: u64,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct OperationProcess {
        pub mode: Mode,
        pub settings: Settings,
        pub output_enabled: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub recipe: Option<RecipeRef>,
        /// IM25 options (additive in schema 2). Omitted when every option is off.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub advanced: Option<Advanced>,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
    pub enum Mode {
        Line {
            kerf_mm: Num,
        },
        Fill {
            interval_mm: Num,
            axis: ScanAxis,
            direction: ScanDirection,
            overscan_mm: Num,
            /// Scan angle (IM25). Omitted for an unrotated fill.
            #[serde(default, deserialize_with = "present_num", skip_serializing_if = "Option::is_none")]
            angle_deg: Option<Num>,
            /// Second, perpendicular pass (W1-L3). Omitted when off.
            #[serde(default, skip_serializing_if = "std::ops::Not::not")]
            cross_hatch: bool,
            /// Omitted for "all shapes at once".
            #[serde(default, skip_serializing_if = "Option::is_none")]
            grouping: Option<FillGrouping>,
            /// Omitted when off.
            #[serde(default, skip_serializing_if = "std::ops::Not::not")]
            flood_fill: bool,
            /// Power ramp length at the run ends. Omitted when zero.
            #[serde(default, deserialize_with = "present_num", skip_serializing_if = "Option::is_none")]
            ramp_mm: Option<Num>,
        },
        /// Concentric rings (IM25).
        OffsetFill {
            step_mm: Num,
            order: RingOrder,
        },
        /// Image engraving (IM18); `source` is the content hash (hex) of the source pixels.
        Image {
            source: String,
            dot_pitch_mm: Num,
            interval_mm: Num,
            render: Render,
            gamma: Num,
            brightness: Num,
            contrast: Num,
            direction: ScanDirection,
            serpentine: bool,
            overscan_mm: Num,
            /// Options added after IM18 (W1-L5a). Omitted when all are at their defaults.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            options: Option<ImageOptions>,
        },
    }

    /// Which shapes of a fill are scanned together (the default, all at once, is omitted).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum FillGrouping {
        Groups,
        Individual,
    }

    /// Lead shape; the default line lead is omitted.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum LeadStyle {
        Arc,
    }

    /// Image options beyond IM18; every field is omitted when at its default.
    #[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ImageOptions {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        pub negative: bool,
        #[serde(default, deserialize_with = "present_num", skip_serializing_if = "Option::is_none")]
        pub dot_width_correction_mm: Option<Num>,
        #[serde(default, deserialize_with = "present_num", skip_serializing_if = "Option::is_none")]
        pub scan_angle_deg: Option<Num>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub gray: Option<GrayMethod>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub crop: Option<Crop>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub sharpen: Option<Sharpen>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub screen: Option<Screen>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        pub mask: bool,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum GrayMethod {
        Rec709,
        Rec601,
        Average,
        Lightness,
        Red,
        Green,
        Blue,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Crop {
        pub x: u32,
        pub y: u32,
        pub width: u32,
        pub height: u32,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Sharpen {
        pub amount: Num,
        pub radius_mm: Num,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Screen {
        pub cell_mm: Num,
        pub angle_deg: Num,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum RingOrder {
        OutsideIn,
        InsideOut,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
    pub enum Render {
        Grayscale,
        Threshold { level: u8 },
        FloydSteinberg,
        Jarvis,
        Stucki,
        Atkinson,
        Ordered { size: u8 },
        Newsprint,
        Halftone,
        Sketch { level: u8 },
        PassThrough,
    }

    /// IM25 options; every field is omitted when off.
    #[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Advanced {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub tabs: Option<Tabs>,
        #[serde(default, deserialize_with = "present_num", skip_serializing_if = "Option::is_none")]
        pub lead_in_mm: Option<Num>,
        #[serde(default, deserialize_with = "present_num", skip_serializing_if = "Option::is_none")]
        pub lead_out_mm: Option<Num>,
        /// Lead shape and angle to the contour in degrees (W1-L3); omitted for a perpendicular line.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub lead_in_style: Option<LeadStyle>,
        #[serde(default, deserialize_with = "present_num", skip_serializing_if = "Option::is_none")]
        pub lead_in_angle_deg: Option<Num>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub lead_out_style: Option<LeadStyle>,
        #[serde(default, deserialize_with = "present_num", skip_serializing_if = "Option::is_none")]
        pub lead_out_angle_deg: Option<Num>,
        #[serde(default, deserialize_with = "present_num", skip_serializing_if = "Option::is_none")]
        pub overcut_mm: Option<Num>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub perforation: Option<PerforationDto>,
        /// Dot (dwell) mode (W1-L3).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub dot: Option<DotDto>,
        #[serde(default, deserialize_with = "present_num", skip_serializing_if = "Option::is_none")]
        pub z_step_mm: Option<Num>,
        /// Focus offset of the operation (W2-L3). Omitted when none.
        #[serde(default, deserialize_with = "present_num", skip_serializing_if = "Option::is_none")]
        pub z_offset_mm: Option<Num>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub sub_operations: Vec<SubOperationDto>,
        /// Omitted for the default (`cut_once`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub shared_edges: Option<SharedEdges>,
    }

    /// Tabs: exactly one of `count` and `spacing_mm` (W1-L3) is present.
    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Tabs {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub count: Option<u32>,
        #[serde(default, deserialize_with = "present_num", skip_serializing_if = "Option::is_none")]
        pub spacing_mm: Option<Num>,
        pub width_mm: Num,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct DotDto {
        pub spacing_mm: Num,
        pub dwell_ms: Num,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct PerforationDto {
        pub on_mm: Num,
        pub off_mm: Num,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct SubOperationDto {
        pub mode: Mode,
        pub settings: Settings,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum SharedEdges {
        CutOnce,
        CutPerRegion,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ScanAxis {
        X,
        Y,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ScanDirection {
        Bidirectional,
        Unidirectional,
    }

    /// Units are in the field names: mm/s and percent of rated power.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Settings {
        pub speed_mm_s: Num,
        pub power_min_pct: Num,
        pub power_max_pct: Num,
        pub passes: u32,
        pub air_assist: bool,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct RecipeRef {
        pub recipe: u64,
        pub revision: u32,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Constraint {
        pub before: u64,
        pub after: u64,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Recipe {
        pub id: u64,
        pub name: String,
        pub material: String,
        pub thickness_mm: Num,
        pub machine: String,
        pub revision: u32,
        pub notes: String,
        pub mode: Mode,
        pub settings: Settings,
    }
}
