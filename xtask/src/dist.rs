//! `cargo xtask bundle --zip|--dmg|--sign-adhoc`: the macOS release route (X07).
//!
//! The `.app` itself comes from [`crate::bundle`]. This module turns it into distributable files
//! with system tools only (`ditto`, `hdiutil`, `codesign`), called as processes:
//!
//! - the **zip** is reproducible: file times are normalised to `SOURCE_DATE_EPOCH` first;
//! - the **dmg** is not byte-reproducible (the disk image format embeds identifiers);
//! - ad-hoc signing (`codesign -s -`) gives the bundle a valid seal without an identity; it is
//!   *not* notarisation (that stays a manual step with a developer identity);
//! - every artifact gets a `<file>.sha256` next to it, readable by `shasum -a 256 -c`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

/// 2024-01-01T00:00:00Z: the file time used when `SOURCE_DATE_EPOCH` is not set.
pub const DEFAULT_EPOCH: u64 = 1_704_067_200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BundleOptions {
    pub release: bool,
    pub zip: bool,
    pub dmg: bool,
    pub adhoc_sign: bool,
}

impl BundleOptions {
    /// Any option that produces files next to the `.app`.
    pub fn wants_dist(&self) -> bool {
        self.zip || self.dmg
    }
}

/// `--release --zip --dmg --sign-adhoc`; anything else is an error.
pub fn parse_options(args: &[String]) -> Result<BundleOptions, String> {
    let mut o = BundleOptions::default();
    for a in args {
        match a.as_str() {
            "--release" => o.release = true,
            "--zip" => o.zip = true,
            "--dmg" => o.dmg = true,
            "--sign-adhoc" => o.adhoc_sign = true,
            other => {
                return Err(format!("bundle: unknown argument `{other}` (only --release, --zip, --dmg, --sign-adhoc)"));
            }
        }
    }
    Ok(o)
}

/// The Rust architecture name as used in artifact names (`aarch64` is called `arm64` on macOS).
pub fn mac_arch(rust_arch: &str) -> &str {
    if rust_arch == "aarch64" { "arm64" } else { rust_arch }
}

/// `BurnBurn-0.1.0-macos-arm64.zip`.
pub fn artifact_name(name: &str, version: &str, arch: &str, extension: &str) -> String {
    format!("{name}-{version}-macos-{arch}.{extension}")
}

/// `SOURCE_DATE_EPOCH` (seconds) or [`DEFAULT_EPOCH`]; a value that is not a number is an error.
pub fn source_date_epoch(env: Option<&str>) -> Result<u64, String> {
    match env {
        None => Ok(DEFAULT_EPOCH),
        Some(v) => v.trim().parse().map_err(|_| format!("SOURCE_DATE_EPOCH must be whole seconds, got {v:?}")),
    }
}

/// Sets the modification time of every file and directory below `dir` (and `dir` itself).
/// Symbolic links are left alone.
pub fn normalize_mtimes(dir: &Path, epoch: u64) -> Result<(), String> {
    let time = SystemTime::UNIX_EPOCH + Duration::from_secs(epoch);
    let io = |p: &Path, e: std::io::Error| format!("{}: {e}", p.display());
    let entries = std::fs::read_dir(dir).map_err(|e| io(dir, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| io(dir, e))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| io(&path, e))?;
        if kind.is_dir() {
            normalize_mtimes(&path, epoch)?;
        } else if kind.is_file() {
            std::fs::File::open(&path).and_then(|f| f.set_modified(time)).map_err(|e| io(&path, e))?;
        }
    }
    std::fs::File::open(dir).and_then(|f| f.set_modified(time)).map_err(|e| io(dir, e))
}

fn text(p: &Path) -> String {
    p.display().to_string()
}

pub fn zip_args(app: &Path, out: &Path) -> Vec<String> {
    ["-c", "-k", "--sequesterRsrc", "--keepParent"]
        .into_iter()
        .map(str::to_owned)
        .chain([text(app), text(out)])
        .collect()
}

pub fn dmg_args(volume_name: &str, source_dir: &Path, out: &Path) -> Vec<String> {
    let fixed = ["create", "-volname", volume_name, "-srcfolder"].map(str::to_owned);
    let tail = ["-ov", "-format", "UDZO"].map(str::to_owned);
    fixed.into_iter().chain([text(source_dir)]).chain(tail).chain([text(out)]).collect()
}

pub fn codesign_args(app: &Path) -> Vec<String> {
    ["--force", "--deep", "--sign", "-"].into_iter().map(str::to_owned).chain([text(app)]).collect()
}

/// `<hex sha-256>  <file name>\n`: the format `shasum -a 256 -c` reads.
pub fn checksum_line(bytes: &[u8], file_name: &str) -> String {
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("{hex}  {file_name}\n")
}

