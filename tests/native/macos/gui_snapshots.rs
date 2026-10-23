//! Opt-in review tool, not a check: renders every host window, panel and sheet it can open to a
//! PNG **offscreen**, for the look-and-feel review (`docs/plan/look-and-feel.md`, LF0).
//!
//! It only runs when `BURNBURN_GUI_SNAPSHOT_DIR` names an output directory; otherwise it reports
//! "ignored" and passes, so `cargo test` and `cargo xtask verify` never produce files. The windows
//! are always quiet (alpha 0, click-through, never active): nothing appears on screen and the
//! user's focus is never taken. Views are drawn with `cacheDisplayInRect:toBitmapImageRep:`,
//! which ignores the window alpha. The Metal canvas does not draw into such a cache, so its
//! pixels come from `burnburn_render::gpu::render_offscreen` of the same scene and are composited
//! into the window image at the canvas view's frame.
//!
//! Variables: `BURNBURN_GUI_SNAPSHOT_THEME=light|dark` (default light) and
//! `BURNBURN_GUI_SNAPSHOT_LOCALE=nl|en` (default nl). File names: `<nn>-<step>-<window>.png`.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("gui_snapshots: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    let Some(dir) = std::env::var_os("BURNBURN_GUI_SNAPSHOT_DIR") else {
        println!(
            "test gui_snapshots ... ignored (set BURNBURN_GUI_SNAPSHOT_DIR to render)\n\ntest result: ok. 0 passed; 0 failed; 1 ignored"
        );
        return ExitCode::SUCCESS;
    };
    common::report("gui_snapshots", snapshots::run(std::path::PathBuf::from(dir)))
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod snapshots {
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use std::ptr;

    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::appkit::{HostOptions, canvas_packet_themed};
    use burnburn_macos::l10n::Locale;
    use burnburn_presentation::{CommandKey, UiIntent};
    use burnburn_render::gpu::{GpuContext, render_offscreen};
    use burnburn_types::DocumentId;
    use objc2::rc::Retained;
    use objc2::{AnyThread, Message};
    use objc2_app_kit::{
        NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication, NSBitmapImageFileType,
        NSBitmapImageRep, NSDeviceRGBColorSpace, NSGraphicsContext, NSView, NSWindow,
    };
    use objc2_foundation::{NSDictionary, NSRect};

    use crate::common::windows;
    use crate::common::{Session, start_with};

    struct Shooter {
        dir: PathBuf,
        prefix: String,
        count: usize,
        gpu: Option<GpuContext>,
        dark: bool,
    }

    pub fn run(dir: PathBuf) -> Result<(), String> {
        let dark = std::env::var("BURNBURN_GUI_SNAPSHOT_THEME").is_ok_and(|t| t == "dark");
        let en = std::env::var("BURNBURN_GUI_SNAPSHOT_LOCALE").is_ok_and(|l| l == "en");
        let prefix = format!("{}-{}", if dark { "dark" } else { "light" }, if en { "en" } else { "nl" });
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let s = start_with(|o: &mut HostOptions| {
            o.quiet_windows = true;
            o.float_windows = false;
            o.locale = Some(if en { Locale::En } else { Locale::Nl });
        })?;
        let app = NSApplication::sharedApplication(s.mtm);
        // SAFETY: AppKit's immutable appearance-name constants.
        let name = unsafe { if dark { NSAppearanceNameDarkAqua } else { NSAppearanceNameAqua } };
        app.setAppearance(NSAppearance::appearanceNamed(name).as_deref());
        let gpu = match GpuContext::headless() {
            Ok(g) => Some(g),
            Err(e) => {
                println!("    no GPU for the canvas ({e:?}): window images show an empty canvas");
                None
            }
        };
        let mut shot = Shooter { dir, prefix, count: 0, gpu, dark };

        let doc = s.new_document()?;
        shot.document(&s, doc, "empty-document")?;
        // A machine profile (fixture dialect, never a real laser) so plan and preview can open.
        s.app.dispatch(UiIntent::SetMachineProfile {
            profile: Box::new(windows::machine_profile()?),
            ruida: burnburn_ruida::RuidaProfile {
                magic: burnburn_ruida::Magic::FIXTURE_0X88,
                overscan: burnburn_ruida::OverscanPolicy::ControllerManaged,
            },
        });
        s.draw_rectangle(doc, (10.0, 10.0), (90.0, 60.0))?;
        s.menu(MenuItem::Command(CommandKey::ToolEllipse))?;
        s.drag_mm(doc, (110.0, 20.0), (170.0, 70.0))?;
        s.menu(MenuItem::Command(CommandKey::ToolLine)).ok();
        s.drag_mm(doc, (20.0, 80.0), (160.0, 110.0)).ok();
        s.menu(MenuItem::Command(CommandKey::ToolSelect))?;
        s.pump(0.3);
        shot.document(&s, doc, "drawing")?;
        s.app.dispatch(UiIntent::AddOperation { document: doc, draft: windows::cut_draft()? });
        s.pump(0.3);
        // Select the rectangle with a click on its edge (the Select All item has no menu tag).
        if let Some(d) = s.doc(doc) {
            let at = d.camera.world_to_points(&d.viewport, burnburn_geometry::Point::new(10.0, 35.0));
            s.mouse(doc, objc2_app_kit::NSEventType::LeftMouseDown, at)?;
            s.mouse(doc, objc2_app_kit::NSEventType::LeftMouseUp, at)?;
        }
        s.pump(0.3);
        shot.document(&s, doc, "selection")?;

        // About is the system's standard panel (Q5): it cannot be laid out without being shown, so
        // no offscreen shot; its options are checked in app_panels.rs.

        // Cut Settings for the first layer.
        if let Some(op) = s.doc(doc).and_then(|d| d.operations.first().map(|o| o.id)) {
            s.app.dispatch(UiIntent::OpenCutSettings { document: doc, operation: op });
            shot.new_windows(&s, doc, "cut-settings");
        }

        let steps = windows::steps();
        // Compile once, before the plan-dependent panels.
        if s.menu(MenuItem::Command(CommandKey::Compile)).is_ok() {
            let _ = s.wait(5.0, "the plan", || s.doc(doc).is_some_and(|d| d.plan.is_some()));
            shot.document(&s, doc, "compiled")?;
        }
        for (step, item) in steps {
            if let Err(e) = s.menu(item) {
                println!("    {step}: {e}");
                continue;
            }
            shot.new_windows(&s, doc, step);
        }
        shot.file_sheets(&s, doc);
        shot.document(&s, doc, "after-compile-sketch")?;
        println!("    {} images in {}", shot.count, shot.dir.display());
        Ok(())
    }

    impl Shooter {
        /// The import, bitmap, Adjust Image, tracer and bundle-import sheets (LF6b): each is opened
        /// the way the user does (a chosen file), shot, and closed through its own command.
        fn file_sheets(&mut self, s: &Session, doc: DocumentId) {
            use burnburn_macos::adapter::NativeEvent;
            let close = |command: CommandKey| {
                s.app.dispatch(UiIntent::Invoke { document: Some(doc), command });
                s.pump(0.4);
            };
            let svg = s.dir.join("vierkant.svg");
            let svg_text = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40mm\" height=\"20mm\" \
                viewBox=\"0 0 40 20\"><rect width=\"40\" height=\"20\" fill=\"none\" stroke=\"black\"/></svg>";
            if std::fs::write(&svg, svg_text).is_ok() {
                s.app.native(NativeEvent::ImportFileChosen { document: doc, path: Some(svg) });
                self.new_windows_keep(s, doc, "import-svg");
                close(CommandKey::CancelImport);
            }
            let pixels: Vec<u8> = (0..40).flat_map(|_| (0..60).map(|x| if x < 30 { 0u8 } else { 255 })).collect();
            let Ok(png) = burnburn_image::encode_png_gray8(60, 40, &pixels) else { return };
            let path = s.dir.join("twee-tinten.png");
            if std::fs::write(&path, png).is_err() {
                return;
            }
            s.app.native(NativeEvent::ImportFileChosen { document: doc, path: Some(path.clone()) });
            let _ = s.wait(5.0, "the bitmap import sheet", || {
                s.app.with_window(doc, |w| w.image_import_sheet.is_some()).unwrap_or(false)
            });
            if let Some(dpi) = s.app.with_window(doc, |w| w.image_dpi.retain()) {
                dpi.setStringValue(&objc2_foundation::NSString::from_str("254"));
                // SAFETY: the field's own action to its own target, as Return sends it.
                unsafe { dpi.sendAction_to(dpi.action(), dpi.target().as_deref()) };
                s.pump(0.4);
            }
            self.new_windows_keep(s, doc, "image-import");
            // Import it, so the Adjust Image sheet and the tracer have a picture.
            if let Some(commit) = s.app.with_window(doc, |w| w.image_commit.retain()) {
                // SAFETY: clicking our own button with no sender, as the user does.
                unsafe { commit.performClick(None) };
            }
            let _ = s.wait(3.0, "the sheet to close", || {
                s.app.with_window(doc, |w| w.window.attachedSheet().is_none()).unwrap_or(false)
            });
            s.pump(0.5);
            if let Some(id) = s.doc(doc).and_then(|d| d.objects.last().map(|o| o.id)) {
                s.app.dispatch(UiIntent::Select {
                    document: doc,
                    ids: vec![id],
                    mode: burnburn_presentation::SelectMode::Replace,
                });
                s.pump(0.3);
            }
            let adjust = s.menu(MenuItem::Command(CommandKey::AdjustImage));
            if let Err(e) = &adjust {
                println!("    image-adjust: {e}");
            }
            if adjust.is_ok() {
                let _ = s.wait(5.0, "the first preview", || {
                    s.app.with_window(doc, |w| w.adjust_preview.image().is_some()).unwrap_or(false)
                });
                self.new_windows_keep(s, doc, "image-adjust");
                close(CommandKey::CancelImageAdjust);
            }
            s.app.native(NativeEvent::TraceFileChosen { document: doc, path: Some(path) });
            let _ = s.wait(5.0, "the tracer sheet", || {
                s.app.with_window(doc, |w| w.tracer_sheet.is_some()).unwrap_or(false)
            });
            self.new_windows_keep(s, doc, "tracer");
            close(CommandKey::CloseTracer);
        }

        /// Like `new_windows`, but leaves the windows up: the caller closes them by their command.
        fn new_windows_keep(&mut self, s: &Session, doc: DocumentId, step: &str) {
            s.pump(0.6);
            let main = s.app.window(doc);
            let app = NSApplication::sharedApplication(s.mtm);
            let mut seen = HashSet::new();
            for w in app.windows().iter() {
                if !w.isVisible() || main.as_deref().is_some_and(|m| ptr::eq(m, &*w)) {
                    continue;
                }
                if seen.insert(w.windowNumber()) {
                    self.window(s, &w, step, None);
                }
            }
        }

        fn path(&mut self, step: &str, title: &str) -> PathBuf {
            self.count += 1;
            let clean: String = title
                .chars()
                .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
                .collect::<String>()
                .split('-')
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join("-");
            let name = if clean.is_empty() { step.to_string() } else { format!("{step}-{clean}") };
            self.dir.join(format!("{}-{:02}-{name}.png", self.prefix, self.count))
        }

        /// The document window with the offscreen-rendered canvas composited in.
        fn document(&mut self, s: &Session, doc: DocumentId, step: &str) -> Result<(), String> {
            let window = s.app.window(doc).ok_or("no window")?;
            let canvas = s.app.with_window(doc, |w| w.canvas_view.retain()).ok_or("no canvas view")?;
            self.window(s, &window, step, Some((doc, canvas)));
            Ok(())
        }

        /// Every visible window that appeared for `step` (panels and sheets), then hides them again.
        fn new_windows(&mut self, s: &Session, doc: DocumentId, step: &str) {
            s.pump(0.6);
            let main = s.app.window(doc);
            let app = NSApplication::sharedApplication(s.mtm);
            let mut seen = HashSet::new();
            for w in app.windows().iter() {
                if !w.isVisible() || main.as_deref().is_some_and(|m| ptr::eq(m, &*w)) {
                    continue;
                }
                if !seen.insert(w.windowNumber()) {
                    continue;
                }
                self.window(s, &w, step, None);
                if let Some(parent) = w.sheetParent() {
                    parent.endSheet(&w);
                } else {
                    w.orderOut(None);
                }
            }
            s.pump(0.2);
        }

        fn window(
            &mut self,
            s: &Session,
            window: &NSWindow,
            step: &str,
            canvas: Option<(DocumentId, Retained<NSView>)>,
        ) {
            let title = window.title().to_string();
            let path = self.path(step, &title);
            match self.save(s, window, canvas, &path) {
                Ok(()) => println!("    {}", path.display()),
                Err(e) => println!("    {step} ({title}): {e}"),
            }
        }

        fn save(
            &self,
            s: &Session,
            window: &NSWindow,
            canvas: Option<(DocumentId, Retained<NSView>)>,
            path: &Path,
        ) -> Result<(), String> {
            window.displayIfNeeded();
            // The frame view (the content view's superview) includes the title bar and toolbar.
            let content = window.contentView().ok_or("no content view")?;
            // SAFETY: reading the superview of the window's own content view on the main thread.
            let frame_view = unsafe { content.superview() }.unwrap_or(content);
            let bounds = frame_view.bounds();
            let rep = frame_view.bitmapImageRepForCachingDisplayInRect(bounds).ok_or("no bitmap rep")?;
            frame_view.cacheDisplayInRect_toBitmapImageRep(bounds, &rep);
            if let Some((doc, canvas)) = canvas {
                self.composite(s, doc, &canvas, &frame_view, &rep);
            }
            write_png(&rep, path)
        }

        /// The canvas pixels of `doc`'s scene, rendered offscreen and drawn at the canvas frame.
        fn composite(
            &self,
            s: &Session,
            doc: DocumentId,
            canvas: &NSView,
            frame_view: &NSView,
            rep: &NSBitmapImageRep,
        ) {
            let (Some(gpu), Some(scene)) = (&self.gpu, s.app.scene(doc)) else { return };
            let image = match render_offscreen(gpu, &canvas_packet_themed(&scene, false, 0.25, self.dark)) {
                Ok(i) => i,
                Err(e) => {
                    println!("    canvas render: {e:?}");
                    return;
                }
            };
            let Some(canvas_rep) = rgba_rep(image.width, image.height, &image.rgba) else { return };
            let rect: NSRect = canvas.convertRect_toView(canvas.bounds(), Some(frame_view));
            let Some(ctx) = NSGraphicsContext::graphicsContextWithBitmapImageRep(rep) else { return };
            NSGraphicsContext::saveGraphicsState_class();
            NSGraphicsContext::setCurrentContext(Some(&ctx));
            canvas_rep.drawInRect(rect);
            ctx.flushGraphics();
            NSGraphicsContext::restoreGraphicsState_class();
        }
    }

    fn write_png(rep: &NSBitmapImageRep, path: &Path) -> Result<(), String> {
        let props = NSDictionary::new();
        // SAFETY: PNG encoding of our own bitmap representation with an empty property dictionary.
        let data = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &props) }
            .ok_or("no PNG data")?;
        std::fs::write(path, data.to_vec()).map_err(|e| e.to_string())
    }

    /// Straight RGBA8 (top row first) as a premultiplied device-RGB bitmap representation.
    fn rgba_rep(width: u32, height: u32, rgba: &[u8]) -> Option<Retained<NSBitmapImageRep>> {
        let (w, h) = (isize::try_from(width).ok()?, isize::try_from(height).ok()?);
        let len = (width as usize).checked_mul(height as usize)?.checked_mul(4)?;
        if w == 0 || h == 0 || rgba.len() != len {
            return None;
        }
        // SAFETY: null planes make the representation allocate its own `h * w * 4` bytes; the
        // colour space name is an immutable AppKit constant.
        let rep = unsafe {
            NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(),
                ptr::null_mut(),
                w,
                h,
                8,
                4,
                true,
                false,
                NSDeviceRGBColorSpace,
                w * 4,
                32,
            )
        }?;
        let data = rep.bitmapData();
        if data.is_null() {
            return None;
        }
        // SAFETY: the representation owns exactly `len` bytes at `data` (no row padding).
        let dst = unsafe { std::slice::from_raw_parts_mut(data, len) };
        for (d, s) in dst.as_chunks_mut::<4>().0.iter_mut().zip(rgba.as_chunks::<4>().0) {
            let a = u32::from(s[3]);
            for i in 0..3 {
                d[i] = ((u32::from(s[i]) * a + 127) / 255) as u8;
            }
            d[3] = s[3];
        }
        Some(rep)
    }
}
