//! The headless half of the Windows camera source (Media Foundation): turning what MF reports into
//! the presentation types (modes, errors, authorization) and its frames into images. The Media
//! Foundation calls themselves are in `win32::camera` (Windows only, never run in tests); every
//! decision they make lives here and is tested on every OS.

use burnburn_image::SourceImage;
use burnburn_presentation::{CameraAuthorization, CameraDevice, CameraError, CameraMode, CameraPixelFormat};

/// A pixel format as Media Foundation names it (`MF_MT_SUBTYPE`; the GUID's first field).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NativeSubtype {
    Rgb24,
    Argb32,
    Rgb32,
    Nv12,
    Yuy2,
    Mjpg,
    Other(u32),
}

/// One native media type of a camera (`GetNativeMediaType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeMode {
    pub width: u32,
    pub height: u32,
    /// `MF_MT_FRAME_RATE` as numerator / denominator.
    pub fps_num: u32,
    pub fps_den: u32,
    pub subtype: NativeSubtype,
}

/// The subtype behind the first field of an `MFVideoFormat_*` GUID (a D3DFMT number for the RGB
/// formats, a FourCC for the others).
pub fn subtype_from_data1(data1: u32) -> NativeSubtype {
    const NV12: u32 = u32::from_le_bytes(*b"NV12");
    const YUY2: u32 = u32::from_le_bytes(*b"YUY2");
    const MJPG: u32 = u32::from_le_bytes(*b"MJPG");
    match data1 {
        20 => NativeSubtype::Rgb24,
        21 => NativeSubtype::Argb32,
        22 => NativeSubtype::Rgb32,
        NV12 => NativeSubtype::Nv12,
        YUY2 => NativeSubtype::Yuy2,
        MJPG => NativeSubtype::Mjpg,
        other => NativeSubtype::Other(other),
    }
}

/// The shared pixel format a native subtype is reported as.
pub fn camera_format(subtype: NativeSubtype) -> CameraPixelFormat {
    match subtype {
        NativeSubtype::Rgb32 | NativeSubtype::Argb32 => CameraPixelFormat::Bgra8,
        NativeSubtype::Rgb24 => CameraPixelFormat::Rgb24,
        NativeSubtype::Nv12 => CameraPixelFormat::Yuv420Video,
        NativeSubtype::Yuy2 => CameraPixelFormat::Yuy2,
        NativeSubtype::Mjpg => CameraPixelFormat::Other(u32::from_le_bytes(*b"MJPG")),
        NativeSubtype::Other(fourcc) => CameraPixelFormat::Other(fourcc),
    }
}

/// `MF_MT_FRAME_SIZE` / `MF_MT_FRAME_RATE`: two 32-bit halves packed in a `UINT64` (high, low).
pub fn unpack_pair(packed: u64) -> (u32, u32) {
    ((packed >> 32) as u32, packed as u32)
}

/// The frame rate in thousandths of a frame per second (`None`: no usable rate).
fn fps_milli(num: u32, den: u32) -> Option<u32> {
    if num == 0 || den == 0 {
        return None;
    }
    u32::try_from(u64::from(num) * 1000 / u64::from(den)).ok()
}

/// The modes a camera offers, in the shared type: frame rate in thousandths, duplicates dropped,
/// biggest picture first, then the highest frame rate.
pub fn camera_modes(native: &[NativeMode]) -> Vec<CameraMode> {
    let mut modes: Vec<CameraMode> = Vec::new();
    for n in native {
        let Some(fps) = fps_milli(n.fps_num, n.fps_den).filter(|_| n.width > 0 && n.height > 0) else { continue };
        let mode =
            CameraMode { width: n.width, height: n.height, fps_milli: fps, pixel_format: camera_format(n.subtype) };
        if !modes.contains(&mode) {
            modes.push(mode);
        }
    }
    modes
        .sort_by_key(|m| (std::cmp::Reverse(u64::from(m.width) * u64::from(m.height)), std::cmp::Reverse(m.fps_milli)));
    modes
}

/// The native media type that serves `requested` (same size and format, frame rate within half a
/// frame per second): its index in `native`. `ModeUnsupported` when none does.
pub fn choose_native(requested: &CameraMode, native: &[NativeMode]) -> Result<usize, CameraError> {
    native
        .iter()
        .enumerate()
        .filter(|(_, n)| {
            n.width == requested.width
                && n.height == requested.height
                && camera_format(n.subtype) == requested.pixel_format
        })
        .filter_map(|(i, n)| {
            let off = fps_milli(n.fps_num, n.fps_den)?.abs_diff(requested.fps_milli);
            (off <= 500).then_some((off, i))
        })
        .min()
        .map(|(_, i)| i)
        .ok_or(CameraError::ModeUnsupported)
}

