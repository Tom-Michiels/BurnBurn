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

use burnburn_render::RenderPacket;
use burnburn_render::surface::{CanvasError, DrawReport, SurfaceCanvas};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::NSView;
use raw_window_handle::{AppKitDisplayHandle, AppKitWindowHandle, RawDisplayHandle, RawWindowHandle};

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
