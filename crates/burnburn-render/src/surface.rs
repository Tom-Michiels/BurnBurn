//! A host window's canvas surface with recovery, shared by the macOS and Windows hosts.
//!
//! The host supplies a function that creates the `wgpu::Surface` from its native view (that needs
//! `unsafe` and lives in the host); this type owns the device, the pipeline and the surface
//! configuration and decides what to do when a frame cannot be acquired:
//! a lost surface is recreated, a lost device reopened (with a new surface), an outdated surface
//! reconfigured, an occluded one skipped until the host redraws. None of this touches the
//! document: the next frame is rebuilt from the same scene.

use crate::gpu::{CanvasRenderer, GpuContext, GpuError, instance};
use crate::packet::RenderPacket;

/// What acquiring a frame reported, without the texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcquireStatus {
    Ok,
    Suboptimal,
    Timeout,
    Occluded,
    Outdated,
    Lost,
    Validation,
}

/// The recovery step for an acquire status (and whether the device itself is lost).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    /// Draw this frame (reconfigure afterwards if suboptimal).
    Draw { reconfigure: bool },
    /// Nothing on screen to draw into right now; the host redraws on the next visibility change.
    Skip,
    /// Configure the surface again and retry once.
    Reconfigure,
    /// Create a new surface on the same device and retry once.
    RecreateSurface,
    /// Open a new device and a new surface and retry once.
    RecreateDevice,
    /// Report and drop this frame.
    Fail,
}

/// The recovery policy (pure; tested without a GPU).
pub fn recovery(status: AcquireStatus, device_lost: bool) -> Recovery {
    if device_lost {
        return Recovery::RecreateDevice;
    }
    match status {
        AcquireStatus::Ok => Recovery::Draw { reconfigure: false },
        AcquireStatus::Suboptimal => Recovery::Draw { reconfigure: true },
        AcquireStatus::Timeout | AcquireStatus::Occluded => Recovery::Skip,
        AcquireStatus::Outdated => Recovery::Reconfigure,
        AcquireStatus::Lost => Recovery::RecreateSurface,
        AcquireStatus::Validation => Recovery::Fail,
    }
}

#[derive(Debug)]
pub enum CanvasError {
    Surface(String),
    Gpu(GpuError),
    Frame(String),
}

impl std::fmt::Display for CanvasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CanvasError::Surface(e) => write!(f, "no canvas surface: {e}"),
            CanvasError::Gpu(e) => write!(f, "{e}"),
            CanvasError::Frame(e) => write!(f, "no frame: {e}"),
        }
    }
}

impl std::error::Error for CanvasError {}

/// Creates a surface for the host's native view from the given instance.
pub type SurfaceFactory = Box<dyn FnMut(&wgpu::Instance) -> Result<wgpu::Surface<'static>, String>>;

/// What one [`SurfaceCanvas::draw`] call did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DrawReport {
    pub presented: bool,
    pub surfaces_created: u32,
    pub devices_opened: u32,
}

pub struct SurfaceCanvas {
    // Field order: the surface is dropped before the context and before the host's view.
    surface: Option<wgpu::Surface<'static>>,
    ctx: GpuContext,
    renderer: CanvasRenderer,
    config: wgpu::SurfaceConfiguration,
    instance: wgpu::Instance,
    factory: SurfaceFactory,
    frames: u64,
    surfaces_created: u64,
}

fn surface_format(caps: &wgpu::SurfaceCapabilities) -> Option<wgpu::TextureFormat> {
    // Non-sRGB, like the offscreen target, so colours mean the same in both (ADR 0003).
    caps.formats.iter().copied().find(|f| !f.is_srgb()).or(caps.formats.first().copied())
}

impl SurfaceCanvas {
    /// Create the surface and a device that can present to it.
    pub fn new(mut factory: SurfaceFactory) -> Result<Self, CanvasError> {
        let instance = instance();
        let surface = factory(&instance).map_err(CanvasError::Surface)?;
        let ctx = GpuContext::open(&instance, Some(&surface)).map_err(CanvasError::Gpu)?;
        let (renderer, config) = Self::configure_for(&ctx, &surface)?;
        Ok(SurfaceCanvas {
            surface: Some(surface),
            ctx,
            renderer,
            config,
            instance,
            factory,
            frames: 0,
            surfaces_created: 1,
        })
    }

    fn configure_for(
        ctx: &GpuContext,
        surface: &wgpu::Surface<'_>,
    ) -> Result<(CanvasRenderer, wgpu::SurfaceConfiguration), CanvasError> {
        let caps = surface.get_capabilities(&ctx.adapter);
        let format = surface_format(&caps).ok_or(CanvasError::Surface("the surface offers no format".into()))?;
        let mut config = surface
            .get_default_config(&ctx.adapter, 1, 1)
            .ok_or(CanvasError::Surface("not supported by the adapter".into()))?;
        config.format = format;
        config.present_mode = wgpu::PresentMode::Fifo;
        Ok((CanvasRenderer::new(&ctx.device, format), config))
    }

