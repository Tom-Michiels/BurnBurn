//! The Windows camera source: [`WindowsCameraPort`] implements `burnburn_presentation::CameraPort`
//! over Media Foundation (device enumeration, an `IMFSourceReader` per streaming camera).
//!
//! **Never run**: type-checked and clippy-checked from macOS, not linked or executed on Windows,
//! and no camera was ever opened by it. Every decision (mode choice, error mapping, frame
//! conversion, the device-list cache, the permission state) is in
//! [`crate::host_panels::camera_source`] and tested headlessly; what is left here is the COM
//! plumbing. Manual proof is listed in `docs/dev-notes/G5-windows.md`.
//!
//! Invariants (why the `unsafe` here is sound):
//! - Every Media Foundation call gets the documented argument types: GUID constants from the
//!   `windows` crate for attribute keys and values, out-pointers to locals that outlive the call.
//! - `MFStartup`/`MFShutdown` are paired by [`MfSession`] (reference counted by the platform), and
//!   the capture thread initialises COM as MTA and creates, uses and releases all of its COM objects
//!   on that thread only; the UI thread (STA, initialised by `run_app`) only enumerates.
//! - The device array of `MFEnumDeviceSources` and the strings of `GetAllocatedString` are
//!   `CoTaskMemAlloc`ed; each is read once and freed with `CoTaskMemFree`.
//! - A sample buffer is locked while its bytes are copied into an owned image and unlocked after.
//! - The capture thread never unwinds into the platform: its body runs inside `catch_unwind`, and
//!   the thread is joined by `stop`/`Drop` before the port goes away.

#![allow(unsafe_code)]

use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use burnburn_presentation::{
    CameraAuthorization, CameraDevice, CameraDeviceId, CameraError, CameraFrame, CameraMode, CameraPort,
};
use burnburn_types::MonotonicTime;
use windows::Win32::Foundation::{E_ACCESSDENIED, ERROR_DEVICE_NOT_CONNECTED, ERROR_SHARING_VIOLATION};
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, IMFAttributes, IMFMediaSource, IMFMediaType, IMFSample, IMFSourceReader,
    MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
    MF_E_HW_MFT_FAILED_START_STREAMING, MF_E_INVALIDMEDIATYPE, MF_E_SHUTDOWN, MF_E_VIDEO_RECORDING_DEVICE_INVALIDATED,
    MF_E_VIDEO_RECORDING_DEVICE_PREEMPTED, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE,
    MF_MT_SUBTYPE, MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
    MF_SOURCE_READERF_ERROR, MF_VERSION, MFCreateAttributes, MFCreateMediaType, MFCreateSourceReaderFromMediaSource,
    MFEnumDeviceSources, MFMediaType_Video, MFSTARTUP_FULL, MFShutdown, MFStartup, MFVideoFormat_RGB32,
};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize};
use windows::core::{GUID, HRESULT, PWSTR};

use crate::host_panels::camera_source::{
    AccessState, DeviceCache, MfFailure, NativeMode, camera_error, camera_modes, choose_native, rgb32_frame,
    subtype_from_data1, unpack_pair,
};

/// How long the list of cameras is kept (listing opens every camera to read its modes).
const LIST_TTL_MS: u64 = 5_000;
/// How long `start` waits for the capture thread to report that streaming began.
const START_TIMEOUT: Duration = Duration::from_secs(5);
/// The capture thread wakes the UI at most this often.
const WAKE_EVERY: Duration = Duration::from_millis(100);

fn failure_of(error: &windows::core::Error) -> MfFailure {
    let code = error.code();
    let win32 = |e: windows::Win32::Foundation::WIN32_ERROR| HRESULT::from_win32(e.0);
    if code == E_ACCESSDENIED {
        MfFailure::AccessDenied
    } else if code == win32(ERROR_SHARING_VIOLATION) || code == MF_E_HW_MFT_FAILED_START_STREAMING {
        MfFailure::InUse
    } else if code == win32(ERROR_DEVICE_NOT_CONNECTED)
        || code == MF_E_VIDEO_RECORDING_DEVICE_INVALIDATED
        || code == MF_E_VIDEO_RECORDING_DEVICE_PREEMPTED
        || code == MF_E_SHUTDOWN
    {
        MfFailure::DeviceGone
    } else if code == MF_E_INVALIDMEDIATYPE {
        MfFailure::UnsupportedType
    } else {
        MfFailure::Other(code.0)
    }
}

