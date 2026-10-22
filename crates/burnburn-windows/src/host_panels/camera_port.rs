//! The camera port for hosts without a camera source.
//!
//! The Windows host attaches `win32::camera::WindowsCameraPort` (Media Foundation; G5-windows).
//! [`UnavailableCameraPort`] is what remains for a host build without it, and it says so honestly:
//! no devices, access restricted, nothing can be started. Tests use a fake port
//! (`tests/w4b_camera.rs`); the Media Foundation decisions are tested in `camera_source`.

use std::sync::Arc;

use burnburn_presentation::{
    CameraAuthorization, CameraDevice, CameraDeviceId, CameraError, CameraFrame, CameraMode, CameraPort,
};

/// A camera port without any camera.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnavailableCameraPort;

impl CameraPort for UnavailableCameraPort {
    fn devices(&self) -> Result<Vec<CameraDevice>, CameraError> {
        Ok(Vec::new())
    }
    fn authorization(&self) -> CameraAuthorization {
        CameraAuthorization::Restricted
    }
    fn request_authorization(&self) {}
    fn start(&self, _device: &CameraDeviceId, _mode: &CameraMode) -> Result<(), CameraError> {
        Err(CameraError::DeviceGone)
    }
    fn stop(&self, _device: &CameraDeviceId) {}
    fn latest_frame(&self, _device: &CameraDeviceId) -> Option<Arc<CameraFrame>> {
        None
    }
}
