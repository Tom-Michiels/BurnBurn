//! The shared BurnBurn canvas: scene translation and wgpu rendering.
//!
//! [`scene::build_packet`] turns a `burnburn_editor::SceneSnapshot` (world millimetres, `f64`) into
//! a [`RenderPacket`] of viewport-relative device pixels (`f32`). Only the final, already
//! viewport-relative coordinates are narrowed to `f32`, so large world coordinates keep their
//! precision on screen. [`gpu`] draws a packet with wgpu, offscreen or into a host's surface;
//! [`surface`] is the host-surface lifecycle (loss and device recovery) shared by both hosts.
//!
//! This crate has no AppKit or Windows dependency; the hosts provide the native surface.

pub mod gpu;
pub mod input;
pub mod packet;
pub mod scene;
pub mod surface;
pub mod tess;

pub use input::Rgba;
pub use packet::{ImageCell, Primitive, RenderPacket};
pub use scene::{
    CanvasChrome, LAYER_COLOURS, RoleStyle, SnapLabels, Theme, build_packet, build_packet_with, canvas_text_missing_chars,
    contrast_ratio, delta_e, size_px,
};
