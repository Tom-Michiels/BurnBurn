//! `cargo xtask msix-layout`: the Windows installer route (X07), without C#/C++.
//!
//! Writes the directory that `makeappx pack /d <dir> /p BurnBurn.msix` turns into an MSIX package:
//! `AppxManifest.xml` (identity, version, file type `.burnburn`, icons, capabilities), the logo
//! PNGs (the same original icon as the macOS bundle) and, when given, `burnburn-win.exe`.
//!
//! The manifest is generated and tested headlessly here. Packing, signing and installing need the
//! Windows SDK and a Windows machine and are manual steps (docs/release.md). Minimum OS is
//! Windows 10 22H2 (build 19045, open decision B-11).

use std::path::{Path, PathBuf};

/// Windows 10 22H2.
pub const MIN_WINDOWS_VERSION: &str = "10.0.19045.0";
pub const EXECUTABLE: &str = "burnburn-win.exe";
/// The extension of project files, with its dot (MSIX wants it).
pub const PROJECT_EXTENSION: &str = ".burnburn";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsixSpec {
    /// Package identity name: letters, digits, `.` and `-` only.
    pub identity: String,
    /// Must equal the subject of the signing certificate; the default is a placeholder.
    pub publisher: String,
    pub publisher_display_name: String,
    pub display_name: String,
    /// Four-part MSIX version.
    pub version: String,
}

/// Placeholder certificate subject; replace it with `--publisher` for a signed package.
pub const DEFAULT_PUBLISHER: &str = "CN=BurnBurn Development";

impl MsixSpec {
    /// `cargo_version` is the workspace version (`0.1.0`, optionally with a pre-release suffix).
    pub fn burnburn(cargo_version: &str, publisher: Option<&str>) -> Result<Self, String> {
        let publisher = publisher.unwrap_or(DEFAULT_PUBLISHER);
        let subject = publisher.strip_prefix("CN=").unwrap_or("");
        if subject.trim().is_empty() {
            return Err(format!("publisher must be a certificate subject like \"CN=Name\", got {publisher:?}"));
        }
        Ok(MsixSpec {
            identity: "BurnBurn.Desktop".into(),
            publisher: publisher.into(),
            publisher_display_name: subject.trim().into(),
            display_name: "BurnBurn".into(),
            version: msix_version(cargo_version)?,
        })
    }
}

/// `0.1.0` becomes `0.1.0.0`; a pre-release or build suffix is dropped. The fourth part is
/// reserved by the Microsoft Store and stays 0.
pub fn msix_version(cargo_version: &str) -> Result<String, String> {
    let core = cargo_version.split(['-', '+']).next().unwrap_or("");
    let parts: Vec<&str> = core.split('.').collect();
    let numbers: Option<Vec<u16>> = parts
        .iter()
        .map(|p| if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) { None } else { p.parse().ok() })
        .collect();
    match numbers {
        Some(n) if n.len() == 3 => Ok(format!("{}.{}.{}.0", n[0], n[1], n[2])),
        _ => Err(format!("version {cargo_version:?} is not MAJOR.MINOR.PATCH with parts up to 65535")),
    }
}

/// (file name inside `Assets/`, pixel size) of every logo the manifest refers to.
pub fn asset_files() -> Vec<(&'static str, u32)> {
    vec![("Square44x44Logo.png", 44), ("Square150x150Logo.png", 150), ("StoreLogo.png", 50), ("FileIcon.png", 64)]
}

