use burnburn_document::DocumentSnapshot;
use burnburn_types::{CanonicalWriter, ContentHash, HashDomain, NonFiniteValue};

/// Canonical hash of the document content a plan is compiled from. Domain `plan` (DR-17); the
/// payload starts with `tag("cam-source")`, which no plan payload does (those start with
/// `tag("units-mm-s")`). Then `tag("objects")`, `seq` of objects in id order; `tag("process")`, the process layer
/// (see `burnburn_document::process`). Revision and session state are excluded.
pub fn source_hash(snapshot: &DocumentSnapshot) -> Result<ContentHash, NonFiniteValue> {
    let mut w = CanonicalWriter::new();
    w.tag("cam-source");
    w.tag("objects");
    w.seq(snapshot.objects().map(|(_, o)| o));
    w.tag("process");
    burnburn_types::CanonicalEncode::encode(snapshot.process(), &mut w);
    Ok(ContentHash::of(HashDomain::Plan, &w.finish()?))
}
