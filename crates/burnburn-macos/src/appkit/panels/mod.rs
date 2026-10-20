//! The widgets of the document window, one file per panel: each panel's struct, its builder and the
//! `DocWindow` method that copies its view model into the widgets. `window.rs` assembles them.

pub(crate) mod arrange;
// `define_class!` and `msg_send!` need unsafe (the CAM dialog panel has its own small target object).
#[allow(unsafe_code)]
pub(crate) mod cam_dialog;
pub(crate) mod camera;
pub(crate) mod cut_settings;
pub(crate) mod image;
pub(crate) mod import;
pub(crate) mod inspector;
pub(crate) mod laser;
pub(crate) mod operations;
pub(crate) mod priority;
pub(crate) mod production;
// `define_class!` and `msg_send!` need unsafe (the shape panel has its own small target object).
#[allow(unsafe_code)]
pub(crate) mod shapes;
pub(crate) mod sketch;
pub(crate) mod tracer;
pub(crate) mod widgets;
