//! File form of parametric basic shapes (W1-L2b). Additive to schema 2: the field is omitted for
//! plain paths. `frame` is the placement as `[a, b, c, d, e, f]` (`Affine::coefficients`).

use serde::{Deserialize, Serialize};

use crate::dto::v1::Num;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Parametric {
    Polygon { sides: u32, frame: [Num; 6] },
    Star { points: u32, ratio: Num, frame: [Num; 6] },
    DoubleStar { points: u32, ratio: Num, frame: [Num; 6] },
    RoundedRectangle { width: Num, height: Num, radius: Num, frame: [Num; 6] },
}
