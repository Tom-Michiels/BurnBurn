//! On-disk schema for sketches (part of document schema v1; additive: absent = no sketches).
//! Owner of this file: Sketch role (DR-12), reviewed by Kern.

use serde::{Deserialize, Serialize};

use crate::dto::v1::{Num, Xy};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sketch {
    pub id: u64,
    pub name: String,
    pub next_entity: u32,
    pub next_constraint: u32,
    pub entities: Vec<Entity>,
    pub constraints: Vec<Constraint>,
    /// `[name, expression text]` in definition order.
    pub parameters: Vec<[String; 2]>,
    pub solution: Option<Solution>,
    pub evaluation: Option<Evaluation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entity {
    pub id: u32,
    pub construction: bool,
    pub shape: Shape,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Shape {
    Point { at: Xy },
    Line { start: u32, end: u32 },
    Arc { center: u32, start: u32, end: u32, clockwise: bool },
    Circle { center: u32, radius: Num },
    Ellipse { center: u32, major: u32, minor: Num },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Along {
    Direct,
    X,
    Y,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Driving,
    Driven,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Constraint {
    pub id: u32,
    pub kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Kind {
    Coincident {
        a: u32,
        b: u32,
    },
    PointOn {
        point: u32,
        curve: u32,
    },
    Horizontal {
        line: u32,
    },
    Vertical {
        line: u32,
    },
    Parallel {
        a: u32,
        b: u32,
    },
    Perpendicular {
        a: u32,
        b: u32,
    },
    Tangent {
        a: u32,
        b: u32,
    },
    Equal {
        a: u32,
        b: u32,
    },
    Midpoint {
        point: u32,
        line: u32,
    },
    Symmetric {
        a: u32,
        b: u32,
        about: u32,
    },
    Concentric {
        a: u32,
        b: u32,
    },
    Fix {
        point: u32,
        at: Xy,
    },
    Distance {
        a: u32,
        b: u32,
        along: Along,
        text: String,
        role: Role,
    },
    Length {
        line: u32,
        text: String,
        role: Role,
    },
    Radius {
        curve: u32,
        text: String,
        role: Role,
    },
    Diameter {
        curve: u32,
        text: String,
        role: Role,
    },
    /// The full minor axis of an ellipse (G7-editor, S01).
    MinorDiameter {
        ellipse: u32,
        text: String,
        role: Role,
    },
    Angle {
        from: u32,
        to: u32,
        text: String,
        role: Role,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Solution {
    pub points: Vec<(u32, Xy)>,
    pub radii: Vec<(u32, Num)>,
    pub dof: u32,
    /// Solved semi-minor radii of ellipses whose minor axis the solver moved (G7-editor). Absent
    /// = none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minors: Vec<(u32, Num)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evaluation {
    pub inputs: Option<String>,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Status {
    Valid,
    Pending,
    Conflict { constraints: Vec<u32> },
    NotConverged { unmet: Vec<u32> },
    InvalidDimensions { constraints: Vec<u32> },
    InvalidParameters { parameter: Option<String> },
    InvalidDefinition,
}
