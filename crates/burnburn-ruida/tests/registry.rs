//! The embedded registry table must equal the research JSON (no silent drift).

use std::collections::BTreeMap;
use std::path::PathBuf;

use burnburn_ruida::registry_entries;

#[test]
fn embedded_registry_matches_research_json() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../research/lightburn/ruida/command-registry.json");
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut expected: BTreeMap<Vec<u8>, Vec<String>> = BTreeMap::new();
    for c in json["commands"].as_array().unwrap() {
        let bytes = c["hex"].as_str().unwrap().split(' ').map(|h| u8::from_str_radix(h, 16).unwrap()).collect();
        expected.entry(bytes).or_default().push(c["name"].as_str().unwrap().to_owned());
    }
    assert_eq!(json["commands"].as_array().unwrap().len(), 200);

    let actual: BTreeMap<Vec<u8>, Vec<String>> =
        registry_entries().iter().map(|(p, l)| (p.to_vec(), l.split('/').map(str::to_owned).collect())).collect();
    assert_eq!(actual.len(), registry_entries().len(), "duplicate prefixes in table");
    assert_eq!(actual, expected);
}
