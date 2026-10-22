//! Versioned plugin manifest (IM30, X06): schema and validation only. Plugins run out of
//! process (contracts-phase-c §IM30); nothing here loads or starts code. A manifest that asks
//! for machine access is invalid by contract: plugins never get `MachineIntent` access.
//! (Planned home: a small `burnburn-plugin-api` crate once the first plugin test exists.)

use serde::Deserialize;

/// The only plugin API this build speaks.
pub const PLUGIN_API: &str = "burnburn-plugin/1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginKind {
    Generator,
    Importer,
    Exporter,
}

/// File access a plugin declares; shown to the user, enforced by the runtime host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileAccess {
    None,
    /// Only the input the host passes in.
    HostProvided,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub files: FileAccess,
    pub network: bool,
    pub machine: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub api: String,
    pub name: String,
    pub version: String,
    pub kind: PluginKind,
    /// Executable and arguments, started by the runtime with a timeout.
    pub command: Vec<String>,
    pub capabilities: Capabilities,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    Malformed(String),
    UnsupportedApi {
        found: String,
        supported: &'static str,
    },
    InvalidName,
    EmptyCommand,
    /// Plugins never get machine access.
    MachineAccessForbidden,
    /// Network access is not granted to plugins in this version.
    NetworkAccessForbidden,
}

pub fn validate_manifest(bytes: &[u8]) -> Result<PluginManifest, ManifestError> {
    // The api version is checked first, so a newer manifest gets a clear answer even when the
    // rest of its schema is unknown to this build.
    let raw: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| ManifestError::Malformed(e.to_string()))?;
    match raw.get("api").and_then(serde_json::Value::as_str) {
        Some(PLUGIN_API) => {}
        Some(other) => return Err(ManifestError::UnsupportedApi { found: other.to_owned(), supported: PLUGIN_API }),
        None => return Err(ManifestError::Malformed("missing api".into())),
    }
    let m: PluginManifest = serde_json::from_value(raw).map_err(|e| ManifestError::Malformed(e.to_string()))?;
    let name_ok = !m.name.is_empty()
        && m.name.len() <= 64
        && m.name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if !name_ok {
        return Err(ManifestError::InvalidName);
    }
    if m.command.first().is_none_or(String::is_empty) {
        return Err(ManifestError::EmptyCommand);
    }
    if m.capabilities.machine {
        return Err(ManifestError::MachineAccessForbidden);
    }
    if m.capabilities.network {
        return Err(ManifestError::NetworkAccessForbidden);
    }
    Ok(m)
}

/// A right shown to the user in the extensions panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessRight {
    NoFiles,
    FilesHostProvided,
    Network,
    Machine,
}

