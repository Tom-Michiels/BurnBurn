//! The bed camera (IM27): the port, its device and mode, and the camera panel.

use std::sync::Arc;

use crate::camera::{BedCameraIntent, CameraAuthorization, CameraFrameInfo, CameraPort, CameraViewModel};
use crate::view_model::MessageKey;

use super::{AppController, AppUpdate, error};

impl AppController {
    /// The bed camera (IM27); hosts implement the port.
    pub fn attach_camera(&mut self, camera: Arc<dyn CameraPort>) {
        self.camera = Some(camera);
        self.camera_active = None;
    }
}

impl AppController {
    /// Camera panel, when a camera port is attached.
    pub fn camera_view(&self) -> Option<CameraViewModel> {
        let camera = self.camera.as_ref()?;
        let (devices, devices_error) = match camera.devices() {
            Ok(d) => (d, None),
            Err(e) => (Vec::new(), Some(e)),
        };
        let frame = self
            .camera_active
            .as_ref()
            .and_then(|(device, _)| camera.latest_frame(device))
            .map(|f| CameraFrameInfo { seq: f.seq, time: f.time, width: f.image.width(), height: f.image.height() });
        Some(CameraViewModel {
            authorization: camera.authorization(),
            devices,
            devices_error,
            active: self.camera_active.clone(),
            frame,
            note: MessageKey::VisualPlacementOnly,
        })
    }

    pub(super) fn bed_camera(&mut self, intent: BedCameraIntent) -> AppUpdate {
        let Some(camera) = self.camera.clone() else { return error(None, MessageKey::CommandRejected) };
        let changed = AppUpdate { view_changed: true, ..AppUpdate::default() };
        match intent {
            BedCameraIntent::RequestAccess => {
                camera.request_authorization();
                changed
            }
            BedCameraIntent::Start { device, mode } => {
                if camera.authorization() != CameraAuthorization::Authorized {
                    return error(None, MessageKey::CameraNotAuthorized);
                }
                if let Some((old, _)) = self.camera_active.take() {
                    camera.stop(&old);
                }
                match camera.start(&device, &mode) {
                    Ok(()) => {
                        self.camera_active = Some((device, mode));
                        changed
                    }
                    Err(_) => error(None, MessageKey::CommandRejected),
                }
            }
            BedCameraIntent::Stop => {
                if let Some((device, _)) = self.camera_active.take() {
                    camera.stop(&device);
                }
                changed
            }
        }
    }
}