fn run_tool(program: &str, args: &[String]) -> Result<(), String> {
    println!("$ {program} {}", args.join(" "));
    let status = Command::new(program).args(args).status().map_err(|e| format!("{program}: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("{program} failed ({status})")) }
}

fn write_checksum(file: &Path) -> Result<(), String> {
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let name = file.file_name().and_then(|n| n.to_str()).ok_or("artifact without a file name")?;
    let sum = PathBuf::from(format!("{}.sha256", file.display()));
    std::fs::write(&sum, checksum_line(&bytes, name)).map_err(|e| format!("{}: {e}", sum.display()))
}

/// Signs, zips and/or images `app` according to `options`; returns the files written
/// (artifacts and their `.sha256`). With no dist option nothing is created.
pub fn finish(
    options: &BundleOptions,
    app: &Path,
    out_dir: &Path,
    name: &str,
    version: &str,
) -> Result<Vec<PathBuf>, String> {
    if options.adhoc_sign {
        run_tool("codesign", &codesign_args(app))?;
    }
    if !options.wants_dist() {
        return Ok(Vec::new());
    }
    let epoch = source_date_epoch(std::env::var("SOURCE_DATE_EPOCH").ok().as_deref())?;
    normalize_mtimes(app, epoch)?;
    std::fs::create_dir_all(out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let arch = mac_arch(std::env::consts::ARCH);
    let mut written = Vec::new();
    if options.zip {
        let zip = out_dir.join(artifact_name(name, version, arch, "zip"));
        let _ = std::fs::remove_file(&zip);
        run_tool("ditto", &zip_args(app, &zip))?;
        write_checksum(&zip)?;
        written.push(zip.clone());
        written.push(PathBuf::from(format!("{}.sha256", zip.display())));
    }
    if options.dmg {
        let dmg = out_dir.join(artifact_name(name, version, arch, "dmg"));
        let stage = out_dir.join(format!("{name}-dmg-staging"));
        let _ = std::fs::remove_dir_all(&stage);
        std::fs::create_dir_all(&stage).map_err(|e| format!("{}: {e}", stage.display()))?;
        let staged_app = stage.join(format!("{name}.app"));
        run_tool("ditto", &[text(app), text(&staged_app)])?;
        #[cfg(unix)]
        std::os::unix::fs::symlink("/Applications", stage.join("Applications"))
            .map_err(|e| format!("Applications link: {e}"))?;
        let _ = std::fs::remove_file(&dmg);
        let result = run_tool("hdiutil", &dmg_args(name, &stage, &dmg));
        let _ = std::fs::remove_dir_all(&stage);
        result?;
        write_checksum(&dmg)?;
        written.push(dmg.clone());
        written.push(PathBuf::from(format!("{}.sha256", dmg.display())));
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_owned()).collect()
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("burnburn-xtask-dist-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn options_default_to_the_plain_bundle_and_reject_unknown_flags() {
        assert_eq!(parse_options(&[]).unwrap(), BundleOptions::default());
        let all = parse_options(&strings(&["--release", "--zip", "--dmg", "--sign-adhoc"])).unwrap();
        assert_eq!(all, BundleOptions { release: true, zip: true, dmg: true, adhoc_sign: true });
        assert!(all.wants_dist());
        assert!(!parse_options(&strings(&["--release"])).unwrap().wants_dist());
        let err = parse_options(&strings(&["--notarize"])).unwrap_err();
        assert!(err.contains("--notarize") && err.contains("--sign-adhoc"), "{err}");
    }

    #[test]
    fn artifact_names_carry_version_os_and_architecture() {
        assert_eq!(mac_arch("aarch64"), "arm64");
        assert_eq!(mac_arch("x86_64"), "x86_64");
        assert_eq!(artifact_name("BurnBurn", "0.1.0", "arm64", "zip"), "BurnBurn-0.1.0-macos-arm64.zip");
        assert_eq!(artifact_name("BurnBurn", "1.2.3", "x86_64", "dmg"), "BurnBurn-1.2.3-macos-x86_64.dmg");
    }

    #[test]
    fn source_date_epoch_is_read_strictly() {
        assert_eq!(source_date_epoch(None).unwrap(), DEFAULT_EPOCH);
        assert_eq!(source_date_epoch(Some("1700000000")).unwrap(), 1_700_000_000);
        assert!(source_date_epoch(Some("yesterday")).is_err());
        assert!(source_date_epoch(Some("")).is_err());
    }

    #[test]
    fn the_system_tool_invocations_are_exactly_what_we_document() {
        let app = Path::new("/o/BurnBurn.app");
        assert_eq!(
            zip_args(app, Path::new("/o/x.zip")),
            strings(&["-c", "-k", "--sequesterRsrc", "--keepParent", "/o/BurnBurn.app", "/o/x.zip"])
        );
        assert_eq!(
            dmg_args("BurnBurn", Path::new("/o/stage"), Path::new("/o/x.dmg")),
            strings(&[
                "create",
                "-volname",
                "BurnBurn",
                "-srcfolder",
                "/o/stage",
                "-ov",
                "-format",
                "UDZO",
                "/o/x.dmg"
            ])
        );
        // `-s -` is the ad-hoc identity: no certificate, no notarisation.
        assert_eq!(codesign_args(app), strings(&["--force", "--deep", "--sign", "-", "/o/BurnBurn.app"]));
    }

    #[test]
    fn checksum_line_is_the_sha256_of_the_bytes_in_shasum_format() {
        // FIPS 180-2 test vector for "abc".
        assert_eq!(
            checksum_line(b"abc", "a.zip"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  a.zip\n"
        );
    }

    #[test]
    fn normalised_times_are_the_same_for_every_file_and_directory() {
        let dir = scratch("mtime");
        std::fs::create_dir_all(dir.join("a/b")).unwrap();
        std::fs::write(dir.join("a/b/f"), b"x").unwrap();
        std::fs::write(dir.join("a/g"), b"y").unwrap();
        normalize_mtimes(&dir, 1_700_000_000).unwrap();
        let expected = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        for p in ["", "a", "a/b", "a/b/f", "a/g"] {
            let modified = std::fs::metadata(dir.join(p)).unwrap().modified().unwrap();
            assert_eq!(modified, expected, "{p:?}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A stand-in bundle (scripts, not a built app): enough for the packaging tools.
    fn fake_app(root: &Path) -> PathBuf {
        let app = root.join("BurnBurn.app");
        std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        std::fs::write(app.join("Contents/Info.plist"), b"<plist/>").unwrap();
        std::fs::write(app.join("Contents/MacOS/burnburn"), b"#!/bin/sh\n").unwrap();
        app
    }

    fn mac_only() -> bool {
        if cfg!(target_os = "macos") {
            return true;
        }
        println!("SKIPPED (not macOS; not a pass)");
        false
    }

    /// Independent oracle: `unzip -l` lists the bundle's files under `BurnBurn.app/`.
    #[test]
    fn the_zip_holds_the_bundle_and_is_reproducible_across_builds() {
        if !mac_only() {
            return;
        }
        let mut zips = Vec::new();
        for (i, tag) in ["z1", "z2"].into_iter().enumerate() {
            let dir = scratch(tag);
            let app = fake_app(&dir);
            // Different build times must not show in the artifact.
            let later = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000 + 1000 * i as u64);
            std::fs::File::open(app.join("Contents/Info.plist")).unwrap().set_modified(later).unwrap();
            let files = finish(
                &BundleOptions { release: false, zip: true, dmg: false, adhoc_sign: false },
                &app,
                &dir.join("dist"),
                "BurnBurn",
                "0.1.0",
            )
            .unwrap();
            let zip = files.iter().find(|f| f.extension().is_some_and(|e| e == "zip")).unwrap().clone();
            let listing = Command::new("unzip").arg("-l").arg(&zip).output().unwrap();
            let listing = String::from_utf8_lossy(&listing.stdout).into_owned();
            assert!(listing.contains("BurnBurn.app/Contents/Info.plist"), "{listing}");
            assert!(listing.contains("BurnBurn.app/Contents/MacOS/burnburn"), "{listing}");
            let sum = std::fs::read_to_string(format!("{}.sha256", zip.display())).unwrap();
            assert_eq!(sum, checksum_line(&std::fs::read(&zip).unwrap(), zip.file_name().unwrap().to_str().unwrap()));
            zips.push(std::fs::read(&zip).unwrap());
            std::fs::remove_dir_all(&dir).unwrap();
        }
        assert_eq!(zips[0], zips[1], "same inputs, same bytes");
    }

    /// Independent oracle: `hdiutil verify` checks the image checksum.
    #[test]
    fn the_dmg_is_a_valid_compressed_image_with_a_checksum_file() {
        if !mac_only() {
            return;
        }
        let dir = scratch("dmg");
        let app = fake_app(&dir);
        let files = finish(
            &BundleOptions { release: false, zip: false, dmg: true, adhoc_sign: false },
            &app,
            &dir.join("dist"),
            "BurnBurn",
            "0.1.0",
        )
        .unwrap();
        let dmg = files.iter().find(|f| f.extension().is_some_and(|e| e == "dmg")).unwrap();
        let verify = Command::new("hdiutil").arg("verify").arg(dmg).output().unwrap();
        assert!(verify.status.success(), "{}", String::from_utf8_lossy(&verify.stderr));
        assert!(dmg.with_extension("dmg.sha256").is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn without_any_dist_option_nothing_is_written() {
        let dir = scratch("none");
        let app = fake_app(&dir);
        let files = finish(&BundleOptions::default(), &app, &dir.join("dist"), "BurnBurn", "0.1.0").unwrap();
        assert!(files.is_empty());
        assert!(!dir.join("dist").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
