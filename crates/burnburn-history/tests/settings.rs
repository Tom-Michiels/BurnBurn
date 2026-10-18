//! Settings backups keep unknown fields and value types verbatim (IM29, M09).
#![allow(clippy::unwrap_used)]

mod common;

use burnburn_history::{BackupValue, SettingsBackup};
use burnburn_machine::{MachineId, SettingKey, SettingValue};
use common::*;
use serde_json::json;

fn newer_writer_backup() -> serde_json::Value {
    json!({
        "schema": 1,
        "machine": "ruida-bench-1",
        "read_at": "2026-10-18T09:30:00.000Z",
        "values": {
            "bed.width": {"type": "micrometres", "value": 1500000},
            "register:0x0026": {"type": "raw", "value": "0a0b"},
            "z.enabled": {"type": "bool", "value": false},
            "laser.curve": {"type": "lookup-table", "value": [[0, 0], [50, 48]]}
        },
        "controller_notes": {"source": "panel export", "pages": 3}
    })
}

#[test]
fn settings_backup_roundtrip_preserves_unknown_fields_and_types() {
    let original = newer_writer_backup();
    let backup = SettingsBackup::from_json(&original).unwrap();
    assert_eq!(backup.to_json(), original, "nothing is lost or reformatted on the way through");
    assert_eq!(backup.preserved_keys().collect::<Vec<_>>(), vec!["laser.curve"]);
    let typed = backup.snapshot();
    assert_eq!(typed.values.get(&SettingKey("bed.width".into())), Some(&SettingValue::Micrometres(1_500_000)));
    assert_eq!(typed.values.get(&SettingKey("register:0x0026".into())), Some(&SettingValue::Raw(vec![10, 11])));
    assert!(matches!(backup.values["laser.curve"], BackupValue::Preserved(_)));
}

#[test]
fn settings_backup_is_journaled_and_read_back_per_machine() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let backup = SettingsBackup::from_json(&newer_writer_backup()).unwrap();
    h.record_settings_backup(&fs, &backup, t(0)).unwrap();
    let (h, _) = open(&fs);
    let found = h.settings_backups(&MachineId::new("ruida-bench-1"));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1.as_ref().unwrap(), &backup);
    assert!(h.settings_backups(&MachineId::new("other")).is_empty());
}
