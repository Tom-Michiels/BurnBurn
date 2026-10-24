//! The widgets of the document window, one file per panel: each panel's struct, its builder and the
//! `DocWindow` method that copies its view model into the widgets. `window.rs` assembles them.

pub(crate) mod arrange;
// `define_class!` and `msg_send!` need unsafe (the CAM dialog panel has its own small target object).
pub(crate) mod art;
pub(crate) mod bundle;
#[allow(unsafe_code)]
pub(crate) mod cam_dialog;
pub(crate) mod camera;
pub(crate) mod camera_window;
pub(crate) mod cut_settings;
// The intent target of the empty-state and cancel buttons is a `define_class!` (LF17).
#[allow(unsafe_code)]
pub(crate) mod empty_state;
pub(crate) mod export;
pub(crate) mod files;
pub(crate) mod files_ui;
// The form builder calls `setTarget` on the controls it wires (LF5).
#[allow(unsafe_code)]
pub mod form;
// The sheets and panels that hold built forms create windows and point controls at targets (LF6b).
#[allow(unsafe_code)]
pub(crate) mod form_surface;
// `define_class!` and `msg_send!` need unsafe (the golf-5 panels have their own small target object).
#[allow(unsafe_code)]
pub(crate) mod g5;
#[allow(unsafe_code)]
pub(crate) mod hub;
// `define_class!` and `msg_send!` need unsafe (the GRBL window has its own small target object).
#[allow(unsafe_code)]
pub(crate) mod grbl_window;
pub(crate) mod image;
pub(crate) mod import;
pub(crate) mod inspector;
pub(crate) mod laser;
pub(crate) mod layer_cells;
pub(crate) mod material;
pub(crate) mod operations;
pub(crate) mod priority;
pub(crate) mod production;
// `define_class!` and `msg_send!` need unsafe (the Schikken panel has its own small target object).
#[allow(unsafe_code)]
pub(crate) mod schikken;
// `define_class!` and `msg_send!` need unsafe (the shape panel has its own small target object).
#[allow(unsafe_code)]
pub(crate) mod shapes;
pub(crate) mod sketch;
// The text panels (W3-UI-tekst): `textform` has the shared target object (`define_class!`).
pub(crate) mod barcode;
pub(crate) mod text;
#[allow(unsafe_code)]
pub(crate) mod textform;
pub(crate) mod tracer;
pub(crate) mod variable;
pub(crate) mod widgets;
