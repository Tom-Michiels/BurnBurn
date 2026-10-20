use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use burnburn_document::DocumentSnapshot;
use burnburn_job::MachineProfile;
use burnburn_types::{ContentHash, NonFiniteValue};

/// A file's own identity (not the in-process `DocumentId`). Supplied by the caller (the
/// runtime generates it); never random inside this crate, so tests stay deterministic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectUuid([u8; 16]);

impl ProjectUuid {
    pub fn from_bytes(b: [u8; 16]) -> Self {
        ProjectUuid(b)
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Parses the hyphenated lowercase form `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`.
    pub fn parse(s: &str) -> Option<Self> {
        let hex: String = s.split('-').collect();
        let groups: Vec<usize> = s.split('-').map(str::len).collect();
        if groups != [8, 4, 4, 4, 12] || hex.len() != 32 {
            return None;
        }
        let mut out = [0u8; 16];
        for (i, slot) in out.iter_mut().enumerate() {
            let pair = hex.get(2 * i..2 * i + 2)?;
            if pair.bytes().any(|c| !matches!(c, b'0'..=b'9' | b'a'..=b'f')) {
                return None;
            }
            *slot = u8::from_str_radix(pair, 16).ok()?;
        }
        Some(ProjectUuid(out))
    }
}

impl fmt::Display for ProjectUuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, b) in self.0.iter().enumerate() {
            if matches!(i, 4 | 6 | 8 | 10) {
                f.write_str("-")?;
            }
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

/// Original asset bytes (imported SVG/DXF/images), stored unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub media: String,
    pub bytes: Arc<[u8]>,
}

/// Content-addressed assets: the key is [`crate::asset_hash`] of the bytes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AssetStore {
    assets: BTreeMap<ContentHash, Asset>,
}

impl AssetStore {
    /// Stores `bytes` and returns their content hash. Adding the same bytes twice is a no-op.
    pub fn add(&mut self, media: &str, bytes: &[u8]) -> ContentHash {
        let hash = crate::asset_hash(bytes);
        self.assets.entry(hash).or_insert_with(|| Asset { media: media.to_owned(), bytes: Arc::from(bytes) });
        hash
    }

    pub fn get(&self, hash: &ContentHash) -> Option<&Asset> {
        self.assets.get(hash)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&ContentHash, &Asset)> {
        self.assets.iter()
    }

    pub fn len(&self) -> usize {
        self.assets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }

    pub(crate) fn insert_verified(&mut self, hash: ContentHash, asset: Asset) {
        self.assets.insert(hash, asset);
    }
}

/// Everything a `.burnburn` file stores.
#[derive(Debug, Clone)]
pub struct Project {
    pub uuid: ProjectUuid,
    /// Design objects, sketches and the process layer (operations live in the snapshot, IM13).
    pub document: DocumentSnapshot,
    pub assets: AssetStore,
    /// Machine profiles frozen with the project (P07): inspected against the local profile,
    /// never applied silently ([`crate::compare_profile`]).
    pub profiles: Vec<MachineProfile>,
    /// Frozen production revisions (P08), oldest first; numbers are never reused.
    pub revisions: Vec<FrozenRevision>,
    /// Which library entry each project recipe was made from (C08). Provenance only: it is not
    /// part of the canonical project hash and never changes any setting.
    pub library_links: Vec<LibraryLink>,
}

/// A project recipe that came from a material-library entry: the library, the entry and the
/// entry revision the recipe was copied from. A later library update is *reported* against it
/// ([`crate::material_library::link_status`]) and never applied on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryLink {
    /// The project's recipe (`RecipeId::raw`).
    pub recipe: u64,
    /// Identity of the library the entry lives in.
    pub library: String,
    pub entry: u64,
    /// Entry revision at the time of copying.
    pub revision: u32,
}

/// A frozen copy of the design at one moment ("opnieuw maken als revisie X").
#[derive(Debug, Clone)]
pub struct FrozenRevision {
    pub number: u32,
    pub label: String,
    pub document: DocumentSnapshot,
    /// Project hash of the frozen document with the project's assets (no profiles/revisions).
    pub project_hash: ContentHash,
}

impl Project {
    pub fn new(uuid: ProjectUuid, document: DocumentSnapshot) -> Self {
        Project {
            uuid,
            document,
            assets: AssetStore::default(),
            profiles: Vec::new(),
            revisions: Vec::new(),
            library_links: Vec::new(),
        }
    }

    /// Freezes the current document as the next revision and returns its number. Later edits
    /// of `document` never change it.
    pub fn freeze_revision(&mut self, label: &str) -> Result<u32, NonFiniteValue> {
        let number = self.revisions.last().map_or(1, |r| r.number.saturating_add(1));
        let project_hash = crate::project_hash(&self.frozen_project(self.document.clone()))?;
        self.revisions.push(FrozenRevision {
            number,
            label: label.to_owned(),
            document: self.document.clone(),
            project_hash,
        });
        Ok(number)
    }

    /// `document` with this project's uuid and assets, without profiles or revisions.
    pub(crate) fn frozen_project(&self, document: DocumentSnapshot) -> Project {
        Project {
            uuid: self.uuid,
            document,
            assets: self.assets.clone(),
            profiles: Vec::new(),
            revisions: Vec::new(),
            library_links: Vec::new(),
        }
    }

    pub fn revision(&self, number: u32) -> Option<&FrozenRevision> {
        self.revisions.iter().find(|r| r.number == number)
    }

    /// A project holding revision `number` as its document (same uuid and assets, no profiles
    /// or revisions): what "re-create as revision X" opens. Nothing is compiled or sent.
    pub fn recreate_revision_project(&self, number: u32) -> Option<Project> {
        let r = self.revision(number)?;
        Some(self.frozen_project(r.document.clone()))
    }
}
