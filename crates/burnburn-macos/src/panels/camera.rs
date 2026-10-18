//! The bed-camera section.

use burnburn_presentation::{
    BedCameraIntent, CameraAuthorization, CameraMode, CameraPixelFormat, CameraViewModel, UiIntent,
};

use crate::l10n::{HostText, Locale, host_text, message_or_key};

/// Buttons of the bed-camera section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraControl {
    RequestAccess,
    Start,
    Stop,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CameraPanel {
    pub status: String,
    /// Device names for the pop-up.
    pub devices: Vec<String>,
    /// Pop-up item of the running device.
    pub selected: Option<usize>,
    pub frame: String,
    /// "Visual placement only" (camera pixels never feed CAM).
    pub note: String,
    pub access_enabled: bool,
    pub start_enabled: bool,
    pub stop_enabled: bool,
}

pub fn camera_panel(vm: Option<&CameraViewModel>, locale: Locale) -> CameraPanel {
    let Some(c) = vm else {
        return CameraPanel { status: host_text(HostText::CameraNoPort, locale).to_owned(), ..CameraPanel::default() };
    };
    let authorized = c.authorization == CameraAuthorization::Authorized;
    let status = match c.authorization {
        _ if c.active.is_some() => HostText::CameraRunning,
        _ if authorized && c.devices.is_empty() => HostText::CameraNoDevices,
        CameraAuthorization::Authorized => HostText::CameraAuthorized,
        CameraAuthorization::NotDetermined => HostText::CameraNotDetermined,
        CameraAuthorization::Denied => HostText::CameraDenied,
        CameraAuthorization::Restricted => HostText::CameraRestricted,
    };
    let frame = match (&c.active, c.frame) {
        (Some(_), Some(f)) => match locale {
            Locale::Nl => format!("Beeld {}: {} × {} px", f.seq, f.width, f.height),
            Locale::En => format!("Frame {}: {} × {} px", f.seq, f.width, f.height),
        },
        (Some(_), None) => host_text(HostText::CameraNoFrame, locale).to_owned(),
        (None, _) => String::new(),
    };
    CameraPanel {
        status: host_text(status, locale).to_owned(),
        devices: c.devices.iter().map(|d| d.name.clone()).collect(),
        selected: c.active.as_ref().and_then(|(id, _)| c.devices.iter().position(|d| d.id == *id)),
        frame,
        note: message_or_key(c.note, locale),
        access_enabled: c.authorization == CameraAuthorization::NotDetermined,
        start_enabled: authorized && c.active.is_none() && c.devices.iter().any(|d| bgra_mode(&d.modes).is_some()),
        stop_enabled: c.active.is_some(),
    }
}

/// The capture mode the host's adapters decode: the largest BGRA mode.
fn bgra_mode(modes: &[CameraMode]) -> Option<CameraMode> {
    modes
        .iter()
        .filter(|m| m.pixel_format == CameraPixelFormat::Bgra8)
        .max_by_key(|m| u64::from(m.width) * u64::from(m.height))
        .copied()
}

/// A click on a camera button with pop-up item `device_index` chosen; disabled → nothing.
pub fn camera_intent(vm: Option<&CameraViewModel>, control: CameraControl, device_index: isize) -> Option<UiIntent> {
    let panel = camera_panel(vm, Locale::En);
    let c = vm?;
    let intent = match control {
        CameraControl::RequestAccess if panel.access_enabled => BedCameraIntent::RequestAccess,
        CameraControl::Start if panel.start_enabled => {
            let device = c.devices.get(usize::try_from(device_index).ok()?)?;
            BedCameraIntent::Start { device: device.id.clone(), mode: bgra_mode(&device.modes)? }
        }
        CameraControl::Stop if panel.stop_enabled => BedCameraIntent::Stop,
        _ => return None,
    };
    Some(UiIntent::BedCamera(intent))
}
