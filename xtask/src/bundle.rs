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

/// The pasteboard type of Cut/Copy (`burnburn_macos::pasteboard::CLIPBOARD_TYPE`), exported as plain data.
pub const CLIPBOARD_UTI: &str = "app.burnburn.clipboard.v1";

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
	<key>CFBundleIconName</key>
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
			<string>{document_icon}</string>
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
		<dict>
			<key>UTTypeIdentifier</key>
			<string>{clipboard_uti}</string>
			<key>UTTypeDescription</key>
			<string>BurnBurn-objecten op het klembord</string>
			<key>UTTypeConformsTo</key>
			<array>
				<string>public.data</string>
			</array>
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
        clipboard_uti = CLIPBOARD_UTI,
        icon = ICON_NAME,
        document_icon = DOCUMENT_ICON_NAME,
        camera = e(camera_usage("en")),
    )
}

pub(crate) fn escape(v: &str) -> String {
    v.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Icon file name inside `Contents/Resources` (without extension, as `CFBundleIconFile` wants).
pub const ICON_NAME: &str = "BurnBurn";

/// Icon of the `.burnburn` document type (`CFBundleTypeIconFile`, without extension).
pub const DOCUMENT_ICON_NAME: &str = "BurnBurnDocument";

/// Localisations the bundle declares (AppKit panels and alerts follow these, IM20).
pub const LOCALISATIONS: [&str; 2] = ["nl", "en"];

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
    for (name, svg) in [(ICON_NAME, crate::icon::APP_SVG), (DOCUMENT_ICON_NAME, crate::icon::DOCUMENT_SVG)] {
        let icon = resources.join(format!("{name}.icns"));
        std::fs::write(&icon, crate::icon::icns(svg)?).map_err(|e| io("writing", &icon, e))?;
    }
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

/// `cargo xtask bundle [--release] [--zip] [--dmg] [--sign-adhoc]`: build `burnburn` for macOS 13+ and
/// bundle it next to the binary (`<target>/<profile>/BurnBurn.app`). Unsigned unless `--sign-adhoc`;
/// `--zip`/`--dmg` write the distributable files and their checksums to `<target>/<profile>/dist/`
/// (see [`crate::dist`]). Notarisation is a manual step (docs/release.md).
pub fn run(root: &Path, args: &[String]) -> Result<(), String> {
    let options = crate::dist::parse_options(args)?;
    let release = options.release;
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
    let spec = BundleSpec::burnburn(env!("CARGO_PKG_VERSION"));
    let app = write_bundle(&spec, &profile_dir.join("burnburn"), &profile_dir)?;
    println!("bundle: {}", app.display());
    let files = crate::dist::finish(&options, &app, &profile_dir.join("dist"), &spec.name, &spec.version)?;
    for f in &files {
        println!("dist: {}", f.display());
    }
    println!(
        "bundle: {}; notarisation is manual (docs/release.md)",
        if options.adhoc_sign { "ad-hoc signed (not notarised)" } else { "unsigned" }
    );
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

    /// W2-L1: Cut/Copy put BurnBurn's objects on the pasteboard under a private type. It is declared
    /// (exported, a plain data type that is not a document) so other apps never mistake it for text.
    #[test]
    fn info_plist_exports_the_clipboard_type_as_plain_data() {
        let plist = info_plist(&BundleSpec::burnburn("0.1.0"));
        let exported = plist.split("<key>UTExportedTypeDeclarations</key>").nth(1).unwrap_or_default();
        // The same string as `burnburn_macos::pasteboard::CLIPBOARD_TYPE` (xtask does not depend on the host).
        let declaration = exported
            .split("<dict>")
            .find(|d| d.contains("<string>app.burnburn.clipboard.v1</string>"))
            .unwrap_or_else(|| panic!("clipboard type not declared: {exported}"));
        assert_eq!(value_after(declaration, "UTTypeIdentifier"), Some("app.burnburn.clipboard.v1"));
        assert!(declaration.contains("<string>public.data</string>"), "{declaration}");
        assert!(!declaration.contains("public.filename-extension"), "an in-memory type owns no file extension");
        assert!(!declaration.contains("public.zip-archive"), "it is not a project container: {declaration}");
        // The project type is still declared next to it.
        assert!(exported.contains(&format!("<string>{PROJECT_UTI}</string>")));
    }

    /// X07: the bundle version is the workspace version of `Cargo.toml`, not a second copy.
    #[test]
    fn bundle_version_is_the_workspace_version_from_cargo() {
        let cargo = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml")).unwrap();
        let workspace: toml::Table = toml::from_str(&cargo).unwrap();
        let version = workspace["workspace"]["package"]["version"].as_str().unwrap();
        let plist = info_plist(&BundleSpec::burnburn(env!("CARGO_PKG_VERSION")));
        assert_eq!(value_after(&plist, "CFBundleShortVersionString"), Some(version));
        assert_eq!(value_after(&plist, "CFBundleVersion"), Some(version));
    }

    /// Independent oracle: macOS's own `plutil -lint` parses the generated Info.plist.
    #[test]
    fn info_plist_passes_plutil_lint() {
        if !cfg!(target_os = "macos") {
            println!("SKIPPED (not macOS; not a pass)");
            return;
        }
        let dir = std::env::temp_dir().join(format!("burnburn-xtask-plist-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Info.plist");
        std::fs::write(&path, info_plist(&BundleSpec::burnburn("0.1.0"))).unwrap();
        let out = std::process::Command::new("plutil").arg("-lint").arg(&path).output();
        std::fs::remove_dir_all(&dir).unwrap();
        let out = out.unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{text}{}", String::from_utf8_lossy(&out.stderr));
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

    /// Independent oracle: macOS's own image tool reads the bundle icons (skipped elsewhere, not a pass).
    #[test]
    fn app_and_document_icons_are_readable_by_sips() {
        if !cfg!(target_os = "macos") {
            println!("SKIPPED (not macOS; not a pass)");
            return;
        }
        let dir = std::env::temp_dir().join(format!("burnburn-xtask-icon-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut report = String::new();
        for (name, svg) in [("app.icns", crate::icon::APP_SVG), ("doc.icns", crate::icon::DOCUMENT_SVG)] {
            let path = dir.join(name);
            std::fs::write(&path, crate::icon::icns(svg).unwrap()).unwrap();
            let out = std::process::Command::new("sips")
                .args(["-g", "pixelWidth", "-g", "pixelHeight"])
                .arg(&path)
                .output()
                .unwrap();
            assert!(out.status.success(), "{name}: {}", String::from_utf8_lossy(&out.stderr));
            report.push_str(&String::from_utf8_lossy(&out.stdout));
        }
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(report.contains("pixelWidth: 1024") && report.contains("pixelHeight: 1024"), "{report}");
    }

    #[test]
    fn plist_names_icon_localisations_and_camera_purpose() {
        let plist = info_plist(&BundleSpec::burnburn("0.1.0"));
        assert_eq!(value_after(&plist, "CFBundleIconFile"), Some(ICON_NAME));
        assert_eq!(value_after(&plist, "CFBundleIconName"), Some(ICON_NAME));
        let docs = plist.split("<key>CFBundleDocumentTypes</key>").nth(1).unwrap_or_default();
        assert_eq!(value_after(docs, "CFBundleTypeIconFile"), Some(DOCUMENT_ICON_NAME));
        assert_ne!(DOCUMENT_ICON_NAME, ICON_NAME, "documents get their own icon, not the app icon");
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
        assert_eq!(
            std::fs::read(resources.join("BurnBurn.icns")).unwrap(),
            crate::icon::icns(crate::icon::APP_SVG).unwrap()
        );
        assert_eq!(
            std::fs::read(resources.join("BurnBurnDocument.icns")).unwrap(),
            crate::icon::icns(crate::icon::DOCUMENT_SVG).unwrap()
        );
        for lang in LOCALISATIONS {
            let strings = std::fs::read_to_string(resources.join(format!("{lang}.lproj/InfoPlist.strings"))).unwrap();
            assert_eq!(strings, info_plist_strings(lang));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
