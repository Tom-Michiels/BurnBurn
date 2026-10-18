//! IM27 camera adapter over AVFoundation (feature `camera`): [`AvCamera`] implements
//! `burnburn_presentation::CameraPort`. It is compiled and clippy-checked, but **never opened by
//! automatic tests**: asking for access (`request_authorization`) shows the macOS privacy prompt
//! and is only called on the user's click (manual proof in `docs/dev-notes/IM27.md`); attaching
//! it reads the permission state and the device list, nothing else. The pure conversions it uses
//! are tested headlessly in `crate::camera`.
//!
//! Invariants (why the `unsafe` here is sound):
//! - Every AVFoundation call gets arguments of the documented types (`AVMediaTypeVideo` is the
//!   media type constant; the dictionary passed as `videoSettings` maps the CoreVideo key string
//!   `PixelFormatType` (= `kCVPixelBufferPixelFormatTypeKey`) to an `NSNumber`).
//! - The sample-buffer delegate runs on our own serial dispatch queue, never the main thread; it
//!   only reads the buffer while its base address is locked and copies the pixels out before
//!   unlocking. It shares state with the main thread only through `Arc<Mutex<…>>`.
//! - The delegate object and its queue are retained by `AvCamera` for as long as the session
//!   runs; `stop` stops the session before the delegate is released (`Drop` does the same).
//! - `AvCamera` is `Send + Sync` (the port contract): its Objective-C objects live in `Running`
//!   behind a `Mutex`, so only one thread touches them at a time. `AVCaptureSession`'s
//!   `startRunning`/`stopRunning` may be called from any thread (Apple documents them as blocking
//!   calls best made off the main thread); the output, delegate and queue are only retained there.
//! - Delegate bodies run in `ffi_guard`: no panic unwinds into AVFoundation.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use burnburn_presentation::{
    CameraAuthorization, CameraDevice, CameraDeviceId, CameraError, CameraFrame, CameraMode, CameraPixelFormat,
    CameraPort,
};
use burnburn_types::MonotonicTime;
use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AnyThread, DefinedClass, define_class, msg_send};
use objc2_av_foundation::{
    AVCaptureConnection, AVCaptureDevice, AVCaptureDeviceInput, AVCaptureOutput, AVCaptureSession,
    AVCaptureVideoDataOutput, AVCaptureVideoDataOutputSampleBufferDelegate, AVMediaTypeVideo,
};
use objc2_core_media::{CMSampleBuffer, CMVideoFormatDescriptionGetDimensions};
use objc2_core_video::{
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString};

use super::classes::ffi_guard;
use crate::camera::{bgra_frame, camera_access};

/// `kCVPixelFormatType_32BGRA`.
const BGRA: u32 = u32::from_be_bytes(*b"BGRA");

/// What the capture queue hands to the main thread.
#[derive(Default)]
struct Shared {
    latest: Option<Arc<CameraFrame>>,
    /// Frames delivered so far (`CameraFrame::seq`).
    seq: u64,
}

pub(crate) struct DelegateIvars {
    shared: Arc<Mutex<Shared>>,
    device: CameraDeviceId,
    epoch: Instant,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "BurnBurnCameraDelegate"]
    #[ivars = DelegateIvars]
    pub(crate) struct FrameDelegate;

    unsafe impl NSObjectProtocol for FrameDelegate {}

    unsafe impl AVCaptureVideoDataOutputSampleBufferDelegate for FrameDelegate {
        #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
        fn did_output(
            &self,
            _output: &AVCaptureOutput,
            sample_buffer: &CMSampleBuffer,
            _connection: &AVCaptureConnection,
        ) {
            ffi_guard("captureOutput:didOutputSampleBuffer:fromConnection:", (), || {
                if let Some(mut frame) = self.copy_frame(sample_buffer)
                    && let Ok(mut shared) = self.ivars().shared.lock()
                {
                    shared.seq += 1;
                    frame.seq = shared.seq;
                    shared.latest = Some(Arc::new(frame));
                }
            });
        }
    }
);

impl FrameDelegate {
    fn new(shared: Arc<Mutex<Shared>>, device: CameraDeviceId) -> Retained<Self> {
        let this = Self::alloc().set_ivars(DelegateIvars { shared, device, epoch: Instant::now() });
        // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
        unsafe { msg_send![super(this), init] }
    }