pub fn appx_manifest(spec: &MsixSpec) -> String {
    let e = crate::bundle::escape;
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<Package
  xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
  xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
  xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
  IgnorableNamespaces="uap rescap">
  <Identity Name="{identity}" Publisher="{publisher}" Version="{version}" ProcessorArchitecture="x64" />
  <Properties>
    <DisplayName>{display}</DisplayName>
    <PublisherDisplayName>{publisher_display}</PublisherDisplayName>
    <Logo>Assets\StoreLogo.png</Logo>
  </Properties>
  <Dependencies>
    <TargetDeviceFamily Name="Windows.Desktop" MinVersion="{min_os}" MaxVersionTested="10.0.26100.0" />
  </Dependencies>
  <Resources>
    <Resource Language="nl-NL" />
    <Resource Language="en-US" />
  </Resources>
  <Applications>
    <Application Id="BurnBurn" Executable="{exe}" EntryPoint="Windows.FullTrustApplication">
      <uap:VisualElements DisplayName="{display}" Description="Ontwerp en bestuur je lasersnijder" BackgroundColor="transparent" Square150x150Logo="Assets\Square150x150Logo.png" Square44x44Logo="Assets\Square44x44Logo.png" />
      <Extensions>
        <uap:Extension Category="windows.fileTypeAssociation">
          <uap:FileTypeAssociation Name="burnburn">
            <uap:SupportedFileTypes>
              <uap:FileType>{ext}</uap:FileType>
            </uap:SupportedFileTypes>
            <uap:DisplayName>BurnBurn-project</uap:DisplayName>
            <uap:Logo>Assets\FileIcon.png</uap:Logo>
            <uap:InfoTip>BurnBurn-project</uap:InfoTip>
          </uap:FileTypeAssociation>
        </uap:Extension>
      </Extensions>
    </Application>
  </Applications>
  <Capabilities>
    <Capability Name="privateNetworkClientServer" />
    <rescap:Capability Name="runFullTrust" />
    <DeviceCapability Name="webcam" />
  </Capabilities>
</Package>
"#,
        identity = e(&spec.identity),
        publisher = e(&spec.publisher),
        version = e(&spec.version),
        display = e(&spec.display_name),
        publisher_display = e(&spec.publisher_display_name),
        min_os = MIN_WINDOWS_VERSION,
        exe = EXECUTABLE,
        ext = PROJECT_EXTENSION,
    )
}

/// Writes `AppxManifest.xml`, `Assets/*.png` and (optionally) the executable into `out_dir`.
pub fn write_layout(spec: &MsixSpec, exe: Option<&Path>, out_dir: &Path) -> Result<PathBuf, String> {
    let io = |what: &str, p: &Path, e: std::io::Error| format!("{what} {}: {e}", p.display());
    if let Some(exe) = exe
        && !exe.is_file()
    {
        return Err(format!("no executable at {}", exe.display()));
    }
    let assets = out_dir.join("Assets");
    std::fs::create_dir_all(&assets).map_err(|e| io("creating", &assets, e))?;
    let manifest = out_dir.join("AppxManifest.xml");
    std::fs::write(&manifest, appx_manifest(spec)).map_err(|e| io("writing", &manifest, e))?;
    for (name, size) in asset_files() {
        let path = assets.join(name);
        let png = crate::icon::logo_png(size, name == "FileIcon.png")?;
        std::fs::write(&path, png).map_err(|e| io("writing", &path, e))?;
    }
    if let Some(exe) = exe {
        let target = out_dir.join(EXECUTABLE);
        std::fs::copy(exe, &target).map_err(|e| io("copying executable to", &target, e))?;
    }
    Ok(out_dir.to_path_buf())
}

