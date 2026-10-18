use burnburn_types::ContentHash;

pub fn hash_to_hex(h: &ContentHash) -> String {
    h.to_hex()
}

pub fn hash_from_hex(s: &str) -> Option<ContentHash> {
    ContentHash::from_hex(s).ok()
}
