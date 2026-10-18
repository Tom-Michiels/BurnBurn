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