/// `cargo xtask msix-layout [--exe <burnburn-win.exe>] [--out <dir>] [--publisher "CN=..."]`.
pub fn run(root: &Path, args: &[String]) -> Result<(), String> {
    let (mut exe, mut out, mut publisher) = (None, None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let slot = match a.as_str() {
            "--exe" => &mut exe,
            "--out" => &mut out,
            "--publisher" => &mut publisher,
            other => return Err(format!("msix-layout: unknown argument `{other}` (only --exe, --out, --publisher)")),
        };
        *slot = Some(it.next().ok_or_else(|| format!("msix-layout: {a} needs a value"))?.clone());
    }
    let spec = MsixSpec::burnburn(env!("CARGO_PKG_VERSION"), publisher.as_deref())?;
    let out = out.map_or_else(|| root.join("target/msix-layout"), PathBuf::from);
    let dir = write_layout(&spec, exe.as_deref().map(Path::new), &out)?;
    println!("msix-layout: {} (version {})", dir.display(), spec.version);
    println!(
        "next, on Windows with the SDK: makeappx pack /d \"{}\" /p BurnBurn.msix  (then sign; see docs/release.md)",
        dir.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> MsixSpec {
        MsixSpec::burnburn("0.1.0", None).unwrap()
    }

    /// Value of attribute `name` on the first element whose opening tag contains `marker`.
    fn attr<'a>(xml: &'a str, marker: &str, name: &str) -> Option<&'a str> {
        let at = xml.find(marker)?;
        let tag = &xml[at..at + xml[at..].find('>')?];
        let rest = &tag[tag.find(&format!("{name}=\""))? + name.len() + 2..];
        rest.get(..rest.find('"')?)
    }

    /// Tag-balance check: enough to catch unclosed or mismatched elements in generated XML.
    fn assert_well_formed(xml: &str) {
        let mut stack: Vec<String> = Vec::new();
        let mut rest = xml;
        while let Some(open) = rest.find('<') {
            let close = rest[open..].find('>').map(|c| open + c).unwrap_or_else(|| panic!("unterminated tag in {xml}"));
            let tag = &rest[open + 1..close];
            assert_eq!(tag.matches('"').count() % 2, 0, "unbalanced quotes in <{tag}>");
            if tag.starts_with('?') || tag.starts_with('!') {
            } else if let Some(name) = tag.strip_prefix('/') {
                assert_eq!(stack.pop().as_deref(), Some(name.trim()), "mismatched </{name}>");
            } else {
                let name = tag.split_whitespace().next().unwrap_or("").trim_end_matches('/');
                if !tag.ends_with('/') {
                    stack.push(name.to_owned());
                }
            }
            rest = &rest[close + 1..];
        }
        assert!(stack.is_empty(), "unclosed elements: {stack:?}");
    }

    #[test]
    fn cargo_versions_become_four_part_msix_versions() {
        assert_eq!(msix_version("0.1.0").unwrap(), "0.1.0.0");
        assert_eq!(msix_version("12.34.56").unwrap(), "12.34.56.0");
        assert_eq!(msix_version("1.2.3-rc.1+build5").unwrap(), "1.2.3.0");
        assert_eq!(msix_version("65535.0.1").unwrap(), "65535.0.1.0");
        for bad in ["1.2", "1.2.3.4", "a.b.c", "", "65536.0.0", "-1.0.0", "1..3"] {
            assert!(msix_version(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn the_default_spec_follows_the_workspace_version() {
        let cargo = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml")).unwrap();
        let workspace: toml::Table = toml::from_str(&cargo).unwrap();
        let version = workspace["workspace"]["package"]["version"].as_str().unwrap().to_owned();
        let spec = MsixSpec::burnburn(env!("CARGO_PKG_VERSION"), None).unwrap();
        assert_eq!(spec.version, format!("{version}.0"));
    }

    #[test]
    fn the_manifest_declares_identity_version_os_and_the_executable() {
        let xml = appx_manifest(&spec());
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"utf-8\"?>"), "{xml}");
        assert_eq!(attr(&xml, "<Identity", "Name"), Some("BurnBurn.Desktop"));
        assert_eq!(attr(&xml, "<Identity", "Version"), Some("0.1.0.0"));
        assert_eq!(attr(&xml, "<Identity", "Publisher"), Some("CN=BurnBurn Development"));
        assert_eq!(attr(&xml, "<TargetDeviceFamily", "Name"), Some("Windows.Desktop"));
        assert_eq!(attr(&xml, "<TargetDeviceFamily", "MinVersion"), Some(MIN_WINDOWS_VERSION));
        assert_eq!(attr(&xml, "<Application ", "Executable"), Some("burnburn-win.exe"));
        // A Win32 program, not a UWP one.
        assert_eq!(attr(&xml, "<Application ", "EntryPoint"), Some("Windows.FullTrustApplication"));
        assert!(xml.contains("<rescap:Capability Name=\"runFullTrust\" />"), "{xml}");
        assert_well_formed(&xml);
    }

    #[test]
    fn the_manifest_registers_burnburn_project_files() {
        let xml = appx_manifest(&spec());
        assert!(xml.contains("<uap:Extension Category=\"windows.fileTypeAssociation\">"), "{xml}");
        assert_eq!(attr(&xml, "<uap:FileTypeAssociation", "Name"), Some("burnburn"));
        assert!(xml.contains("<uap:FileType>.burnburn</uap:FileType>"), "{xml}");
        assert!(xml.contains("<uap:DisplayName>BurnBurn-project</uap:DisplayName>"), "{xml}");
    }

    #[test]
    fn every_logo_the_manifest_names_is_a_file_the_layout_writes() {
        let xml = appx_manifest(&spec());
        let names: Vec<&str> = asset_files().into_iter().map(|(n, _)| n).collect();
        let mut referenced = 0;
        for piece in xml.split("Assets\\").skip(1) {
            let file = &piece[..piece.find(['"', '<']).unwrap_or(piece.len())];
            assert!(names.contains(&file), "manifest names Assets\\{file}, which the layout does not write");
            referenced += 1;
        }
        assert!(referenced >= 4, "app tile, small tile, store logo and file icon: {referenced}");
        for (name, size) in asset_files() {
            assert!(size >= 44 && name.ends_with(".png"), "{name} {size}");
        }
    }

    #[test]
    fn values_are_escaped_and_bad_identities_refused() {
        let spec = MsixSpec::burnburn("0.1.0", Some("CN=Smith & Sons <Laser>")).unwrap();
        let xml = appx_manifest(&spec);
        assert_eq!(attr(&xml, "<Identity", "Publisher"), Some("CN=Smith &amp; Sons &lt;Laser&gt;"));
        assert_well_formed(&xml);
        assert!(MsixSpec::burnburn("0.1.0", Some("Smith")).is_err(), "a publisher is a certificate subject (CN=...)");
        assert!(MsixSpec::burnburn("0.1.0", Some("CN=")).is_err());
    }

    #[test]
    fn the_layout_directory_holds_manifest_logos_and_the_executable() {
        let dir = std::env::temp_dir().join(format!("burnburn-xtask-msix-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("in.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        let out = write_layout(&spec(), Some(&exe), &dir.join("pkg")).unwrap();
        assert_eq!(out, dir.join("pkg"));
        assert_eq!(std::fs::read_to_string(out.join("AppxManifest.xml")).unwrap(), appx_manifest(&spec()));
        assert_eq!(std::fs::read(out.join(EXECUTABLE)).unwrap(), b"MZ");
        for (name, size) in asset_files() {
            let png = std::fs::read(out.join("Assets").join(name)).unwrap();
            assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "{name}");
            let be = |at: usize| u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]);
            assert_eq!((be(16), be(20)), (size, size), "{name} size");
        }
        // Without an executable the layout is still written (manifest-only check on a Mac).
        let manifest_only = write_layout(&spec(), None, &dir.join("pkg2")).unwrap();
        assert!(manifest_only.join("AppxManifest.xml").is_file() && !manifest_only.join(EXECUTABLE).exists());
        // A missing executable path is an error, not a silently incomplete package.
        assert!(write_layout(&spec(), Some(&dir.join("missing.exe")), &dir.join("pkg3")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Windows logos fill their square (the macOS plate keeps a margin) and the file icon is the page.
    #[test]
    fn the_logos_are_the_plate_edge_to_edge_and_the_file_icon_is_the_document() {
        let dir = std::env::temp_dir().join(format!("burnburn-xtask-msix-logos-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let out = write_layout(&spec(), None, &dir).unwrap();
        let pixel = |name: &str, x: u32, y: u32| {
            let png = std::fs::read(out.join("Assets").join(name)).unwrap();
            let img = image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap().to_rgba8();
            img.get_pixel(x, y).0
        };
        for (name, size) in [("Square150x150Logo.png", 150), ("Square44x44Logo.png", 44), ("StoreLogo.png", 50)] {
            assert_eq!(pixel(name, 0, size / 2)[3], 255, "{name}: left edge is plate");
            assert_eq!(pixel(name, size / 2, 0)[3], 255, "{name}: top edge is plate");
        }
        // The file icon is the light page (not the dark plate) with transparent surroundings.
        assert_eq!(pixel("FileIcon.png", 2, 2)[3], 0, "outside the page");
        let paper = pixel("FileIcon.png", 14, 40);
        assert!(paper[0] > 200 && paper[1] > 200, "paper {paper:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn command_line_arguments_are_checked() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let err = run(root, &["--sign".to_owned()]).unwrap_err();
        assert!(err.contains("--sign") && err.contains("--exe"), "{err}");
        let err = run(root, &["--exe".to_owned()]).unwrap_err();
        assert!(err.contains("--exe"), "{err}");
    }
}
