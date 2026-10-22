//! The canvas NSView's Metal surface: the host's part of `burnburn_render::surface::SurfaceCanvas`.
//!
//! Invariants for the `unsafe` surface creation:
//! - Surfaces are created from the view on the main thread only (the factory runs inside
//!   `SurfaceCanvas::new`/`draw`, which the host calls from AppKit callbacks on the main thread).
//! - The factory closure owns a retained reference to the view, and `SurfaceCanvas` declares its
//!   surface before the factory, so every surface is dropped before the view can go away.
//! - wgpu's Metal backend adds its own `CAMetalLayer` to the view and keeps its size and
//!   `contentsScale` in step with the view; this code only reconfigures the drawable size.

use std::ptr::NonNull;

use burnburn_editor::SceneSnapshot;
use burnburn_render::surface::{CanvasError, DrawReport, SurfaceCanvas};
use burnburn_render::{RenderPacket, Theme, build_packet};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSAppearanceCustomization, NSView};
use raw_window_handle::{AppKitDisplayHandle, AppKitWindowHandle, RawDisplayHandle, RawWindowHandle};

/// What the canvas draws for `scene`: the light theme, outlines only in wireframe.
pub fn canvas_packet(scene: &SceneSnapshot, wireframe: bool) -> RenderPacket {
    let theme = if wireframe { Theme::light().wireframe() } else { Theme::light() };
    build_packet(scene, &theme)
}

/// As [`canvas_packet`], with curves flattened to within `tolerance_px` device pixels (the
/// display-quality preference).
pub fn canvas_packet_with(scene: &SceneSnapshot, wireframe: bool, tolerance_px: f64) -> RenderPacket {
    let theme = if wireframe { Theme::light().wireframe() } else { Theme::light() };
    build_packet(scene, &theme.with_tolerance_px(tolerance_px))
}

/// The canvas theme for the window's appearance: light or dark, outlines only in wireframe, curves
/// flattened to within `tolerance_px` device pixels.
pub fn canvas_theme(dark: bool, wireframe: bool, tolerance_px: f64) -> Theme {
    let theme = if dark { Theme::dark() } else { Theme::light() }.with_tolerance_px(tolerance_px);
    if wireframe { theme.wireframe() } else { theme }
}

/// As [`canvas_packet_with`], in the light or the dark canvas theme.
pub fn canvas_packet_themed(scene: &SceneSnapshot, wireframe: bool, tolerance_px: f64, dark: bool) -> RenderPacket {
    build_packet(scene, &canvas_theme(dark, wireframe, tolerance_px))
}

/// Whether `view` is in the dark appearance (system setting or the window's own appearance).
pub(crate) fn is_dark(view: &NSView) -> bool {
    view.effectiveAppearance().name().to_string().contains("Dark")
}

pub(crate) struct Canvas {
    inner: SurfaceCanvas,
}

impl Canvas {
    /// Put a Metal surface on `view` (which must be in a window).
    #[allow(unsafe_code)]
    pub(crate) fn attach(_mtm: MainThreadMarker, view: Retained<NSView>) -> Result<Canvas, CanvasError> {
        let factory = Box::new(move |instance: &wgpu::Instance| {
            let ptr = NonNull::from(&*view).cast::<std::ffi::c_void>();
            let target = wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(RawDisplayHandle::AppKit(AppKitDisplayHandle::new())),
                raw_window_handle: RawWindowHandle::AppKit(AppKitWindowHandle::new(ptr)),
            };
            // SAFETY: `ptr` is a live NSView retained by this closure, which `SurfaceCanvas` keeps
            // and drops after its surface (module invariants); we are on the main thread.
            unsafe { instance.create_surface_unsafe(target) }.map_err(|e| e.to_string())
        });
        Ok(Canvas { inner: SurfaceCanvas::new(factory)? })
    }

    pub(crate) fn frames(&self) -> u64 {
        self.inner.frames()
    }

    pub(crate) fn surfaces_created(&self) -> u64 {
        self.inner.surfaces_created()
    }

    pub(crate) fn draw(&mut self, packet: &RenderPacket) -> Result<DrawReport, CanvasError> {
        self.inner.draw(packet)
    }

    pub(crate) fn simulate_surface_loss(&mut self) {
        self.inner.simulate_surface_loss();
    }
}