    /// Copy the BGRA pixels out of the sample buffer (locked only for the copy).
    #[allow(unsafe_code)]
    fn copy_frame(&self, sample_buffer: &CMSampleBuffer) -> Option<CameraFrame> {
        // SAFETY: a valid sample buffer AVFoundation passed to this callback.
        let image = unsafe { sample_buffer.image_buffer() }?;
        let flags = CVPixelBufferLockFlags::ReadOnly;
        // SAFETY: locking a valid pixel buffer read-only; unlocked below on every path.
        if unsafe { CVPixelBufferLockBaseAddress(&image, flags) } != 0 {
            return None;
        }
        let (width, height) = (CVPixelBufferGetWidth(&image), CVPixelBufferGetHeight(&image));
        let bytes_per_row = CVPixelBufferGetBytesPerRow(&image);
        let fourcc = CVPixelBufferGetPixelFormatType(&image);
        let base = CVPixelBufferGetBaseAddress(&image).cast::<u8>();
        let frame = (fourcc == BGRA && !base.is_null())
            .then(|| {
                let len = bytes_per_row.checked_mul(height)?;
                // SAFETY: while locked, `base` points to `bytes_per_row × height` readable bytes of
                // this non-planar BGRA buffer; the slice is dropped before unlocking.
                let data = unsafe { std::slice::from_raw_parts(base, len) };
                bgra_frame(u32::try_from(width).ok()?, u32::try_from(height).ok()?, bytes_per_row, data).ok()
            })
            .flatten();
        // SAFETY: balances the lock above with the same flags.
        unsafe { CVPixelBufferUnlockBaseAddress(&image, flags) };
        let image = frame?;
        let mode = CameraMode {
            width: image.width(),
            height: image.height(),
            fps_milli: 0,
            pixel_format: CameraPixelFormat::Bgra8,
        };
        Some(CameraFrame {
            device: self.ivars().device.clone(),
            mode,
            seq: 0,
            image,
            time: MonotonicTime::from_offset(self.ivars().epoch.elapsed()),
        })
    }
}

struct Running {
    device: CameraDeviceId,
    session: Retained<AVCaptureSession>,
    _output: Retained<AVCaptureVideoDataOutput>,
    _delegate: Retained<FrameDelegate>,
    _queue: dispatch2::DispatchRetained<DispatchQueue>,
}

// SAFETY: see the module invariants: `Running` is only reached through `AvCamera.running`'s
// mutex, and the only messages sent to its objects off their creating thread are
// `stopRunning` (thread-safe) and release.
#[allow(unsafe_code)]
unsafe impl Send for Running {}

/// AVFoundation camera port (`Send + Sync`; see the module invariants).
pub struct AvCamera {
    shared: Arc<Mutex<Shared>>,
    running: Mutex<Option<Running>>,
    /// Devices from the last enumeration (enumerated again after a permission answer).
    devices: Mutex<Option<Vec<CameraDevice>>>,
}

impl Default for AvCamera {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(unsafe_code)]
fn video() -> Option<&'static NSString> {
    // SAFETY: reading an immutable AVFoundation constant.
    unsafe { AVMediaTypeVideo }
}

impl AvCamera {
    pub fn new() -> Self {
        AvCamera {
            shared: Arc::new(Mutex::new(Shared::default())),
            running: Mutex::new(None),
            devices: Mutex::new(None),
        }
    }

    /// The current camera permission; reading it never shows the prompt.
    #[allow(unsafe_code)]
    pub fn access() -> CameraAuthorization {
        let Some(video) = video() else { return CameraAuthorization::Restricted };
        // SAFETY: `video` is AVMediaTypeVideo, one of the two media types this accepts.
        let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(video) };
        camera_access(status.0)
    }

    #[allow(unsafe_code)]
    /// One mode per frame size, with its highest frame rate. The output's `videoSettings` ask for
    /// BGRA, which AVFoundation delivers for every native format, so every mode is offered as BGRA.
    fn modes(device: &AVCaptureDevice) -> Vec<CameraMode> {
        let mut modes: Vec<CameraMode> = Vec::new();
        // SAFETY: a device AVFoundation returned; formats are AVCaptureDeviceFormat objects.
        for format in unsafe { device.formats() }.iter() {
            // SAFETY: as above; the description of a video format is a video format description.
            let description = unsafe { format.formatDescription() };
            // SAFETY: a valid video format description.
            let size = unsafe { CMVideoFormatDescriptionGetDimensions(&description) };
            // SAFETY: as above; frame-rate ranges of a video format.
            let fps = unsafe { format.videoSupportedFrameRateRanges() }
                .iter()
                // SAFETY: a plain property read of a range AVFoundation returned.
                .map(|r| unsafe { r.maxFrameRate() })
                .fold(0.0_f64, f64::max);
            if let (Ok(width), Ok(height)) = (u32::try_from(size.width), u32::try_from(size.height)) {
                let fps_milli = (fps * 1000.0).round().clamp(0.0, f64::from(u32::MAX)) as u32;
                match modes.iter_mut().find(|m| (m.width, m.height) == (width, height)) {
                    Some(m) => m.fps_milli = m.fps_milli.max(fps_milli),
                    None => modes.push(CameraMode { width, height, fps_milli, pixel_format: CameraPixelFormat::Bgra8 }),
                }
            }
        }
        modes
    }
}

impl CameraPort for AvCamera {
    fn devices(&self) -> Result<Vec<CameraDevice>, CameraError> {
        let mut cache = self.devices.lock().map_err(|_| CameraError::Busy)?;
        Ok(cache.get_or_insert_with(Self::enumerate).clone())
    }

    fn authorization(&self) -> CameraAuthorization {
        Self::access()
    }

