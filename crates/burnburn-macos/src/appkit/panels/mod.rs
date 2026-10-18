//! The widgets of the document window, one file per panel: each panel's struct, its builder and the
//! `DocWindow` method that copies its view model into the widgets. `window.rs` assembles them.

pub(crate) mod camera;
pub(crate) mod cut_settings;
pub(crate) mod import;
pub(crate) mod operations;
pub(crate) mod production;
pub(crate) mod sketch;
pub(crate) mod widgets;
