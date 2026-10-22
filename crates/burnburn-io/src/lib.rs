//! The `.burnburn` project container v1 and its storage route (ADR 0004).
//!
//! - [`fs::FileSystem`] is the only way this crate touches storage; every step (temp create,
//!   write, sync, replace, directory sync) is separate so tests can inject failures.
//! - [`dto`] holds the versioned on-disk schema, separate from the domain types.
//! - [`save_project`] writes atomically (temp + sync + replace); a failure leaves the previous
//!   file readable. Recovery files are written by a separate route and never replace a Save.
//!
//! No GUI, GPU or network dependency.

pub mod arrange_dto;
pub mod art_library;
pub mod bundle;
pub mod calibration_library;
pub mod camera_calibration_library;
pub mod clipboard;
mod container;
pub mod dto;
pub mod engine_dto;
mod engine_map;
mod error;
pub mod export;
pub mod fs;
mod hash;
pub mod import;
mod map;
pub mod material_library;
pub mod memory;
mod process_map;
mod profile;
pub mod profile_library;
mod project;
pub mod recent;
pub mod remnant_library;
pub mod search;
pub mod shape_dto;
mod shape_map;
pub mod sketch_dto;
mod sketch_map;
pub mod std_fs;
mod store;
pub mod structure_dto;
mod structure_map;
pub mod template;
pub mod text_dto;
mod text_map;
pub mod trace_recipe_library;

pub use container::{SCHEMA_VERSION, container_schema, decode_container, encode_container};
pub use error::{LoadError, SaveError};
pub use hash::{asset_hash, project_hash};
pub use process_map::{MIGRATED_V1_POWER_PCT, MIGRATED_V1_SPEED_MM_S};
pub use profile::{ProfileComparison, compare_profile};
pub use project::{
    AdvanceChoice, Asset, AssetStore, FrozenRevision, LibraryLink, ProductionState, Project, ProjectUuid, RowsSource,
    SerialState,
};
pub use store::{
    SaveOutcome, discard_recovery, load_project, read_recovery, recovery_path, save_project,
    save_project_keeping_previous, write_recovery,
};