impl PluginManifest {
    /// The access rights this plugin has, for the extensions panel.
    pub fn access_summary(&self) -> Vec<AccessRight> {
        let mut rights = vec![match self.capabilities.files {
            FileAccess::None => AccessRight::NoFiles,
            FileAccess::HostProvided => AccessRight::FilesHostProvided,
        }];
        if self.capabilities.network {
            rights.push(AccessRight::Network);
        }
        if self.capabilities.machine {
            rights.push(AccessRight::Machine);
        }
        rights
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginPath {
    pub closed: bool,
    pub points: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PluginOutput {
    pub last_progress: Option<f64>,
    pub paths: Vec<PluginPath>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginProtocolError {
    MissingHello,
    HelloMismatch,
    UnsupportedApi { found: String },
    ForbiddenMessage(String),
    UnknownMessage(String),
    KindMismatch,
    PluginFailed { code: String, message: String },
    Truncated,
    TrailingData,
    LineTooLong,
    Malformed(String),
}

/// Longest line of plugin output, in bytes.
pub const MAX_LINE_BYTES: usize = 1 << 20;
const MAX_LINES: usize = 100_000;
const MAX_POINTS: usize = 1_000_000;
/// Message types that would reach a machine, a job or the network. A plugin has no way to say
/// them; they get their own refusal rather than "unknown".
const FORBIDDEN_TYPES: &[&str] =
    &["machine", "job", "upload", "start", "send", "connect", "frame", "jog", "home", "stop", "network", "exec"];

fn malformed(m: impl Into<String>) -> PluginProtocolError {
    PluginProtocolError::Malformed(m.into())
}

fn text_field(v: &serde_json::Value, key: &str) -> String {
    v.get(key).and_then(serde_json::Value::as_str).unwrap_or_default().to_owned()
}

fn parse_paths(v: &serde_json::Value, total_points: &mut usize) -> Result<Vec<PluginPath>, PluginProtocolError> {
    let paths =
        v.get("paths").and_then(serde_json::Value::as_array).ok_or_else(|| malformed("paths needs an array"))?;
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        let closed =
            path.get("closed").and_then(serde_json::Value::as_bool).ok_or_else(|| malformed("path needs closed"))?;
        let points =
            path.get("points").and_then(serde_json::Value::as_array).ok_or_else(|| malformed("path needs points"))?;
        if points.is_empty() {
            return Err(malformed("a path needs at least one point"));
        }
        *total_points += points.len();
        if *total_points > MAX_POINTS {
            return Err(malformed("too many points"));
        }
        let mut pts = Vec::with_capacity(points.len());
        for point in points {
            let xy = point.as_array().filter(|a| a.len() == 2).ok_or_else(|| malformed("a point is [x, y]"))?;
            let coordinate = |i: usize| {
                xy.get(i)
                    .and_then(serde_json::Value::as_f64)
                    .filter(|c| c.is_finite())
                    .ok_or_else(|| malformed("coordinates must be finite numbers"))
            };
            pts.push((coordinate(0)?, coordinate(1)?));
        }
        out.push(PluginPath { closed, points: pts });
    }
    Ok(out)
}

/// Checks and reads everything a plugin wrote to its output (JSON Lines, API `burnburn-plugin/1`).
///
/// The first line is a `hello` naming the manifest's API, name and version; then `progress` and
/// (for generators and importers) `paths` lines; the last line is `done`. An `error` line, a
/// missing `done`, a malformed line or any message that is not part of the contract makes the
/// whole run fail with no partial result. The result type holds geometry only: a plugin cannot
/// name a job, a machine or an upload, so it cannot replace a frozen job.
pub fn parse_plugin_output(manifest: &PluginManifest, output: &[u8]) -> Result<PluginOutput, PluginProtocolError> {
    let mut lines: Vec<&[u8]> = output.split(|b| *b == b'\n').collect();
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    if lines.len() > MAX_LINES {
        return Err(malformed("too many lines"));
    }
    let mut result = PluginOutput::default();
    let mut points = 0usize;
    let mut greeted = false;
    let mut done = false;
    for raw in lines {
        if raw.len() > MAX_LINE_BYTES {
            return Err(PluginProtocolError::LineTooLong);
        }
        let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
        let text = std::str::from_utf8(raw).map_err(|_| malformed("output is not UTF-8"))?;
        let value: serde_json::Value = serde_json::from_str(text).map_err(|e| malformed(e.to_string()))?;
        let kind = value
            .as_object()
            .and_then(|o| o.get("type"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| malformed("every line is an object with a type"))?
            .to_owned();
        if done {
            return Err(PluginProtocolError::TrailingData);
        }
        if !greeted {
            if kind != "hello" {
                return Err(PluginProtocolError::MissingHello);
            }
            let api = text_field(&value, "api");
            if api != PLUGIN_API {
                return Err(PluginProtocolError::UnsupportedApi { found: api });
            }
            if text_field(&value, "name") != manifest.name || text_field(&value, "version") != manifest.version {
                return Err(PluginProtocolError::HelloMismatch);
            }
            greeted = true;
            continue;
        }
        match kind.as_str() {
            "progress" => {
                let fraction = value
                    .get("fraction")
                    .and_then(serde_json::Value::as_f64)
                    .filter(|f| (0.0..=1.0).contains(f))
                    .ok_or_else(|| malformed("progress fraction is a number from 0 to 1"))?;
                result.last_progress = Some(fraction);
            }
            "paths" => {
                if !matches!(manifest.kind, PluginKind::Generator | PluginKind::Importer) {
                    return Err(PluginProtocolError::KindMismatch);
                }
                result.paths.extend(parse_paths(&value, &mut points)?);
            }
            "done" => done = true,
            "error" => {
                return Err(PluginProtocolError::PluginFailed {
                    code: text_field(&value, "code"),
                    message: text_field(&value, "message"),
                });
            }
            other if FORBIDDEN_TYPES.contains(&other) => {
                return Err(PluginProtocolError::ForbiddenMessage(other.to_owned()));
            }
            other => return Err(PluginProtocolError::UnknownMessage(other.to_owned())),
        }
    }
    if !greeted {
        return Err(PluginProtocolError::MissingHello);
    }
    if !done {
        return Err(PluginProtocolError::Truncated);
    }
    Ok(result)
}
