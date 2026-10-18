//! History identities and their text forms in the journal.

use std::fmt;

use burnburn_types::ContentHash;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

macro_rules! hash_id {
    ($(#[$doc:meta])* $name:ident, $prefix:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub(crate) ContentHash);

        impl $name {
            pub fn hash(&self) -> ContentHash {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.0.to_hex())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                ContentHash::from_hex(&s).map($name).map_err(serde::de::Error::custom)
            }
        }
    };
}

hash_id!(
    /// Identity of one frozen compiled job: hash over plan id, RD hash and profile content.
    CompiledJobId,
    "job-"
);
hash_id!(
    /// Content address of a blob (`ContentHash::of(Asset, bytes)`).
    BlobKey,
    "blob-"
);

impl BlobKey {
    pub(crate) fn of(bytes: &[u8]) -> BlobKey {
        BlobKey(ContentHash::of(burnburn_types::HashDomain::Asset, bytes))
    }

    /// File name inside `blobs/`.
    pub fn file_name(&self) -> String {
        format!("sha256-{}", self.0.to_hex())
    }
}

/// Global, strictly increasing record number across segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RecordSeq(pub(crate) u64);

impl RecordSeq {
    pub fn raw(self) -> u64 {
        self.0
    }
}

/// One queue line: a job and how many pieces are planned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct QueueItemId(pub(crate) u64);

impl QueueItemId {
    pub fn raw(self) -> u64 {
        self.0
    }
}

/// Idempotency key of one user action (one click, IM31). Text form: 32 lowercase hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActionId(u128);

impl ActionId {
    pub fn from_raw(raw: u128) -> Self {
        ActionId(raw)
    }

    pub fn raw(self) -> u128 {
        self.0
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

impl Serialize for ActionId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ActionId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        if s.len() != 32 || !s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')) {
            return Err(serde::de::Error::custom("action id must be 32 lowercase hex digits"));
        }
        u128::from_str_radix(&s, 16).map(ActionId).map_err(serde::de::Error::custom)
    }
}

/// `ExecutionId` (machine, serde-free) as a JSON number.
pub(crate) mod execution_id {
    use burnburn_machine::ExecutionId;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(id: &ExecutionId, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(id.raw())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<ExecutionId, D::Error> {
        u64::deserialize(d).map(ExecutionId::from_raw)
    }
}

/// `MachineId` (machine, serde-free) as a JSON string.
pub(crate) mod machine_id {
    use burnburn_machine::MachineId;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(id: &MachineId, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(id.as_str())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<MachineId, D::Error> {
        String::deserialize(d).map(MachineId::new)
    }
}

/// `ContentHash` (types, serde-free) as hex.
pub(crate) mod content_hash {
    use burnburn_types::ContentHash;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(h: &ContentHash, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&h.to_hex())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<ContentHash, D::Error> {
        let s = String::deserialize(d)?;
        ContentHash::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

/// `Option<ContentHash>` as hex or `null`.
pub(crate) mod opt_content_hash {
    use burnburn_types::ContentHash;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(h: &Option<ContentHash>, s: S) -> Result<S::Ok, S::Error> {
        match h {
            Some(h) => s.serialize_some(&h.to_hex()),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<ContentHash>, D::Error> {
        Option::<String>::deserialize(d)?
            .map(|s| ContentHash::from_hex(&s).map_err(serde::de::Error::custom))
            .transpose()
    }
}

/// `ProfileRevision` (job, serde-free) as `{id, revision, content}`.
pub(crate) mod profile_revision {
    use burnburn_job::ProfileRevision;
    use burnburn_types::ContentHash;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    struct Dto {
        id: String,
        revision: u32,
        content: String,
    }

    pub fn serialize<S: Serializer>(p: &ProfileRevision, s: S) -> Result<S::Ok, S::Error> {
        Dto { id: p.id.clone(), revision: p.revision, content: p.content.to_hex() }.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<ProfileRevision, D::Error> {
        let dto = Dto::deserialize(d)?;
        let content = ContentHash::from_hex(&dto.content).map_err(serde::de::Error::custom)?;
        Ok(ProfileRevision { id: dto.id, revision: dto.revision, content })
    }
}
