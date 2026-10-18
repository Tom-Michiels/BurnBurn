//! `cargo xtask bundle`: wraps the `burnburn` binary of `burnburn-macos` in an unsigned `BurnBurn.app`.
//!
//! Owner: GUI (module in Testing's xtask by agreement). The bundle declares the `.burnburn` project
//! document type (exported UTI) and macOS 13 as minimum. No signing or notarisation.

use std::path::{Path, PathBuf};

/// What goes into `Info.plist`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleSpec {
    pub name: String,
    pub executable: String,
    pub identifier: String,
    pub version: String,
    pub minimum_macos: String,
}

impl BundleSpec {
    pub fn burnburn(version: &str) -> Self {
        BundleSpec {
            name: "BurnBurn".into(),
            executable: "burnburn".into(),
            identifier: "app.burnburn.BurnBurn".into(),
            version: version.into(),
            minimum_macos: "13.0".into(),
        }
    }
}

/// The UTI exported for `.burnburn` project containers.
pub const PROJECT_UTI: &str = "app.burnburn.project";

/// The `Info.plist` XML for `spec`.
pub fn info_plist(spec: &BundleSpec) -> String {
    let e = |v: &str| escape(v);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>nl</string>
	<key>CFBundleDisplayName</key>
	<string>{name}</string>
	<key>CFBundleExecutable</key>
	<string>{executable}</string>
	<key>CFBundleIconFile</key>
	<string>{icon}</string>
	<key>CFBundleIdentifier</key>
	<string>{identifier}</string>
	<key>CFBundleLocalizations</key>
	<array>
		<string>nl</string>
		<string>en</string>
	</array>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>{name}</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>{version}</string>
	<key>CFBundleVersion</key>
	<string>{version}</string>
	<key>LSMinimumSystemVersion</key>
	<string>{minimum}</string>
	<key>NSCameraUsageDescription</key>
	<string>{camera}</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>NSPrincipalClass</key>
	<string>NSApplication</string>
	<key>NSSupportsAutomaticGraphicsSwitching</key>
	<true/>
	<key>CFBundleDocumentTypes</key>
	<array>
		<dict>
			<key>CFBundleTypeIconFile</key>
			<string>{icon}</string>
			<key>CFBundleTypeName</key>
			<string>BurnBurn-project</string>
			<key>CFBundleTypeRole</key>
			<string>Editor</string>
			<key>LSHandlerRank</key>
			<string>Owner</string>
			<key>LSItemContentTypes</key>
			<array>
				<string>{uti}</string>
			</array>
		</dict>
	</array>
	<key>UTExportedTypeDeclarations</key>
	<array>
		<dict>
			<key>UTTypeIdentifier</key>
			<string>{uti}</string>
			<key>UTTypeDescription</key>
			<string>BurnBurn-project</string>
			<key>UTTypeConformsTo</key>
			<array>
				<string>public.data</string>
				<string>public.zip-archive</string>
			</array>
			<key>UTTypeTagSpecification</key>
			<dict>
				<key>public.filename-extension</key>
				<array>
					<string>burnburn</string>
				</array>
			</dict>
		</dict>
	</array>
</dict>
</plist>
"#,
        name = e(&spec.name),
        executable = e(&spec.executable),
        identifier = e(&spec.identifier),
        version = e(&spec.version),
        minimum = e(&spec.minimum_macos),
        uti = PROJECT_UTI,
        icon = ICON_NAME,
        camera = e(camera_usage("en")),
    )
}