fn typed(error: windows::core::Error) -> CameraError {
    camera_error(failure_of(&error))
}

/// `MFStartup` .. `MFShutdown` for as long as it lives.
struct MfSession;

impl MfSession {
    fn start() -> Result<MfSession, CameraError> {
        // SAFETY: no preconditions; the matching MFShutdown is in `Drop`.
        unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }.map_err(typed)?;
        Ok(MfSession)
    }
}

impl Drop for MfSession {
    fn drop(&mut self) {
        // SAFETY: pairs the successful MFStartup of `start`.
        let _ = unsafe { MFShutdown() };
    }
}

fn attributes(capacity: u32) -> Result<IMFAttributes, CameraError> {
    let mut attrs: Option<IMFAttributes> = None;
    // SAFETY: the out-pointer is a local that outlives the call.
    unsafe { MFCreateAttributes(&mut attrs, capacity) }.map_err(typed)?;
    attrs.ok_or(CameraError::Platform { code: 0 })
}

/// Every video capture device as an activation object.
fn activates() -> Result<Vec<IMFActivate>, CameraError> {
    let attrs = attributes(1)?;
    // SAFETY: documented key and value GUIDs.
    unsafe { attrs.SetGUID(&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID) }
        .map_err(typed)?;
    let mut array: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0u32;
    // SAFETY: out-pointers to locals; on success `array` holds `count` interface pointers.
    unsafe { MFEnumDeviceSources(&attrs, &mut array, &mut count) }.map_err(typed)?;
    if array.is_null() {
        return Ok(Vec::new());
    }
    // SAFETY: `array` points to `count` initialised entries that nothing else references.
    let slots = unsafe { std::slice::from_raw_parts_mut(array, count as usize) };
    let found = slots.iter_mut().filter_map(Option::take).collect();
    // SAFETY: the array was allocated by Media Foundation with CoTaskMemAlloc; its entries were taken out.
    unsafe { CoTaskMemFree(Some(array as *const c_void)) };
    Ok(found)
}

fn allocated_string(attrs: &IMFAttributes, key: &GUID) -> Option<String> {
    let mut text = PWSTR::null();
    let mut len = 0u32;
    // SAFETY: out-pointers to locals that outlive the call.
    unsafe { attrs.GetAllocatedString(key, &mut text, &mut len) }.ok()?;
    // SAFETY: on success `text` is a NUL-terminated UTF-16 string allocated by the platform.
    let value = unsafe { text.to_string() }.ok();
    // SAFETY: frees exactly what GetAllocatedString allocated.
    unsafe { CoTaskMemFree(Some(text.0 as *const c_void)) };
    value
}

fn symbolic_link(activate: &IMFActivate) -> Option<String> {
    allocated_string(activate, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK)
}

/// An opened camera: its source and a reader that converts every native type to RGB32.
struct Opened {
    source: IMFMediaSource,
    reader: IMFSourceReader,
}