    /// Shows the macOS privacy prompt (only ever called on the user's click). The answer arrives
    /// on an arbitrary queue; the next enumeration then sees the devices with their new state.
    #[allow(unsafe_code)]
    fn request_authorization(&self) {
        let Some(video) = video() else { return };
        let handler = block2::RcBlock::new(|_granted: Bool| {});
        // SAFETY: AVMediaTypeVideo and a completion block that does nothing but return.
        unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(video, &handler) };
        if let Ok(mut cache) = self.devices.lock() {
            *cache = None;
        }
    }

    /// Starts capturing in BGRA. Without permission it refuses (`PermissionDenied`) instead of
    /// prompting: asking is an explicit user action ([`CameraPort::request_authorization`]).
    #[allow(unsafe_code)]
    fn start(&self, device: &CameraDeviceId, mode: &CameraMode) -> Result<(), CameraError> {
        if Self::access() != CameraAuthorization::Authorized {
            return Err(CameraError::PermissionDenied);
        }
        self.stop(device);
        // SAFETY: looking up a device by the unique id AVFoundation gave us.
        let Some(capture) = (unsafe { AVCaptureDevice::deviceWithUniqueID(&NSString::from_str(&device.0)) }) else {
            return Err(CameraError::DeviceGone);
        };
        if !Self::modes(&capture).iter().any(|m| (m.width, m.height) == (mode.width, mode.height)) {
            return Err(CameraError::ModeUnsupported);
        }
        // SAFETY: an input for the device we just looked up.
        let input = unsafe { AVCaptureDeviceInput::deviceInputWithDevice_error(&capture) }
            .map_err(|e| CameraError::Platform { code: e.code() as i64 })?;
        // SAFETY: a fresh session; the input and output below belong to it.
        let session = unsafe { AVCaptureSession::new() };
        // SAFETY: as above.
        if !unsafe { session.canAddInput(&input) } {
            return Err(CameraError::Busy);
        }
        // SAFETY: as above.
        unsafe { session.addInput(&input) };
        // SAFETY: a fresh output.
        let output = unsafe { AVCaptureVideoDataOutput::new() };
        let format: Retained<AnyObject> =
            Retained::into_super(Retained::into_super(Retained::into_super(NSNumber::new_u32(BGRA))));
        let settings: Retained<NSDictionary<NSString, AnyObject>> =
            NSDictionary::from_retained_objects(&[&*NSString::from_str("PixelFormatType")], &[format]);
        // SAFETY: videoSettings with the CoreVideo pixel-format key and an NSNumber (invariants).
        unsafe { output.setVideoSettings(Some(&settings)) };
        // SAFETY: plain property write.
        unsafe { output.setAlwaysDiscardsLateVideoFrames(true) };
        let delegate = FrameDelegate::new(self.shared.clone(), device.clone());
        let queue = DispatchQueue::new("app.burnburn.camera", None);
        // SAFETY: a serial queue (required) and our delegate, both kept in `Running`.
        unsafe { output.setSampleBufferDelegate_queue(Some(ProtocolObject::from_ref(&*delegate)), Some(&queue)) };
        // SAFETY: the output belongs to this session only.
        unsafe { session.addOutput(&output) };
        // SAFETY: starting a configured session (blocks briefly; called on an explicit user action).
        unsafe { session.startRunning() };
        let running = Running { device: device.clone(), session, _output: output, _delegate: delegate, _queue: queue };
        *self.running.lock().map_err(|_| CameraError::Busy)? = Some(running);
        Ok(())
    }

    fn latest_frame(&self, device: &CameraDeviceId) -> Option<Arc<CameraFrame>> {
        self.shared.lock().ok()?.latest.clone().filter(|f| f.device == *device)
    }

    #[allow(unsafe_code)]
    fn stop(&self, device: &CameraDeviceId) {
        let Ok(mut running) = self.running.lock() else { return };
        if running.as_ref().is_some_and(|r| r.device == *device)
            && let Some(r) = running.take()
        {
            // SAFETY: stopping our own running session before its delegate is released.
            unsafe { r.session.stopRunning() };
        }
    }
}

impl AvCamera {
    #[allow(unsafe_code, deprecated)]
    fn enumerate() -> Vec<CameraDevice> {
        let Some(video) = video() else { return Vec::new() };
        // SAFETY: AVMediaTypeVideo; enumerating devices needs no permission and opens nothing.
        let devices = unsafe { AVCaptureDevice::devicesWithMediaType(video) };
        devices
            .iter()
            .map(|d| CameraDevice {
                // SAFETY: plain property reads of a device AVFoundation returned.
                id: CameraDeviceId(unsafe { d.uniqueID() }.to_string()),
                // SAFETY: as above.
                name: unsafe { d.localizedName() }.to_string(),
                modes: Self::modes(&d),
            })
            .collect()
    }
}

impl Drop for AvCamera {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        if let Ok(mut running) = self.running.lock()
            && let Some(r) = running.take()
        {
            // SAFETY: as in `stop`.
            unsafe { r.session.stopRunning() };
        }
    }
}
