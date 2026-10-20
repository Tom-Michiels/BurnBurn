use burnburn_types::{CanonicalEncode, CanonicalWriter, ContentHash, HashDomain, NonFiniteValue, StableId};

use crate::Project;

/// Content address of original asset bytes (domain `asset`).
pub fn asset_hash(bytes: &[u8]) -> ContentHash {
    ContentHash::of(HashDomain::Asset, bytes)
}

/// Canonical hash of the project's meaning (domain `project`); excludes the file uuid,
/// the id-allocator position, formatting, entry order and recovery metadata.
///
/// Payload layout: `tag("objects")`, `seq` of objects in id order (see `burnburn-document`);
/// `tag("operations")`, `seq` of operations in process-layer order, each in the canonical layout
/// of `burnburn_document::process` (id, name, mode, settings, targets, output flag, recipe ref);
/// `tag("assets")`, `seq` of (`bytes` hash, `str` media) in hash order (the bytes are covered by the hash);
/// only when the process layer has order constraints or recipes: `tag("process")`, `seq` of
/// constraints, `seq` of recipes;
/// only when the document has structure (IM21): `tag("structure")` and the layer's canonical form;
/// only when the document has sketches: `tag("sketches")`, `seq` of (`u64` id, sketch canonical form incl. solution);
/// only when the document has text objects: `tag("texts")`, `seq` of (`u64` id, text source canonical form);
/// only when the document has traces: `tag("traces")`, `seq` of (`u64` id, trace definition);
/// only when the document has generators: `tag("generators")`, `seq` of (`u64` id, generator definition);
/// only when the document has a draw order or locks (W2-L1-bewerken): `tag("arrange")`, `seq` of the
/// ordered ids, `seq` of the locked ids (ascending).
pub fn project_hash(project: &Project) -> Result<ContentHash, NonFiniteValue> {
    let mut w = CanonicalWriter::new();
    w.tag("objects");
    w.seq(project.document.objects().map(|(_, o)| o));
    w.tag("operations");
    let process = project.document.process();
    w.seq(process.operations());
    w.tag("assets");
    w.seq(project.assets.iter().map(|(h, a)| AssetRef(h, &a.media)));
    // Order constraints and recipes (IM13) only when present, so other projects keep their hash.
    if !process.constraints().is_empty() || process.recipes().len() > 0 {
        w.tag("process");
        w.seq(process.constraints().iter());
        w.seq(process.recipes());
    }
    // Sketches (IM11) only when present, so sketch-free projects keep their hash.
    let layer = project.document.sketch_layer();
    if layer.sketches().len() > 0 {
        w.tag("sketches");
        w.seq(layer.sketches().map(|(id, s)| SketchRef(id.raw(), s)));
    }
    // Text sources (IM18) only when present, so text-free projects keep their hash.
    let texts = project.document.text_layer();
    if !texts.is_empty() {
        w.tag("texts");
        w.seq(texts.texts().map(|(id, t)| TextRef(id.raw(), t)));
    }
    // Structure (IM21) only when present, so other projects keep their hash. Evaluation records
    // and provenance are derived state and not hashed.
    let structure = project.document.structure();
    if !structure.is_empty() {
        w.tag("structure");
        structure.encode(&mut w);
    }
    // Traces and generators (C-9, IM22) only when present, so other projects keep their hash.
    // Evaluation records are derived state and not hashed.
    let engines = project.document.engine_layer();
    if engines.traces().len() > 0 {
        w.tag("traces");
        w.seq(engines.traces().map(|(id, t)| IdRef(id.raw(), t)));
    }
    if engines.generators().len() > 0 {
        w.tag("generators");
        w.seq(engines.generators().map(|(id, g)| IdRef(id.raw(), g)));
    }
    // Draw order and locks only when present, so other projects keep their hash.
    let arrange = project.document.arrange();
    if !arrange.is_empty() {
        w.tag("arrange");
        w.seq(arrange.order().iter().map(|id| IdOnly(id.raw())));
        w.seq(arrange.locked().map(|id| IdOnly(id.raw())));
    }
    // Frozen profiles and revisions (IM28) only when present.
    if !project.profiles.is_empty() {
        w.tag("profiles");
        w.seq(project.profiles.iter().map(ProfileRef));
    }
    if !project.revisions.is_empty() {
        w.tag("revisions");
        w.seq(project.revisions.iter().map(RevisionRef));
    }
    // Domain values are finite by construction; a non-finite value is a broken invariant and
    // is reported rather than given a stable-looking hash.
    Ok(ContentHash::of(HashDomain::Project, &w.finish()?))
}

struct AssetRef<'a>(&'a ContentHash, &'a str);

impl CanonicalEncode for AssetRef<'_> {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.bytes(self.0.as_bytes());
        out.str(self.1);
    }
}

struct IdOnly(u64);

impl CanonicalEncode for IdOnly {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.u64(self.0);
    }
}

struct IdRef<'a, T: CanonicalEncode>(u64, &'a T);

impl<T: CanonicalEncode> CanonicalEncode for IdRef<'_, T> {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.u64(self.0);
        self.1.encode(out);
    }
}

struct TextRef<'a>(u64, &'a burnburn_document::text::TextObject);

impl CanonicalEncode for TextRef<'_> {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.u64(self.0);
        self.1.encode(out);
    }
}

struct SketchRef<'a>(u64, &'a burnburn_document::sketch::Sketch);

impl CanonicalEncode for SketchRef<'_> {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.u64(self.0);
        self.1.encode(out);
    }
}

struct ProfileRef<'a>(&'a burnburn_job::MachineProfile);

impl CanonicalEncode for ProfileRef<'_> {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.str(&self.0.revision().id);
        out.u64(u64::from(self.0.revision().revision));
        out.bytes(self.0.revision().content.as_bytes());
    }
}

struct RevisionRef<'a>(&'a crate::FrozenRevision);

impl CanonicalEncode for RevisionRef<'_> {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.u64(u64::from(self.0.number));
        out.str(&self.0.label);
        out.bytes(self.0.project_hash.as_bytes());
    }
}