impl Opened {
    fn open(activate: &IMFActivate) -> Result<Opened, CameraError> {
        // SAFETY: activation of a device source object; the interface type is the one requested.
        let source: IMFMediaSource = unsafe { activate.ActivateObject() }.map_err(typed)?;
        let attrs = attributes(1)?;
        // SAFETY: documented key; asks the reader to insert the colour converter and decoder.
        unsafe { attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1) }.map_err(typed)?;
        // SAFETY: the source and attributes are valid interfaces for the duration of the call.
        let reader = unsafe { MFCreateSourceReaderFromMediaSource(&source, &attrs) }.map_err(typed)?;
        Ok(Opened { source, reader })
    }

    /// The camera's native media types, as plain data and as the interfaces to select them with.
    fn native_types(&self) -> (Vec<NativeMode>, Vec<IMFMediaType>) {
        let stream = video_stream();
        let (mut modes, mut types) = (Vec::new(), Vec::new());
        for index in 0..512u32 {
            // SAFETY: plain COM call; the call fails once the index is past the last type.
            let Ok(media) = (unsafe { self.reader.GetNativeMediaType(stream, index) }) else { break };
            // SAFETY: documented attribute key on a valid media type.
            let size = unsafe { media.GetUINT64(&MF_MT_FRAME_SIZE) }.map(unpack_pair);
            // SAFETY: as above.
            let rate = unsafe { media.GetUINT64(&MF_MT_FRAME_RATE) }.map(unpack_pair);
            // SAFETY: as above.
            let subtype = unsafe { media.GetGUID(&MF_MT_SUBTYPE) };
            if let (Ok((width, height)), Ok((fps_num, fps_den)), Ok(subtype)) = (size, rate, subtype) {
                modes.push(NativeMode { width, height, fps_num, fps_den, subtype: subtype_from_data1(subtype.data1) });
                types.push(media);
            }
        }
        (modes, types)
    }

    /// Selects the native type, then asks for RGB32 on top of it (the reader converts).
    fn select(&self, native: &IMFMediaType) -> Result<(), CameraError> {
        let stream = video_stream();
        // SAFETY: the type comes from this reader's GetNativeMediaType.
        unsafe { self.reader.SetCurrentMediaType(stream, None, native) }.map_err(typed)?;
        // SAFETY: creates an empty media type.
        let out = unsafe { MFCreateMediaType() }.map_err(typed)?;
        // SAFETY: documented keys and values.
        unsafe { out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video) }.map_err(typed)?;
        // SAFETY: as above.
        unsafe { out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32) }.map_err(typed)?;
        // SAFETY: the reader converts its native type to the requested one.
        unsafe { self.reader.SetCurrentMediaType(stream, None, &out) }.map_err(typed)
    }

    /// Width, height and stride of the frames the reader delivers now.
    fn output(&self) -> Option<(u32, u32, i32)> {
        // SAFETY: plain COM call on a valid reader.
        let current = unsafe { self.reader.GetCurrentMediaType(video_stream()) }.ok()?;
        // SAFETY: documented attribute key.
        let (width, height) = unpack_pair(unsafe { current.GetUINT64(&MF_MT_FRAME_SIZE) }.ok()?);
        // SAFETY: as above. The stride attribute is a signed number stored as an unsigned one.
        let stride = unsafe { current.GetUINT32(&MF_MT_DEFAULT_STRIDE) }.ok().map(|s| s as i32);
        // Without the attribute RGB32 is taken as top-down: unconfirmed (manual check).
        Some((width, height, stride.unwrap_or((width as i32).saturating_mul(4))))
    }

    /// Reads the next sample; `Ok(None)` is a gap in the stream.
    fn read(&self) -> Result<Option<IMFSample>, CameraError> {
        let mut flags = 0u32;
        let mut sample: Option<IMFSample> = None;
        // SAFETY: out-pointers to locals that outlive the (blocking) call.
        unsafe { self.reader.ReadSample(video_stream(), 0, None, Some(&mut flags), None, Some(&mut sample)) }
            .map_err(typed)?;
        if flags & (MF_SOURCE_READERF_ERROR.0 as u32) != 0 {
            return Err(CameraError::DeviceGone);
        }
        Ok(sample)
    }

    fn close(self, activate: &IMFActivate) {
        // SAFETY: shuts the source down once, before the objects are released.
        let _ = unsafe { self.source.Shutdown() };
        // SAFETY: releases the device the activation object opened.
        let _ = unsafe { activate.ShutdownObject() };
    }
}

fn video_stream() -> u32 {
    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32
}

