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
    use burnburn_macos::app_menu::AppAction;
    use burnburn_macos::appkit::{HostOptions, canvas_packet_themed};
    use burnburn_macos::l10n::Locale;
    use burnburn_presentation::{CamDialog, CommandKey, UiIntent};
    use burnburn_render::gpu::{GpuContext, render_offscreen};
    use burnburn_types::DocumentId;
    use objc2::rc::Retained;
    use objc2::{AnyThread, Message};
    use objc2_app_kit::{
        NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication, NSBitmapImageFileType,
        NSBitmapImageRep, NSDeviceRGBColorSpace, NSGraphicsContext, NSView, NSWindow,
    };
    use objc2_foundation::{NSDictionary, NSRect};

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
            profile: Box::new(machine_profile()?),
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
        s.app.dispatch(UiIntent::AddOperation { document: doc, draft: cut_draft()? });
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

        let steps: Vec<(&str, MenuItem)> = vec![
            ("preferences", MenuItem::App(AppAction::Preferences)),
            ("shortcuts", MenuItem::App(AppAction::Shortcuts)),
            ("history", MenuItem::App(AppAction::History)),
            ("machine-hub", MenuItem::App(AppAction::MachineHub)),
            ("laser", MenuItem::LaserWindow),
            ("material-library", MenuItem::Command(CommandKey::ShowMaterialLibrary)),
            ("art-library", MenuItem::Command(CommandKey::ShowArtLibrary)),
            ("notes", MenuItem::Command(CommandKey::ShowNotes)),
            ("schikken", MenuItem::SchikkenPanel),
            ("offset", MenuItem::OffsetPanel),
            ("structure", MenuItem::StructurePanel),
            ("trace-prep", MenuItem::TracePrepPanel),
            ("operator", MenuItem::OperatorPanel),
            ("remnants", MenuItem::RemnantsPanel),
            ("recognition", MenuItem::RecognitionPanel),
            ("search", MenuItem::Command(CommandKey::SearchProjects)),
            ("generator", MenuItem::Command(CommandKey::GeneratorNew)),
            ("cam-optimization", MenuItem::CamDialog(CamDialog::Optimization)),
            ("cam-order", MenuItem::CamDialog(CamDialog::Order)),
            ("cam-sublayers", MenuItem::CamDialog(CamDialog::Sublayers)),
            ("cam-device", MenuItem::CamDialog(CamDialog::Device)),
            ("cam-array", MenuItem::CamDialog(CamDialog::Array)),
            ("cam-material-test", MenuItem::CamDialog(CamDialog::MaterialTest)),
            ("cam-interval-test", MenuItem::CamDialog(CamDialog::IntervalTest)),
            ("cam-focus-test", MenuItem::CamDialog(CamDialog::FocusTest)),
            ("cam-feeder", MenuItem::CamDialog(CamDialog::Feeder)),
            ("round-corners", MenuItem::Command(CommandKey::ShapesRoundCorners)),
            ("barcode", MenuItem::Command(CommandKey::BarcodeNew)),
            ("export-svg", MenuItem::Command(CommandKey::ExportSvg)),
            ("export-bundle", MenuItem::Command(CommandKey::ExportBundle)),
            ("cam-preflight", MenuItem::CamDialog(CamDialog::Preflight)),
            ("preview", MenuItem::App(AppAction::Preview)),
            ("inspect-rd", MenuItem::App(AppAction::InspectRd)),
            ("sketch", MenuItem::Command(CommandKey::SketchNew)),
        ];
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
        shot.document(&s, doc, "after-compile-sketch")?;
        println!("    {} images in {}", shot.count, shot.dir.display());
        Ok(())
    }

    fn machine_profile() -> Result<burnburn_job::MachineProfile, String> {
        use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
        let e = |e: &dyn std::fmt::Debug| format!("{e:?}");
        MachineProfile::new(
            "review-600x400",
            1,
            MachineFrame::new(600.0, 400.0, OriginCorner::BottomLeft).map_err(|x| e(&x))?,
            400.0,
            400.0,
            PowerRatio::new(0.0).map_err(|x| e(&x))?,
            PowerRatio::new(1.0).map_err(|x| e(&x))?,
        )
        .map_err(|x| e(&x))
    }

    fn cut_draft() -> Result<burnburn_document::process::OperationDraft, String> {
        use burnburn_document::process::{
            Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed,
        };
        let e = |e: &dyn std::fmt::Debug| format!("{e:?}");
        let p = PowerPercent::new(20.0).map_err(|x| e(&x))?;
        Ok(OperationDraft {
            name: "Snijden".into(),
            mode: OperationMode::Line { kerf: Kerf::NONE },
            settings: ProcessSettings {
                speed: Speed::mm_per_s(100.0).map_err(|x| e(&x))?,
                power: PowerRange::new(p, p).map_err(|x| e(&x))?,
                passes: Passes::new(1).map_err(|x| e(&x))?,
                air_assist: true,
            },
            output_enabled: true,
        })
    }

    impl Shooter {
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
