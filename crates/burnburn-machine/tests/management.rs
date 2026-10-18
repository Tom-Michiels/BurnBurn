//! Controller files and settings (IM29, M09): explicit intents, proven capabilities, typed diff.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use burnburn_machine::{
    Capability, ControllerFileIntent, ControllerFileName, ControllerSettingsSnapshot, FileCapabilities, FileOpError,
    MachineId, SettingChange, SettingKey, SettingValue, authorize, diff,
};

fn name(s: &str) -> ControllerFileName {
    ControllerFileName::new(s).unwrap()
}

#[test]
fn controller_file_actions_need_explicit_intent_and_proven_capability() {
    let unproven = FileCapabilities::default();
    for intent in [ControllerFileIntent::List, ControllerFileIntent::Delete { name: name("PLAAT01") }] {
        assert!(matches!(authorize(&intent, &unproven), Err(FileOpError::Unverified { .. })), "{intent:?}");
    }
    let caps = FileCapabilities {
        list: Capability::Supported,
        metadata: Capability::Supported,
        upload_named: Capability::Unsupported,
        run: Capability::Unverified { reason: "seen in OSS only".into() },
        delete: Capability::Unverified { reason: "seen in OSS only".into() },
    };
    assert_eq!(authorize(&ControllerFileIntent::List, &caps), Ok(()));
    assert_eq!(
        authorize(&ControllerFileIntent::Delete { name: name("PLAAT01") }, &caps),
        Err(FileOpError::Unverified { operation: "delete", reason: "seen in OSS only".into() })
    );
    assert!(ControllerFileName::new("").is_err());
    assert_eq!(ControllerFileName::new("a\0b"), Err(FileOpError::InvalidName));
    assert_eq!(ControllerFileName::new("plaat-é"), Err(FileOpError::InvalidName));
}

fn snapshot(values: &[(&str, SettingValue)]) -> ControllerSettingsSnapshot {
    ControllerSettingsSnapshot {
        machine: MachineId::new("bench"),
        values: values.iter().map(|(k, v)| (SettingKey((*k).into()), v.clone())).collect::<BTreeMap<_, _>>(),
    }
}

#[test]
fn settings_diff_lists_changed_added_and_removed_in_key_order() {
    let before = snapshot(&[
        ("bed.width", SettingValue::Micrometres(1_500_000)),
        ("register:0x0026", SettingValue::Raw(vec![1, 2, 3])),
        ("z.enabled", SettingValue::Bool(false)),
        ("name", SettingValue::Text("RDC".into())),
    ]);
    let after = snapshot(&[
        ("bed.width", SettingValue::Micrometres(1_400_000)),
        ("register:0x0026", SettingValue::Raw(vec![1, 2, 3])),
        ("a.new", SettingValue::Integer(4)),
        ("name", SettingValue::Text("RDC".into())),
    ]);
    let k = |s: &str| SettingKey(s.into());
    assert_eq!(
        diff(&before, &after),
        vec![
            SettingChange { key: k("a.new"), before: None, after: Some(SettingValue::Integer(4)) },
            SettingChange {
                key: k("bed.width"),
                before: Some(SettingValue::Micrometres(1_500_000)),
                after: Some(SettingValue::Micrometres(1_400_000)),
            },
            SettingChange { key: k("z.enabled"), before: Some(SettingValue::Bool(false)), after: None },
        ]
    );
    assert!(diff(&before, &before).is_empty());
}