/// Why a Media Foundation call failed, in the cases the host tells apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MfFailure {
    /// `E_ACCESSDENIED`: the Windows privacy switch for the camera is off for this app.
    AccessDenied,
    /// Another app has the camera.
    InUse,
    /// Unplugged, or invalidated/preempted while streaming.
    DeviceGone,
    /// The media type is not supported.
    UnsupportedType,
    /// Any other `HRESULT`.
    Other(i32),
}

pub fn camera_error(failure: MfFailure) -> CameraError {
    match failure {
        MfFailure::AccessDenied => CameraError::PermissionDenied,
        MfFailure::InUse => CameraError::Busy,
        MfFailure::DeviceGone => CameraError::DeviceGone,
        MfFailure::UnsupportedType => CameraError::ModeUnsupported,
        MfFailure::Other(code) => CameraError::Platform { code: i64::from(code) },
    }
}

/// What the host knows about the camera permission. Windows shows no prompt to desktop apps: the
/// privacy switch is simply on or off, and only a refused start reveals that it is off.
#[derive(Debug, Default, Clone, Copy)]
pub struct AccessState {
    denied: bool,
}

impl AccessState {
    pub fn authorization(&self) -> CameraAuthorization {
        if self.denied { CameraAuthorization::Denied } else { CameraAuthorization::Authorized }
    }

    /// The outcome of a start: a refusal for permission marks access denied, a success clears it.
    pub fn note_start(&mut self, result: &Result<(), CameraError>) {
        match result {
            Ok(()) => self.denied = false,
            Err(CameraError::PermissionDenied) => self.denied = true,
            Err(_) => {}
        }
    }

    /// "Request access": there is no prompt on Windows, so this only forgets an earlier denial
    /// and lets the next start find out again (after the user switched the privacy setting on).
    pub fn request(&mut self) {
        self.denied = false;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// Size, stride and buffer length do not describe an RGB32 frame.
    BadLayout,
}

/// An RGB32 frame (memory order B, G, R, X; MF leaves X undefined) → opaque RGBA image. `stride` is
/// the distance between rows in bytes; a negative stride is a bottom-up picture (the first row in
/// memory is the bottom one).
pub fn rgb32_frame(width: u32, height: u32, stride: i32, data: &[u8]) -> Result<SourceImage, FrameError> {
    let row = (width as usize).checked_mul(4).ok_or(FrameError::BadLayout)?;
    let pitch = stride.unsigned_abs() as usize;
    let rows = height as usize;
    let needed =
        pitch.checked_mul(rows.saturating_sub(1)).and_then(|n| n.checked_add(row)).ok_or(FrameError::BadLayout)?;
    if width == 0 || height == 0 || stride == 0 || pitch < row || data.len() < needed {
        return Err(FrameError::BadLayout);
    }
    let mut rgba = Vec::with_capacity(row * rows);
    for y in 0..rows {
        let source_row = if stride < 0 { rows - 1 - y } else { y };
        let start = source_row * pitch;
        for px in data[start..start + row].as_chunks::<4>().0 {
            rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
    }
    SourceImage::from_rgba8(width, height, rgba).map_err(|_| FrameError::BadLayout)
}

/// The device list with its age. Listing a camera's modes opens the camera, and the controller
/// asks for the list on every refresh: the list is kept for a while, and kept as it is while a
/// camera is streaming (the open camera cannot be listed again).
#[derive(Debug, Clone)]
pub struct DeviceCache {
    ttl_ms: u64,
    stored: Option<(u64, Vec<CameraDevice>)>,
}

impl DeviceCache {
    pub fn new(ttl_ms: u64) -> DeviceCache {
        DeviceCache { ttl_ms, stored: None }
    }

    /// The list at `now_ms`: the stored one while it is fresh (or while `streaming`), otherwise
    /// `list()`. A failed listing is returned and not stored.
    pub fn get(
        &mut self,
        now_ms: u64,
        streaming: bool,
        list: impl FnOnce() -> Result<Vec<CameraDevice>, CameraError>,
    ) -> Result<Vec<CameraDevice>, CameraError> {
        if let Some((at, devices)) = &self.stored
            && (streaming || now_ms.saturating_sub(*at) < self.ttl_ms)
        {
            return Ok(devices.clone());
        }
        let devices = list()?;
        self.stored = Some((now_ms, devices.clone()));
        Ok(devices)
    }

    /// Forget the stored list (a request for access, a start that found the device gone).
    pub fn invalidate(&mut self) {
        self.stored = None;
    }
}
