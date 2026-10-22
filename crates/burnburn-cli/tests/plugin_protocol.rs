//! X06 (G5-tekst-api): the version-bound plugin contract. The manifest declares API version and
//! capabilities; the out-of-process protocol is JSON Lines; plugins have no machine access and no
//! way to express one. There is no loader in the application (decision B-5): this tests the
//! contract only, on bytes a plugin would have written.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_cli::plugin_manifest::{
    AccessRight, PLUGIN_API, PluginKind, PluginManifest, PluginProtocolError, parse_plugin_output, validate_manifest,
};
use common::*;

fn manifest(kind: &str) -> PluginManifest {
    let json = format!(
        r#"{{"api":"{PLUGIN_API}","name":"box-maker","version":"0.3.1","kind":"{kind}",
            "command":["box-maker","--stdio"],
            "capabilities":{{"files":"host-provided","network":false,"machine":false}}}}"#
    );
    validate_manifest(json.as_bytes()).unwrap()
}

const HELLO: &str = r#"{"type":"hello","api":"burnburn-plugin/1","name":"box-maker","version":"0.3.1"}"#;
const DONE: &str = r#"{"type":"done"}"#;
const SQUARE: &str = r#"{"type":"paths","paths":[{"closed":true,"points":[[0,0],[10,0],[10,10],[0,10]]}]}"#;

fn output(lines: &[&str]) -> Vec<u8> {
    lines.join("\n").into_bytes()
}

// ---- manifest: schema, capabilities, access summary -------------------------------------------

#[test]
fn the_schema_file_lists_exactly_the_fields_the_validator_accepts() {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../../../docs/design/plugin-manifest.schema.json")).unwrap();
    let props = |v: &serde_json::Value| -> Vec<String> {
        let mut keys: Vec<String> = v["properties"].as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    };
    assert_eq!(props(&schema), ["api", "capabilities", "command", "kind", "name", "version"]);
    assert_eq!(props(&schema["properties"]["capabilities"]), ["files", "machine", "network"]);
    assert_eq!(schema["properties"]["api"]["const"], PLUGIN_API);
    assert_eq!(schema["properties"]["capabilities"]["properties"]["machine"]["const"], false);
    assert_eq!(schema["properties"]["capabilities"]["properties"]["network"]["const"], false);
    assert_eq!(schema["additionalProperties"], false);
    // Every example in the schema file is accepted by the validator.
    for example in schema["examples"].as_array().unwrap() {
        validate_manifest(example.to_string().as_bytes()).unwrap();
    }
}

#[test]
fn the_access_summary_for_the_extensions_panel_names_every_right_and_never_machine_access() {
    let m = manifest("generator");
    assert_eq!(m.access_summary(), [AccessRight::FilesHostProvided]);
    let mut none = m.clone();
    none.capabilities.files = burnburn_cli::plugin_manifest::FileAccess::None;
    assert_eq!(none.access_summary(), [AccessRight::NoFiles]);
    for rights in [m.access_summary(), none.access_summary()] {
        assert!(!rights.contains(&AccessRight::Network) && !rights.contains(&AccessRight::Machine));
    }
}

// ---- protocol ---------------------------------------------------------------------------------

