//! Versioned plugin manifest: schema and validation only (IM30, X06). Nothing is loaded.

use burnburn_cli::plugin_manifest::{ManifestError, PLUGIN_API, PluginKind, validate_manifest};

fn manifest(api: &str, machine: bool, network: bool, name: &str, command: &str) -> String {
    format!(
        r#"{{"api":"{api}","name":"{name}","version":"0.3.1","kind":"generator",
            "command":[{command}],
            "capabilities":{{"files":"host-provided","network":{network},"machine":{machine}}}}}"#
    )
}

#[test]
fn valid_manifest_is_accepted() {
    let m = validate_manifest(manifest(PLUGIN_API, false, false, "box-maker", r#""box-maker","--stdio""#).as_bytes());
    let m = m.expect("valid manifest");
    assert_eq!((m.kind, m.name.as_str()), (PluginKind::Generator, "box-maker"));
}

#[test]
fn manifest_of_another_api_version_is_rejected() {
    let r = validate_manifest(manifest("burnburn-plugin/2", false, false, "box-maker", r#""x""#).as_bytes());
    assert_eq!(r, Err(ManifestError::UnsupportedApi { found: "burnburn-plugin/2".into(), supported: PLUGIN_API }));
}

#[test]
fn plugins_never_get_machine_or_network_access() {
    let r = validate_manifest(manifest(PLUGIN_API, true, false, "box-maker", r#""x""#).as_bytes());
    assert_eq!(r, Err(ManifestError::MachineAccessForbidden));
    let r = validate_manifest(manifest(PLUGIN_API, false, true, "box-maker", r#""x""#).as_bytes());
    assert_eq!(r, Err(ManifestError::NetworkAccessForbidden));
}

#[test]
fn malformed_names_commands_and_unknown_fields_are_rejected() {
    assert_eq!(
        validate_manifest(manifest(PLUGIN_API, false, false, "Box Maker!", r#""x""#).as_bytes()),
        Err(ManifestError::InvalidName)
    );
    assert_eq!(
        validate_manifest(manifest(PLUGIN_API, false, false, "box", "").as_bytes()),
        Err(ManifestError::EmptyCommand)
    );
    let extra = manifest(PLUGIN_API, false, false, "box", r#""x""#).replacen('{', r#"{"dlopen":"libbox.dylib","#, 1);
    assert!(matches!(validate_manifest(extra.as_bytes()), Err(ManifestError::Malformed(_))));
}
