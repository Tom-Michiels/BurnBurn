//! Array generators (IM26). They live in `burnburn_geometry::array` (shared with the IM21 array
//! modifiers); re-exported here unchanged.

pub(crate) use burnburn_geometry::array::rotation_about;
pub use burnburn_geometry::array::{ArrayError, CircularArray, GridArray, MAX_COPIES, circular_array, grid_array};
