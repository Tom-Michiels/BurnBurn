//! Controller-settings backups (IM29, M09) as JSON documents in the journal.
//!
//! Values are typed where the type is known; a value of an unknown type and any unknown
//! top-level field (from a newer writer or a richer reader) are kept verbatim and written back
//! unchanged, so a backup never loses information on its way through this build.

use std::collections::BTreeMap;

use burnburn_machine::{ControllerSettingsSnapshot, MachineId, SettingKey, SettingValue};
use serde_json::{Map, Value, json};

use crate::HistoryError;
use crate::time::WallTime;

pub const SETTINGS_BACKUP_SCHEMA: u64 = 1;

#[derive(Debug, Clone, PartialEq)]
pub enum BackupValue {
    Known(SettingValue),
    /// A value whose type this build does not know, kept as read.
    Preserved(Value),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsBackup {
    pub machine: MachineId,
    pub read_at: WallTime,
    pub values: BTreeMap<String, BackupValue>,
    /// Unknown top-level fields, kept verbatim.
    pub extra: BTreeMap<String, Value>,
}

const KNOWN_FIELDS: &[&str] = &["schema", "machine", "read_at", "values"];

impl SettingsBackup {
    pub fn from_snapshot(snapshot: &ControllerSettingsSnapshot, read_at: WallTime) -> Self {
        SettingsBackup {
            machine: snapshot.machine.clone(),
            read_at,
            values: snapshot.values.iter().map(|(k, v)| (k.0.clone(), BackupValue::Known(v.clone()))).collect(),
            extra: BTreeMap::new(),
        }
    }

    /// The typed part (preserved values are not interpreted).
    pub fn snapshot(&self) -> ControllerSettingsSnapshot {
        let values = self
            .values
            .iter()
            .filter_map(|(k, v)| match v {
                BackupValue::Known(v) => Some((SettingKey(k.clone()), v.clone())),
                BackupValue::Preserved(_) => None,
            })
            .collect();
        ControllerSettingsSnapshot { machine: self.machine.clone(), values }
    }

    pub fn preserved_keys(&self) -> impl Iterator<Item = &str> {
        self.values.iter().filter(|(_, v)| matches!(v, BackupValue::Preserved(_))).map(|(k, _)| k.as_str())
    }

    pub fn to_json(&self) -> Value {
        let values: Map<String, Value> = self
            .values
            .iter()
            .map(|(k, v)| {
                let v = match v {
                    BackupValue::Known(v) => encode_value(v),
                    BackupValue::Preserved(v) => v.clone(),
                };
                (k.clone(), v)
            })
            .collect();
        let mut out: Map<String, Value> = self.extra.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        out.insert("schema".into(), json!(SETTINGS_BACKUP_SCHEMA));
        out.insert("machine".into(), json!(self.machine.as_str()));
        out.insert("read_at".into(), json!(self.read_at.to_rfc3339()));
        out.insert("values".into(), Value::Object(values));
        Value::Object(out)
    }

    pub fn from_json(v: &Value) -> Result<Self, HistoryError> {
        let bad = |what: &str| HistoryError::Encode(format!("settings backup: {what}"));
        let obj = v.as_object().ok_or_else(|| bad("not an object"))?;
        if obj.get("schema").and_then(Value::as_u64) != Some(SETTINGS_BACKUP_SCHEMA) {
            return Err(bad("unknown schema"));
        }
        let machine = MachineId::new(obj.get("machine").and_then(Value::as_str).ok_or_else(|| bad("machine"))?);
        let read_at = obj
            .get("read_at")
            .and_then(Value::as_str)
            .and_then(WallTime::parse_rfc3339)
            .ok_or_else(|| bad("read_at"))?;
        let mut values = BTreeMap::new();
        for (k, v) in obj.get("values").and_then(Value::as_object).ok_or_else(|| bad("values"))? {
            values.insert(
                k.clone(),
                decode_value(v).map_or_else(|| BackupValue::Preserved(v.clone()), BackupValue::Known),
            );
        }
        let extra = obj
            .iter()
            .filter(|(k, _)| !KNOWN_FIELDS.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Ok(SettingsBackup { machine, read_at, values, extra })
    }
}

fn encode_value(v: &SettingValue) -> Value {
    match v {
        SettingValue::Bool(b) => json!({"type": "bool", "value": b}),
        SettingValue::Integer(i) => json!({"type": "integer", "value": i}),
        SettingValue::Micrometres(i) => json!({"type": "micrometres", "value": i}),
        SettingValue::Text(t) => json!({"type": "text", "value": t}),
        SettingValue::Raw(b) => {
            let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
            json!({"type": "raw", "value": hex})
        }
    }
}

/// `None` = not a value of a known type (kept as preserved).
fn decode_value(v: &Value) -> Option<SettingValue> {
    let obj = v.as_object()?;
    if obj.len() != 2 {
        return None;
    }
    let value = obj.get("value")?;
    Some(match obj.get("type")?.as_str()? {
        "bool" => SettingValue::Bool(value.as_bool()?),
        "integer" => SettingValue::Integer(value.as_i64()?),
        "micrometres" => SettingValue::Micrometres(value.as_i64()?),
        "text" => SettingValue::Text(value.as_str()?.to_owned()),
        "raw" => {
            let s = value.as_str()?;
            if s.len() % 2 != 0 || !s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')) {
                return None;
            }
            SettingValue::Raw(
                (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect::<Option<_>>()?,
            )
        }
        _ => return None,
    })
}