    /// Frames presented so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Surfaces created so far (1 until a loss was recovered).
    pub fn surfaces_created(&self) -> u64 {
        self.surfaces_created
    }

    pub fn context(&self) -> &GpuContext {
        &self.ctx
    }

    /// Drop the surface as if the OS had taken it away (host tests of the recovery path).
    pub fn simulate_surface_loss(&mut self) {
        self.surface = None;
    }

    fn recreate_surface(&mut self) -> Result<(), CanvasError> {
        self.surface = None;
        let surface = (self.factory)(&self.instance).map_err(CanvasError::Surface)?;
        surface.configure(&self.ctx.device, &self.config);
        self.surface = Some(surface);
        self.surfaces_created += 1;
        Ok(())
    }

    fn recreate_device(&mut self) -> Result<(), CanvasError> {
        self.surface = None;
        let surface = (self.factory)(&self.instance).map_err(CanvasError::Surface)?;
        let ctx = GpuContext::open(&self.instance, Some(&surface)).map_err(CanvasError::Gpu)?;
        let (renderer, mut config) = Self::configure_for(&ctx, &surface)?;
        config.width = self.config.width;
        config.height = self.config.height;
        surface.configure(&ctx.device, &config);
        (self.ctx, self.renderer, self.config, self.surface) = (ctx, renderer, config, Some(surface));
        self.surfaces_created += 1;
        Ok(())
    }

    /// Draw `packet`, recovering at most once from a lost/outdated surface or a lost device.
    pub fn draw(&mut self, packet: &RenderPacket) -> Result<DrawReport, CanvasError> {
        let mut report = DrawReport::default();
        let [w, h] = packet.size_px;
        if (self.config.width, self.config.height) != (w, h) {
            self.config.width = w;
            self.config.height = h;
            if let Some(surface) = &self.surface {
                surface.configure(&self.ctx.device, &self.config);
            }
        }
        for attempt in 0..2 {
            let (status, frame) = match &self.surface {
                None => (AcquireStatus::Lost, None),
                Some(surface) => match surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(t) => (AcquireStatus::Ok, Some(t)),
                    wgpu::CurrentSurfaceTexture::Suboptimal(t) => (AcquireStatus::Suboptimal, Some(t)),
                    wgpu::CurrentSurfaceTexture::Timeout => (AcquireStatus::Timeout, None),
                    wgpu::CurrentSurfaceTexture::Occluded => (AcquireStatus::Occluded, None),
                    wgpu::CurrentSurfaceTexture::Outdated => (AcquireStatus::Outdated, None),
                    wgpu::CurrentSurfaceTexture::Lost => (AcquireStatus::Lost, None),
                    _ => (AcquireStatus::Validation, None),
                },
            };
            let step = recovery(status, self.ctx.is_lost());
            match (step, frame) {
                (Recovery::Draw { reconfigure }, Some(frame)) => {
                    let view = frame.texture.create_view(&Default::default());
                    let mut encoder = self
                        .ctx
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("burnburn frame") });
                    self.renderer.encode(&self.ctx.device, &self.ctx.queue, &mut encoder, &view, packet);
                    self.ctx.queue.submit([encoder.finish()]);
                    self.ctx.queue.present(frame);
                    if reconfigure && let Some(surface) = &self.surface {
                        surface.configure(&self.ctx.device, &self.config);
                    }
                    self.frames += 1;
                    report.presented = true;
                    return Ok(report);
                }
                (Recovery::Skip, _) => return Ok(report),
                _ if attempt == 1 => break,
                (Recovery::Reconfigure, _) => {
                    if let Some(surface) = &self.surface {
                        surface.configure(&self.ctx.device, &self.config);
                    }
                }
                (Recovery::RecreateSurface, _) => {
                    self.recreate_surface()?;
                    report.surfaces_created += 1;
                }
                (Recovery::RecreateDevice, _) => {
                    self.recreate_device()?;
                    report.surfaces_created += 1;
                    report.devices_opened += 1;
                }
                (Recovery::Draw { .. } | Recovery::Fail, _) => break,
            }
        }
        Err(CanvasError::Frame("no frame after recovery".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_policy_per_acquire_status() {
        use AcquireStatus as S;
        assert_eq!(recovery(S::Ok, false), Recovery::Draw { reconfigure: false });
        assert_eq!(recovery(S::Suboptimal, false), Recovery::Draw { reconfigure: true });
        assert_eq!(recovery(S::Occluded, false), Recovery::Skip);
        assert_eq!(recovery(S::Timeout, false), Recovery::Skip);
        assert_eq!(recovery(S::Outdated, false), Recovery::Reconfigure);
        assert_eq!(recovery(S::Lost, false), Recovery::RecreateSurface);
        assert_eq!(recovery(S::Validation, false), Recovery::Fail);
        // A lost device wins over whatever the surface says: everything is recreated.
        for s in [S::Ok, S::Suboptimal, S::Occluded, S::Timeout, S::Outdated, S::Lost, S::Validation] {
            assert_eq!(recovery(s, true), Recovery::RecreateDevice, "{s:?}");
        }
    }
}
