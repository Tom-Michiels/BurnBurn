//! wgpu rendering of a [`RenderPacket`]: offscreen (tests, thumbnails) or into a host surface.
//!
//! Colours are written as given into a non-sRGB 8-bit target (`Rgba8Unorm` offscreen, the
//! surface's non-sRGB format on screen), with straight-alpha blending.

use crate::packet::RenderPacket;
use crate::tess::{Vertex, tessellate};

/// The offscreen target format.
pub const OFFSCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Why rendering could not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuError {
    /// No GPU adapter on this machine for the native backends (a skip, never a pass).
    NoAdapter(String),
    /// The device could not be opened.
    Device(String),
    /// Recording, submitting or reading back failed.
    Render(String),
    /// The device is gone; open a new [`GpuContext`].
    DeviceLost,
}

impl std::fmt::Display for GpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GpuError::NoAdapter(e) => write!(f, "no GPU adapter: {e}"),
            GpuError::Device(e) => write!(f, "the GPU device could not be opened: {e}"),
            GpuError::Render(e) => write!(f, "GPU rendering failed: {e}"),
            GpuError::DeviceLost => f.write_str("the GPU device was lost"),
        }
    }
}

impl std::error::Error for GpuError {}

/// The native backends this build supports (Metal on macOS, DX12 on Windows; none elsewhere).
pub fn native_backends() -> wgpu::Backends {
    if cfg!(target_os = "macos") {
        wgpu::Backends::METAL
    } else if cfg!(target_os = "windows") {
        wgpu::Backends::DX12
    } else {
        wgpu::Backends::empty()
    }
}

/// A wgpu instance for the native backends, without a display handle.
pub fn instance() -> wgpu::Instance {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = native_backends();
    wgpu::Instance::new(desc)
}

/// An adapter, device and queue.
pub struct GpuContext {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    lost: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl GpuContext {
    /// Open a device without a window.
    pub fn headless() -> Result<Self, GpuError> {
        Self::open(&instance(), None)
    }

    /// Open a device that can present to `surface` (created from the same `instance`).
    pub fn open(instance: &wgpu::Instance, surface: Option<&wgpu::Surface<'_>>) -> Result<Self, GpuError> {
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: surface,
            apply_limit_buckets: false,
        }))
        .map_err(|e| GpuError::NoAdapter(e.to_string()))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("burnburn canvas"),
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .map_err(|e| GpuError::Device(e.to_string()))?;
        let lost = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = std::sync::Arc::clone(&lost);
        device.set_device_lost_callback(move |_reason, _message| {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        Ok(GpuContext { adapter, device, queue, lost })
    }

    /// The device was lost (driver reset, GPU removed, or destroyed). A lost context never
    /// recovers: the host opens a new one and recreates its surface; the document is untouched.
    pub fn is_lost(&self) -> bool {
        self.lost.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// A picture read back from the GPU: tightly packed RGBA8 rows, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = ((y as usize) * (self.width as usize) + x as usize) * 4;
        let p = self.rgba.get(i..i + 4)?;
        Some([p[0], p[1], p[2], p[3]])
    }
}

/// The canvas pipeline for one colour target format.
pub struct CanvasRenderer {
    pipeline: wgpu::RenderPipeline,
    format: wgpu::TextureFormat,
}

/// Bytes per vertex: position (2 × f32) then colour (4 × f32).
const VERTEX_STRIDE: u64 = 24;

impl CanvasRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("burnburn canvas"),
            source: wgpu::ShaderSource::Wgsl(include_str!("canvas.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("burnburn canvas"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let buffer = wgpu::VertexBufferLayout {
            array_stride: VERTEX_STRIDE,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("burnburn canvas"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[Some(buffer)],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
            depth_stencil: None,
            multisample: wgpu::MultisampleState { count: 1, mask: !0, alpha_to_coverage_enabled: false },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        CanvasRenderer { pipeline, format }
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// Record a pass that clears `target` to the packet's clear colour and draws its primitives.
    /// `target` must have the packet's size and this renderer's format.
    pub fn encode(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        packet: &RenderPacket,
    ) {
        let vertices = tessellate(packet);
        let bytes = clip_space_bytes(&vertices, packet.size_px);
        let buffer = (!bytes.is_empty()).then(|| {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("burnburn canvas vertices"),
                size: bytes.len() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&buffer, 0, &bytes);
            buffer
        });
        let c = packet.clear;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("burnburn canvas"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: c.r.into(),
                        g: c.g.into(),
                        b: c.b.into(),
                        a: c.a.into(),
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if let Some(buffer) = &buffer {
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
    }
}

/// Vertex bytes with positions converted from viewport-relative device pixels (y down) to clip
/// space (y up): x / w · 2 − 1, 1 − y / h · 2.
fn clip_space_bytes(vertices: &[Vertex], [w, h]: [u32; 2]) -> Vec<u8> {
    let (w, h) = (w.max(1) as f32, h.max(1) as f32);
    let mut out = Vec::with_capacity(vertices.len() * VERTEX_STRIDE as usize);
    for v in vertices {
        let [x, y] = v.position_px;
        for f in [x / w * 2.0 - 1.0, 1.0 - y / h * 2.0].iter().chain(&v.color) {
            out.extend_from_slice(&f.to_ne_bytes());
        }
    }
    out
}

/// Render `packet` into an offscreen texture of its size and read it back.
pub fn render_offscreen(ctx: &GpuContext, packet: &RenderPacket) -> Result<Image, GpuError> {
    if ctx.is_lost() {
        return Err(GpuError::DeviceLost);
    }
    let (device, queue) = (&ctx.device, &ctx.queue);
    let [width, height] = packet.size_px;
    if width == 0 || height == 0 {
        return Err(GpuError::Render("an empty picture has no pixels".into()));
    }
    let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("burnburn offscreen"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: OFFSCREEN_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let renderer = CanvasRenderer::new(device, OFFSCREEN_FORMAT);
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("burnburn offscreen") });
    renderer.encode(device, queue, &mut encoder, &view, packet);
    let row = width * 4;
    let padded = row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("burnburn offscreen read"),
        size: u64::from(padded) * u64::from(height),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &read,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        size,
    );
    let submission = queue.submit([encoder.finish()]);
    if let Some(e) = pollster::block_on(scope.pop()) {
        return Err(GpuError::Render(e.to_string()));
    }
    let slice = read.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .map_err(|e| GpuError::Render(format!("waiting for the picture: {e}")))?;
    match rx.try_recv() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(GpuError::Render(format!("reading the picture back: {e}"))),
        Err(_) => return Err(GpuError::Render("the picture was not mapped after waiting".into())),
    }
    let data = slice.get_mapped_range().map_err(|e| GpuError::Render(format!("reading the picture back: {e}")))?;
    let mut rgba = Vec::with_capacity((row as usize) * (height as usize));
    for r in data.chunks(padded as usize).take(height as usize) {
        rgba.extend_from_slice(r.get(..row as usize).unwrap_or_default());
    }
    if rgba.len() != (row as usize) * (height as usize) {
        return Err(GpuError::Render("the picture read back short".into()));
    }
    Ok(Image { width, height, rgba })
}