/// The bytes of a sample's contiguous buffer, copied into an image.
fn sample_image(sample: &IMFSample, width: u32, height: u32, stride: i32) -> Option<burnburn_image::SourceImage> {
    // SAFETY: plain COM call on a valid sample.
    let buffer = unsafe { sample.ConvertToContiguousBuffer() }.ok()?;
    let mut data: *mut u8 = std::ptr::null_mut();
    let mut length = 0u32;
    // SAFETY: out-pointers to locals; the buffer stays locked until the Unlock below.
    unsafe { buffer.Lock(&mut data, None, Some(&mut length)) }.ok()?;
    let image = if data.is_null() {
        None
    } else {
        // SAFETY: Lock gave `length` readable bytes at `data`, valid until Unlock.
        let bytes = unsafe { std::slice::from_raw_parts(data, length as usize) };
        rgb32_frame(width, height, stride, bytes).ok()
    };
    // SAFETY: pairs the successful Lock above.
    let _ = unsafe { buffer.Unlock() };
    image
}

/// What the capture thread hands over.
#[derive(Default)]
struct Slot {
    latest: Option<Arc<CameraFrame>>,
    seq: u64,
}

struct Running {
    device: CameraDeviceId,
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

struct Inner {
    access: Mutex<AccessState>,
    cache: Mutex<DeviceCache>,
    running: Mutex<Option<Running>>,
    slot: Arc<Mutex<Slot>>,
    wake: Arc<dyn Fn() + Send + Sync>,
    epoch: Instant,
}

/// The camera port of the Windows host. `wake` is called (from the capture thread, at most ten
/// times a second) when a new frame is waiting; it must be thread-safe (a `PostMessageW`).
#[derive(Clone)]
pub struct WindowsCameraPort {
    inner: Arc<Inner>,
}

impl WindowsCameraPort {
    pub fn new(wake: Arc<dyn Fn() + Send + Sync>) -> WindowsCameraPort {
        WindowsCameraPort {
            inner: Arc::new(Inner {
                access: Mutex::new(AccessState::default()),
                cache: Mutex::new(DeviceCache::new(LIST_TTL_MS)),
                running: Mutex::new(None),
                slot: Arc::new(Mutex::new(Slot::default())),
                wake,
                epoch: Instant::now(),
            }),
        }
    }

    fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn stop_running(&self) {
        let running = Self::lock(&self.inner.running).take();
        if let Some(running) = running {
            running.stop.store(true, Ordering::SeqCst);
            let _ = running.thread.join();
        }
        Self::lock(&self.inner.slot).latest = None;
    }
}

/// The cameras with their modes (opens every camera briefly).
fn list_devices() -> Result<Vec<CameraDevice>, CameraError> {
    let _session = MfSession::start()?;
    let mut devices = Vec::new();
    for activate in activates()? {
        let (Some(link), name) =
            (symbolic_link(&activate), allocated_string(&activate, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME))
        else {
            continue;
        };
        // A camera another app has open cannot be listed: it stays in the list without modes.
        let modes = Opened::open(&activate)
            .map(|opened| {
                let modes = camera_modes(&opened.native_types().0);
                opened.close(&activate);
                modes
            })
            .unwrap_or_default();
        devices.push(CameraDevice { id: CameraDeviceId(link.clone()), name: name.unwrap_or(link), modes });
    }
    Ok(devices)
}

/// Opens `device`, selects `mode`, tells `ready` how that went and streams until `stop`.
fn capture(
    device: CameraDeviceId,
    mode: CameraMode,
    stop: Arc<AtomicBool>,
    slot: Arc<Mutex<Slot>>,
    wake: Arc<dyn Fn() + Send + Sync>,
    epoch: Instant,
    ready: Sender<Result<(), CameraError>>,
) {
    // SAFETY: this thread becomes an MTA COM thread for as long as it lives (balanced below).
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let session = MfSession::start();
    let setup = || -> Result<(IMFActivate, Opened), CameraError> {
        session.as_ref().map_err(Clone::clone)?;
        let activate = activates()?
            .into_iter()
            .find(|a| symbolic_link(a).as_deref() == Some(device.0.as_str()))
            .ok_or(CameraError::DeviceGone)?;
        let opened = Opened::open(&activate)?;
        let (modes, types) = opened.native_types();
        let index = choose_native(&mode, &modes)?;
        opened.select(types.get(index).ok_or(CameraError::ModeUnsupported)?)?;
        Ok((activate, opened))
    };
    match setup() {
        Err(e) => {
            let _ = ready.send(Err(e));
        }
        Ok((activate, opened)) => {
            let _ = ready.send(Ok(()));
            let mut last_wake = Instant::now() - WAKE_EVERY;
            while !stop.load(Ordering::SeqCst) {
                let Ok(sample) = opened.read() else { break };
                let Some(sample) = sample else { continue };
                let Some((width, height, stride)) = opened.output() else { continue };
                let Some(image) = sample_image(&sample, width, height, stride) else { continue };
                let mut slot = Slot::lock_in(&slot);
                slot.seq += 1;
                slot.latest = Some(Arc::new(CameraFrame {
                    device: device.clone(),
                    mode,
                    seq: slot.seq,
                    image,
                    time: MonotonicTime::from_offset(epoch.elapsed()),
                }));
                drop(slot);
                if last_wake.elapsed() >= WAKE_EVERY {
                    last_wake = Instant::now();
                    wake();
                }
            }
            opened.close(&activate);
        }
    }
    drop(session);
    if com {
        // SAFETY: balances the successful CoInitializeEx above.
        unsafe { CoUninitialize() };
    }
}

impl Slot {
    fn lock_in(slot: &Mutex<Slot>) -> std::sync::MutexGuard<'_, Slot> {
        slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl CameraPort for WindowsCameraPort {
    fn devices(&self) -> Result<Vec<CameraDevice>, CameraError> {
        let streaming = Self::lock(&self.inner.running).is_some();
        let now = u64::try_from(self.inner.epoch.elapsed().as_millis()).unwrap_or(u64::MAX);
        Self::lock(&self.inner.cache).get(now, streaming, list_devices)
    }

    fn authorization(&self) -> CameraAuthorization {
        Self::lock(&self.inner.access).authorization()
    }

    fn request_authorization(&self) {
        Self::lock(&self.inner.access).request();
        Self::lock(&self.inner.cache).invalidate();
    }

    fn start(&self, device: &CameraDeviceId, mode: &CameraMode) -> Result<(), CameraError> {
        self.stop_running();
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = channel();
        let (device_id, mode_copy) = (device.clone(), *mode);
        let (slot, wake, epoch) = (Arc::clone(&self.inner.slot), Arc::clone(&self.inner.wake), self.inner.epoch);
        let stop_for_thread = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("burnburn-camera".to_owned())
            .spawn(move || {
                // No panic may unwind out of the capture thread into the platform.
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    capture(device_id, mode_copy, stop_for_thread, slot, wake, epoch, ready_tx)
                }));
            })
            .map_err(|_| CameraError::Platform { code: 0 })?;
        let result = match ready_rx.recv_timeout(START_TIMEOUT) {
            Ok(result) => result,
            // The thread died (a panic) or never answered.
            Err(_) => Err(CameraError::Platform { code: 0 }),
        };
        Self::lock(&self.inner.access).note_start(&result);
        match result {
            Ok(()) => {
                *Self::lock(&self.inner.running) = Some(Running { device: device.clone(), stop, thread });
                Ok(())
            }
            Err(e) => {
                stop.store(true, Ordering::SeqCst);
                let _ = thread.join();
                if e == CameraError::DeviceGone {
                    Self::lock(&self.inner.cache).invalidate();
                }
                Err(e)
            }
        }
    }

    fn stop(&self, device: &CameraDeviceId) {
        let is_running = Self::lock(&self.inner.running).as_ref().is_some_and(|r| r.device == *device);
        if is_running {
            self.stop_running();
        }
    }

    fn latest_frame(&self, device: &CameraDeviceId) -> Option<Arc<CameraFrame>> {
        let running = Self::lock(&self.inner.running).as_ref().is_some_and(|r| r.device == *device);
        running.then(|| Self::lock(&self.inner.slot).latest.clone()).flatten()
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        let running = self.running.get_mut().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        if let Some(running) = running {
            running.stop.store(true, Ordering::SeqCst);
            let _ = running.thread.join();
        }
    }
}
