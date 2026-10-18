//! Controller settings snapshots and their difference (IM29, M09). Reading settings is a
//! sequence of idempotent queries; writing is a separate, explicitly confirmed action with a
//! backup in the history first (not implemented on the wire: no verified register map).

use std::collections::BTreeMap;

use crate::ids::MachineId;

/// A setting's address or name, e.g. `"bed.width"` or `"register:0x0026"`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SettingKey(pub String);

/// Typed value. `Raw` keeps bytes whose meaning is not established, unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingValue {
    Bool(bool),
    Integer(i64),
    /// Lengths in µm (controller-native unit), never converted on read.
    Micrometres(i64),
    Text(String),
    Raw(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerSettingsSnapshot {
    pub machine: MachineId,
    pub values: BTreeMap<SettingKey, SettingValue>,
}

/// One difference between two snapshots: `before`/`after` is `None` where the key is absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingChange {
    pub key: SettingKey,
    pub before: Option<SettingValue>,
    pub after: Option<SettingValue>,
}

/// Differences from `a` to `b`, in key order. Equal values are not listed.
pub fn diff(a: &ControllerSettingsSnapshot, b: &ControllerSettingsSnapshot) -> Vec<SettingChange> {
    let keys: std::collections::BTreeSet<&SettingKey> = a.values.keys().chain(b.values.keys()).collect();
    keys.into_iter()
        .filter_map(|k| {
            let (before, after) = (a.values.get(k), b.values.get(k));
            (before != after).then(|| SettingChange { key: k.clone(), before: before.cloned(), after: after.cloned() })
        })
        .collect()
}