#[test]
fn a_well_formed_generator_run_yields_paths_in_millimetres() {
    let m = manifest("generator");
    let progress = r#"{"type":"progress","fraction":0.5}"#;
    let out = parse_plugin_output(&m, &output(&[HELLO, progress, SQUARE, DONE])).unwrap();
    assert_eq!(out.last_progress, Some(0.5));
    assert_eq!(out.paths.len(), 1);
    assert!(out.paths[0].closed);
    assert_eq!(out.paths[0].points, [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
}

#[test]
fn the_first_line_must_be_a_hello_that_matches_the_manifest() {
    let m = manifest("generator");
    assert_eq!(parse_plugin_output(&m, &output(&[SQUARE, DONE])), Err(PluginProtocolError::MissingHello));
    let other_name = HELLO.replace("box-maker", "andere");
    assert_eq!(parse_plugin_output(&m, &output(&[&other_name, DONE])), Err(PluginProtocolError::HelloMismatch));
    let other_version = HELLO.replace("0.3.1", "9.9.9");
    assert_eq!(parse_plugin_output(&m, &output(&[&other_version, DONE])), Err(PluginProtocolError::HelloMismatch));
    let other_api = HELLO.replace("plugin/1", "plugin/2");
    assert!(matches!(
        parse_plugin_output(&m, &output(&[&other_api, DONE])),
        Err(PluginProtocolError::UnsupportedApi { .. })
    ));
}

#[test]
fn a_plugin_cannot_express_a_machine_action() {
    let m = manifest("generator");
    for kind in ["machine", "job", "upload", "start", "send", "connect", "frame", "jog", "home", "network", "exec"] {
        let line = format!(r#"{{"type":"{kind}","target":"ruida-bench-1"}}"#);
        assert_eq!(
            parse_plugin_output(&m, &output(&[HELLO, &line, DONE])),
            Err(PluginProtocolError::ForbiddenMessage(kind.into())),
            "{kind}"
        );
    }
    // Anything else it makes up is merely unknown.
    assert_eq!(
        parse_plugin_output(&m, &output(&[HELLO, r#"{"type":"banana"}"#, DONE])),
        Err(PluginProtocolError::UnknownMessage("banana".into()))
    );
}

#[test]
fn the_output_kind_must_fit_the_plugin_kind() {
    let exporter = manifest("exporter");
    assert_eq!(exporter.kind, PluginKind::Exporter);
    assert_eq!(parse_plugin_output(&exporter, &output(&[HELLO, SQUARE, DONE])), Err(PluginProtocolError::KindMismatch));
    let generator = manifest("generator");
    assert!(parse_plugin_output(&generator, &output(&[HELLO, DONE])).unwrap().paths.is_empty());
}

#[test]
fn a_reported_error_is_a_failure_and_delivers_no_partial_result() {
    let m = manifest("generator");
    let error = r#"{"type":"error","code":"bad-parameter","message":"width must be positive"}"#;
    // The paths before the error are not handed out.
    assert_eq!(
        parse_plugin_output(&m, &output(&[HELLO, SQUARE, error])),
        Err(PluginProtocolError::PluginFailed {
            code: "bad-parameter".into(),
            message: "width must be positive".into()
        })
    );
}

#[test]
fn truncated_trailing_malformed_and_oversized_output_is_refused() {
    let m = manifest("generator");
    assert_eq!(parse_plugin_output(&m, &output(&[HELLO, SQUARE])), Err(PluginProtocolError::Truncated));
    assert_eq!(parse_plugin_output(&m, b""), Err(PluginProtocolError::MissingHello));
    assert_eq!(parse_plugin_output(&m, &output(&[HELLO, DONE, SQUARE])), Err(PluginProtocolError::TrailingData));
    assert!(matches!(
        parse_plugin_output(&m, &output(&[HELLO, "{not json", DONE])),
        Err(PluginProtocolError::Malformed(_))
    ));
    assert!(matches!(
        parse_plugin_output(&m, &output(&[HELLO, "[1,2]", DONE])),
        Err(PluginProtocolError::Malformed(_))
    ));
    assert!(matches!(parse_plugin_output(&m, &[0xff, 0xfe, b'\n']), Err(PluginProtocolError::Malformed(_))));
    let long = format!(r#"{{"type":"progress","fraction":0.1,"pad":"{}"}}"#, "x".repeat(1 << 20));
    assert_eq!(parse_plugin_output(&m, &output(&[HELLO, &long, DONE])), Err(PluginProtocolError::LineTooLong));
    for bad in [
        r#"{"type":"progress","fraction":1.5}"#,
        r#"{"type":"progress","fraction":-0.1}"#,
        r#"{"type":"paths","paths":[{"closed":true,"points":[[0,0],[1,1e999]]}]}"#,
        r#"{"type":"paths","paths":[{"closed":true,"points":[[0,0,0]]}]}"#,
        r#"{"type":"paths","paths":[{"closed":true,"points":[]}]}"#,
    ] {
        assert!(
            matches!(parse_plugin_output(&m, &output(&[HELLO, bad, DONE])), Err(PluginProtocolError::Malformed(_))),
            "{bad}"
        );
    }
}

// ---- a plugin failure never replaces a frozen job ---------------------------------------------

#[test]
fn plugin_failure_never_replaces_frozen_job() {
    let r = rig();
    with_project(&r);
    let compiled =
        r.run("compile /work/plaat.burnburn --profile /work/bench.json --out /work/p.rd --history /h").unwrap();
    let rd_before = r.file("/work/p.rd").unwrap();
    let history_before = r.run("history --history /h").unwrap().stdout;
    assert!(history_before.contains(&format!("job {}", compiled.job.unwrap())));

    // A plugin run that crashes midway, one that reports an error, and one that is cut off.
    let m = manifest("generator");
    let error = r#"{"type":"error","code":"crash","message":"boom"}"#;
    for bytes in [output(&[HELLO, SQUARE, error]), output(&[HELLO, SQUARE]), output(&[error])] {
        assert!(parse_plugin_output(&m, &bytes).is_err());
    }

    // The frozen job, its RD bytes and the history are exactly as before; nothing in the
    // plugin contract can name a job, so nothing could have replaced it.
    assert_eq!(r.file("/work/p.rd").unwrap(), rd_before);
    assert_eq!(r.run("history --history /h").unwrap().stdout, history_before);
}