fn escape(v: &str) -> String {
    v.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Icon file name inside `Contents/Resources` (without extension, as `CFBundleIconFile` wants).
pub const ICON_NAME: &str = "BurnBurn";

/// Localisations the bundle declares (AppKit panels and alerts follow these, IM20).
pub const LOCALISATIONS: [&str; 2] = ["nl", "en"];

/// The app and document icon: an original placeholder drawn here (a laser beam cutting a circle
/// out of a sheet), as an `.icns` with 128, 256 and 512 px PNG entries. No third-party artwork.
pub fn app_icon_icns() -> Vec<u8> {
    let mut entries = Vec::new();
    for (kind, size) in [(b"ic07", 128u32), (b"ic08", 256), (b"ic09", 512)] {
        let png = png_rgba(size, size, &draw_icon(size));
        entries.extend_from_slice(kind);
        entries.extend_from_slice(&((png.len() + 8) as u32).to_be_bytes());
        entries.extend_from_slice(&png);
    }
    let mut icns = Vec::with_capacity(entries.len() + 8);
    icns.extend_from_slice(b"icns");
    icns.extend_from_slice(&((entries.len() + 8) as u32).to_be_bytes());
    icns.extend_from_slice(&entries);
    icns
}

/// Colour of the icon at normalised position `(u, v)` (v down), or transparent.
fn icon_sample(u: f64, v: f64) -> [f64; 4] {
    let rounded = |x0: f64, y0: f64, x1: f64, y1: f64, r: f64| {
        let dx = (x0 + r - u).max(u - (x1 - r)).max(0.0);
        let dy = (y0 + r - v).max(v - (y1 - r)).max(0.0);
        u >= x0 && u <= x1 && v >= y0 && v <= y1 && dx * dx + dy * dy <= r * r
    };
    let (cx, cy, radius) = (0.5, 0.62, 0.14);
    let d = ((u - cx).powi(2) + (v - cy).powi(2)).sqrt();
    // Angle from the top of the circle, clockwise: the cut runs 270° from where the beam is.
    let angle = (u - cx).atan2(cy - v).rem_euclid(std::f64::consts::TAU);
    let hot = ((u - 0.5).powi(2) + (v - 0.48).powi(2)).sqrt();
    if hot <= 0.032 {
        [1.0, 0.86, 0.48, 1.0]
    } else if (0.49..=0.51).contains(&u) && (0.1..=0.48).contains(&v) {
        [1.0, 0.36, 0.12, 1.0]
    } else if (d - radius).abs() <= 0.014 && angle <= 1.5 * std::f64::consts::PI {
        [0.42, 0.24, 0.12, 1.0]
    } else if rounded(0.2, 0.42, 0.8, 0.84, 0.04) {
        [0.89, 0.75, 0.53, 1.0]
    } else if rounded(0.06, 0.06, 0.94, 0.94, 0.18) {
        [0.13, 0.19 + 0.06 * (1.0 - v), 0.24 + 0.06 * (1.0 - v), 1.0]
    } else {
        [0.0; 4]
    }
}

/// RGBA8 pixels with 4 × 4 supersampling (premultiplied while averaging).
fn draw_icon(size: u32) -> Vec<u8> {
    const SS: u32 = 4;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let mut acc = [0.0f64; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let u = (f64::from(x) + (f64::from(sx) + 0.5) / f64::from(SS)) / f64::from(size);
                    let v = (f64::from(y) + (f64::from(sy) + 0.5) / f64::from(SS)) / f64::from(size);
                    let c = icon_sample(u, v);
                    for (a, v) in acc.iter_mut().zip(c).take(3) {
                        *a += v * c[3];
                    }
                    acc[3] += c[3];
                }
            }
            let n = f64::from(SS * SS);
            let alpha = acc[3] / n;
            for premultiplied in &acc[..3] {
                let straight = if alpha > 0.0 { premultiplied / n / alpha } else { 0.0 };
                out.push((straight.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
            out.push((alpha.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    out
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in bytes {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

/// A minimal valid PNG (RGBA8, no filtering, zlib "stored" blocks): no compression library needed.
fn png_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgba.len() + height as usize);
    for row in rgba.chunks(width as usize * 4) {
        raw.push(0); // filter type None
        raw.extend_from_slice(row);
    }
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, block) in blocks.iter().enumerate() {
        zlib.push(u8::from(i + 1 == blocks.len()));
        let len = block.len() as u16;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        png.extend_from_slice(&body);
        png.extend_from_slice(&crc32(&body).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 bit, RGBA, deflate, adaptive filtering, no interlace
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &zlib);
    chunk(b"IEND", &[]);
    png
}

/// Purpose string shown in the camera privacy prompt (IM27), per language.
fn camera_usage(language: &str) -> &'static str {
    match language {
        "nl" => "BurnBurn gebruikt de camera alleen als plaatsingshulp boven het laserbed.",
        _ => "BurnBurn uses the camera only as a placement aid above the laser bed.",
    }
}

/// `<lang>.lproj/InfoPlist.strings` (UTF-8 is accepted by macOS for .strings files).
pub fn info_plist_strings(language: &str) -> String {
    format!("\"NSCameraUsageDescription\" = \"{}\";\n", camera_usage(language))
}

/// Lays out `<out_dir>/<name>.app` around an existing binary; returns the bundle path.
pub fn write_bundle(spec: &BundleSpec, binary: &Path, out_dir: &Path) -> Result<PathBuf, String> {
    let io = |what: &str, p: &Path, e: std::io::Error| format!("{what} {}: {e}", p.display());
    if !binary.is_file() {
        return Err(format!("no binary at {}", binary.display()));
    }
    let app = out_dir.join(format!("{}.app", spec.name));
    if app.exists() {
        std::fs::remove_dir_all(&app).map_err(|e| io("removing old bundle", &app, e))?;
    }
    let macos = app.join("Contents/MacOS");
    let resources = app.join("Contents/Resources");
    for dir in [&macos, &resources] {
        std::fs::create_dir_all(dir).map_err(|e| io("creating", dir, e))?;
    }
    let exe = macos.join(&spec.executable);
    std::fs::copy(binary, &exe).map_err(|e| io("copying binary to", &exe, e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| io("making executable", &exe, e))?;
    }
    let plist = app.join("Contents/Info.plist");
    std::fs::write(&plist, info_plist(spec)).map_err(|e| io("writing", &plist, e))?;
    let icon = resources.join(format!("{ICON_NAME}.icns"));
    std::fs::write(&icon, app_icon_icns()).map_err(|e| io("writing", &icon, e))?;
    for lang in LOCALISATIONS {
        let dir = resources.join(format!("{lang}.lproj"));
        std::fs::create_dir_all(&dir).map_err(|e| io("creating", &dir, e))?;
        let strings = dir.join("InfoPlist.strings");
        std::fs::write(&strings, info_plist_strings(lang)).map_err(|e| io("writing", &strings, e))?;
    }
    let pkginfo = app.join("Contents/PkgInfo");
    std::fs::write(&pkginfo, "APPL????").map_err(|e| io("writing", &pkginfo, e))?;
    Ok(app)
}

/// `cargo xtask bundle [--release]`: build `burnburn` for macOS 13+ and bundle it next to the binary
/// (`<target>/<profile>/BurnBurn.app`). Unsigned.
pub fn run(root: &Path, args: &[String]) -> Result<(), String> {
    let release = args.iter().any(|a| a == "--release");
    if let Some(other) = args.iter().find(|a| *a != "--release") {
        return Err(format!("bundle: unknown argument `{other}` (only --release)"));
    }
    if !cfg!(target_os = "macos") {
        return Err("bundle: an .app is built on macOS only".into());
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut build = std::process::Command::new(cargo);
    build.args(["build", "--locked", "-p", "burnburn-macos", "--bin", "burnburn"]).current_dir(root);
    if release {
        build.arg("--release");
    }
    // The binary's own load commands then say macOS 13, matching LSMinimumSystemVersion.
    build.env("MACOSX_DEPLOYMENT_TARGET", "13.0");
    println!(
        "$ MACOSX_DEPLOYMENT_TARGET=13.0 cargo build --locked -p burnburn-macos --bin burnburn{}",
        if release { " --release" } else { "" }
    );
    let status = build.status().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("building burnburn failed ({status})"));
    }
    let target = std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|| root.join("target"));
    let target = if target.is_absolute() { target } else { root.join(target) };
    let profile_dir = target.join(if release { "release" } else { "debug" });
    let app =
        write_bundle(&BundleSpec::burnburn(env!("CARGO_PKG_VERSION")), &profile_dir.join("burnburn"), &profile_dir)?;
    println!("bundle: {} (unsigned)", app.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value_after<'a>(plist: &'a str, key: &str) -> Option<&'a str> {
        let rest = plist.split(&format!("<key>{key}</key>")).nth(1)?;
        let start = rest.find("<string>")? + "<string>".len();
        let end = rest.find("</string>")?;
        rest.get(start..end)
    }

    #[test]
    fn info_plist_declares_burnburn_documents_and_macos_13() {
        let plist = info_plist(&BundleSpec::burnburn("0.1.0"));
        assert!(plist.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"), "{plist}");
        assert_eq!(value_after(&plist, "CFBundleExecutable"), Some("burnburn"));
        assert_eq!(value_after(&plist, "CFBundleIdentifier"), Some("app.burnburn.BurnBurn"));
        assert_eq!(value_after(&plist, "CFBundlePackageType"), Some("APPL"));
        assert_eq!(value_after(&plist, "CFBundleShortVersionString"), Some("0.1.0"));
        assert_eq!(value_after(&plist, "LSMinimumSystemVersion"), Some("13.0"));
        // The document type names the exported UTI, and the UTI owns the extension.
        let docs = plist.split("<key>CFBundleDocumentTypes</key>").nth(1).unwrap_or_default();
        assert_eq!(value_after(docs, "CFBundleTypeRole"), Some("Editor"));
        assert_eq!(value_after(docs, "LSHandlerRank"), Some("Owner"));
        assert!(
            docs.contains(&format!(
                "<key>LSItemContentTypes</key>\n\t\t\t<array>\n\t\t\t\t<string>{PROJECT_UTI}</string>"
            )),
            "{docs}"
        );
        let uti = plist.split("<key>UTExportedTypeDeclarations</key>").nth(1).unwrap_or_default();
        assert_eq!(value_after(uti, "UTTypeIdentifier"), Some(PROJECT_UTI));
        assert!(
            uti.contains("<key>public.filename-extension</key>\n\t\t\t\t<array>\n\t\t\t\t\t<string>burnburn</string>"),
            "{uti}"
        );
        assert!(uti.contains("<string>public.data</string>") && uti.contains("<string>public.zip-archive</string>"));
    }

    #[test]
    fn plist_escapes_xml_in_values() {
        let mut spec = BundleSpec::burnburn("1 < 2 & \"3\"");
        spec.name = "A&B".into();
        let plist = info_plist(&spec);
        assert_eq!(value_after(&plist, "CFBundleShortVersionString"), Some("1 &lt; 2 &amp; &quot;3&quot;"));
        assert_eq!(value_after(&plist, "CFBundleName"), Some("A&amp;B"));
    }

    #[test]
    fn bundle_layout_places_executable_plist_and_pkginfo() {
        let dir = std::env::temp_dir().join(format!("burnburn-xtask-bundle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let binary = dir.join("burnburn");
        std::fs::write(&binary, b"#!/bin/sh\n").unwrap();
        let spec = BundleSpec::burnburn("0.1.0");
        let app = write_bundle(&spec, &binary, &dir.join("out")).unwrap();
        assert_eq!(app, dir.join("out/BurnBurn.app"));
        assert_eq!(std::fs::read(app.join("Contents/MacOS/burnburn")).unwrap(), b"#!/bin/sh\n");
        assert_eq!(std::fs::read_to_string(app.join("Contents/Info.plist")).unwrap(), info_plist(&spec));
        assert_eq!(std::fs::read_to_string(app.join("Contents/PkgInfo")).unwrap(), "APPL????");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(app.join("Contents/MacOS/burnburn")).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "executable bits");
        }
        // Re-bundling replaces the old bundle instead of failing or merging.
        std::fs::write(app.join("Contents/stale"), b"x").unwrap();
        write_bundle(&spec, &binary, &dir.join("out")).unwrap();
        assert!(!app.join("Contents/stale").exists());
        assert!(write_bundle(&spec, &dir.join("missing"), &dir.join("out2")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Big-endian u32 at `at`.
    fn be32(b: &[u8], at: usize) -> u32 {
        u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
    }

    /// The icns container is self-consistent and holds PNGs of the sizes its type codes promise.
    #[test]
    fn app_icon_is_an_icns_with_png_entries_of_the_declared_sizes() {
        let icns = app_icon_icns();
        assert!(icns.len() > 8, "empty icon");
        assert_eq!(&icns[..4], b"icns");
        assert_eq!(be32(&icns, 4) as usize, icns.len(), "total length");
        let mut at = 8;
        let mut sizes = Vec::new();
        while at < icns.len() {
            let kind = &icns[at..at + 4];
            let len = be32(&icns, at + 4) as usize;
            let png = &icns[at + 8..at + len];
            assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "entry {kind:?} is a PNG");
            assert_eq!(&png[12..16], b"IHDR");
            let (w, h) = (be32(png, 16), be32(png, 20));
            let expected = match kind {
                b"ic07" => 128,
                b"ic08" => 256,
                b"ic09" => 512,
                other => panic!("unexpected icns entry {other:?}"),
            };
            assert_eq!((w, h), (expected, expected));
            sizes.push(expected);
            at += len;
        }
        assert_eq!(sizes, vec![128, 256, 512]);
    }

    /// Independent oracle: macOS's own image tool reads the icon (skipped elsewhere, not a pass).
    #[test]
    fn app_icon_is_readable_by_sips() {
        if !cfg!(target_os = "macos") {
            println!("SKIPPED (not macOS; not a pass)");
            return;
        }
        let dir = std::env::temp_dir().join(format!("burnburn-xtask-icon-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("BurnBurn.icns");
        std::fs::write(&path, app_icon_icns()).unwrap();
        let out =
            std::process::Command::new("sips").args(["-g", "pixelWidth", "-g", "pixelHeight"]).arg(&path).output();
        std::fs::remove_dir_all(&dir).unwrap();
        let out = out.unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{text}");
        assert!(text.contains("pixelWidth: 512") && text.contains("pixelHeight: 512"), "{text}");
    }

    #[test]
    fn plist_names_icon_localisations_and_camera_purpose() {
        let plist = info_plist(&BundleSpec::burnburn("0.1.0"));
        assert_eq!(value_after(&plist, "CFBundleIconFile"), Some(ICON_NAME));
        let docs = plist.split("<key>CFBundleDocumentTypes</key>").nth(1).unwrap_or_default();
        assert_eq!(value_after(docs, "CFBundleTypeIconFile"), Some(ICON_NAME));
        assert!(
            plist.contains(
                "<key>CFBundleLocalizations</key>\n\t<array>\n\t\t<string>nl</string>\n\t\t<string>en</string>"
            ),
            "{plist}"
        );
        // Camera access (IM27) needs a purpose string or macOS terminates the app on first use.
        assert!(value_after(&plist, "NSCameraUsageDescription").is_some_and(|v| !v.is_empty()));
        for lang in LOCALISATIONS {
            let strings = info_plist_strings(lang);
            assert!(strings.contains("\"NSCameraUsageDescription\" = \""), "{lang}: {strings}");
        }
        assert_ne!(info_plist_strings("nl"), info_plist_strings("en"));
    }

    #[test]
    fn bundle_contains_icon_and_language_folders() {
        let dir = std::env::temp_dir().join(format!("burnburn-xtask-bundle2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let binary = dir.join("burnburn");
        std::fs::write(&binary, b"#!/bin/sh\n").unwrap();
        let app = write_bundle(&BundleSpec::burnburn("0.1.0"), &binary, &dir).unwrap();
        let resources = app.join("Contents/Resources");
        assert_eq!(std::fs::read(resources.join("BurnBurn.icns")).unwrap(), app_icon_icns());
        for lang in LOCALISATIONS {
            let strings = std::fs::read_to_string(resources.join(format!("{lang}.lproj/InfoPlist.strings"))).unwrap();
            assert_eq!(strings, info_plist_strings(lang));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
