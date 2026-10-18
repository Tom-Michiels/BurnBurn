//! IM27 bed camera port (contracts-phase-c §IM27), moved here from the interim
//! `burnburn_macos::camera`. Pull model like tasks: hosts implement the port with AVFoundation /
//! Media Foundation and wake the UI when a frame arrives; presentation only reads the latest
//! frame and shows what it is (a visual placement aid; camera pixels never feed CAM).

use std::sync::Arc;

use burnburn_image::SourceImage;
use burnburn_types::MonotonicTime;

use crate::view_model::MessageKey;

/// Stable OS identity of a camera (`AVCaptureDevice.uniqueID`, MF symbolic link).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CameraDeviceId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CameraPixelFormat {
    Bgra8,
    /// 4:2:0 bi-planar video range (`420v` / NV12).
    Yuv420Video,
    Yuy2,
    Rgb24,
    /// Any other FourCC.
    Other(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CameraMode {
    pub width: u32,
    pub height: u32,
    pub fps_milli: u32,
    pub pixel_format: CameraPixelFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraDevice {
    pub id: CameraDeviceId,
    pub name: String,
    pub modes: Vec<CameraMode>,
}

/// Camera permission as the OS reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CameraAuthorization {
    /// Asking shows the OS prompt: only on an explicit user action, never in tests.
    NotDetermined,
    Denied,
    Restricted,
    Authorized,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CameraError {
    PermissionDenied,
    DeviceGone,
    ModeUnsupported,
    Busy,
    Platform { code: i64 },
}

/// One captured frame with the configuration it was taken in (calibration validity).
#[derive(Debug, Clone)]
pub struct CameraFrame {
    pub device: CameraDeviceId,
    pub mode: CameraMode,
    pub seq: u64,
    pub image: SourceImage,
    pub time: MonotonicTime,
}

/// Implemented by the hosts. Never blocking; `request_authorization` returns at once and the
/// host wakes the UI when the OS answered.
pub trait CameraPort: Send + Sync {
    fn devices(&self) -> Result<Vec<CameraDevice>, CameraError>;
    fn authorization(&self) -> CameraAuthorization;
    fn request_authorization(&self);
    fn start(&self, device: &CameraDeviceId, mode: &CameraMode) -> Result<(), CameraError>;
    fn stop(&self, device: &CameraDeviceId);
    fn latest_frame(&self, device: &CameraDeviceId) -> Option<Arc<CameraFrame>>;
}

/// Camera panel actions (`UiIntent::BedCamera`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BedCameraIntent {
    /// The user asks to use the camera: the only moment the OS prompt may appear.
    RequestAccess,
    Start {
        device: CameraDeviceId,
        mode: CameraMode,
    },
    Stop,
}

/// What the panel shows of the latest frame (the host draws the pixels it holds itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraFrameInfo {
    pub seq: u64,
    pub time: MonotonicTime,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraViewModel {
    pub authorization: CameraAuthorization,
    pub devices: Vec<CameraDevice>,
    pub devices_error: Option<CameraError>,
    pub active: Option<(CameraDeviceId, CameraMode)>,
    pub frame: Option<CameraFrameInfo>,
    /// Always "visuele plaatsing": camera pixels are never production geometry.
    pub note: MessageKey,
}
