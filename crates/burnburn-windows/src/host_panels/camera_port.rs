//! The camera source of the Windows host.
//!
//! **Not built yet**: the real source (Media Foundation: device enumeration, `IMFSourceReader`
//! frames, the privacy switch of Windows) does not exist. Until it does, the host attaches
//! [`UnavailableCameraPort`], which says so honestly: no devices, access restricted, nothing can
//! be started. Tests use a fake port (`tests/w4b_camera.rs`).

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
