# ADR 0004 — Serialisatie, canonieke hash en projectcontainer

- Status: **Aanvaard (Design)**, 17 oktober 2026
- Geldt voor: `burnburn-io` (container, migraties), `burnburn-document` / `burnburn-job` (hashprojectie), alle
  types met `Deserialize`

## Context

Plan: `.burnburn` = manifest + getypeerd project + content-addressed assets + afgeleide previews; expliciet schema
en migraties zodat een Rust-structwijziging niet stil een nieuw formaat wordt; NaN/Infinity aan de grens afwijzen;
hashes over een canonieke representatie, niet over `Debug` of HashMap-volgorde. IM04 eist atomair bewaren met
foutinjectie op write/flush/replace.

## Besluit

### Bibliotheken

| Crate | Versie | Gebruik |
|---|---|---|
| `serde` (+`derive`) | 1.0.229 | DTO's in `io`; `Serialize`/`Deserialize` op kerntypes alleen via `try_from`/`into` |
| `serde_json` | 1.0.154 | `manifest.json`, `document.json` (leesbaar, diffbaar, goed te fuzzen) |
| `sha2` | 0.11.1 | SHA-256 voor `ContentHash` (assets, project, plan) |
| `zip` | 9.0.5, `default-features = false`, `features = ["deflate-flate2-zlib-rs"]` | Containerbestand; alleen *stored* en *deflate*. Default-features zijn verboden (trekken `zstd`-C-code, AES, bzip2 e.d. mee). |

### Containerformaat v1

Eén bestand (zip), zodat bewaren één atomaire `replace` is (een macOS-package-directory kan niet atomair vervangen):

```text
project.burnburn (zip, entries in vaste volgorde)
  manifest.json          { "format": "burnburn-project", "schema": 1, "app": "0.1.0", "project": "<uuid>",
                           "document": "document.json", "assets": { "sha256-<hex>": { "media": "image/png", "bytes": N } } }
  document.json          getypeerd document (DTO-schema v1)
  assets/sha256-<hex>    originele bytes, ongewijzigd (content-addressed; hash bij lezen gecontroleerd)
  previews/…             afgeleid, mag ontbreken of worden weggegooid
```

- Leesgrenzen: maximale entrygrootte, maximaal aantal entries, geen paden met `..`/absolute paden, totale
  uitgepakte grootte begrensd (zip-bom) — onderdeel van het contract en fuzz-target.
- Bewaartraject via de `FileSystem`-poort: `create_temp_in(dir)` → `write_all` → `sync_file` → `replace` →
  `sync_dir`. Faalt een stap, dan blijft het vorige bestand leesbaar (`failed_replace_keeps_last_readable_project`).
  We noemen het resultaat niet "duurzaam bij stroomverlies" zonder OS-specifieke test.
- Recovery: apart bestand in applicatieopslag (`<app-support>/Recovery/<project-uuid>.burnburn`), zelfde formaat,
  zelfde traject; verandert de Save-betekenis niet.

### DTO-scheiding en schema

- `io` bezit versie-specifieke DTO's (`dto::v1::Document`, …) met `#[serde(deny_unknown_fields)]`. Domeintypen
  (`DocumentSnapshot`, `Length`, `Point`) zijn **niet** direct het bestandsformaat; `io` mapt DTO ↔ domein met
  validerende constructors.
- `schema` is een integer in het manifest. Lezen:
  - `schema < huidig` → migratieketen `v1→v2→…` als pure functies op het DTO (of op `serde_json::Value` waar een
    oud DTO niet meer bestaat); origineel bestand blijft ongewijzigd tot de gebruiker bewaart (kopiebehoud).
  - `schema == huidig` → onbekend veld is een fout (`UnknownField`), geen stil verlies.
  - `schema > huidig` → alleen-lezen openen met melding; Save is uitgeschakeld, "Bewaar als kopie" naar een oud
    schema bestaat niet (test: `newer_schema_opens_read_only_without_overwrite`).
- Elke schemaversie krijgt een fixture in `test-data/projects/` met hash en herkomst.

### Eindige waarden

- `serde_json` schrijft `f64::NAN`/`INFINITY` als `null`. Daarom: DTO-velden voor getallen zijn `f64`, en de
  mapping naar domein gaat via `Length::from_mm`/`Point::try_new` → `null` of een niet-eindige waarde geeft
  `InvalidValue { path: "objects[3].rect.width" }`. Schrijven kan geen NaN produceren omdat domeintypes die niet
  bevatten (test: `non_finite_value_in_file_is_rejected_with_path`).
- `-0.0` wordt bij de hashprojectie genormaliseerd naar `0.0`.

### Canonieke hashprojectie

Hashes gaan **niet** over `serde_json`-uitvoer (opmaak, float-formattering en mapvolgorde zijn geen contract).
In plaats daarvan:

```rust
// burnburn-types (trait) — implementaties in document/job/io
pub trait CanonicalEncode {
    fn encode(&self, out: &mut CanonicalWriter);
}
pub struct CanonicalWriter { /* Vec<u8> */ }
impl CanonicalWriter {
    pub fn tag(&mut self, t: &'static str);   // type/field tag, length-prefixed UTF-8
    pub fn u64(&mut self, v: u64);            // little-endian
    pub fn f64(&mut self, v: f64);            // finite only; -0.0 → 0.0; bits little-endian
    pub fn bytes(&mut self, b: &[u8]);        // length-prefixed
    pub fn seq<T: CanonicalEncode>(&mut self, items: impl ExactSizeIterator<Item = T>);
}
```

- Volgorde is altijd expliciet (gesorteerd op ID / `BTreeMap`), nooit `HashMap`-iteratie.
- `ContentHash = SHA-256(b"burnburn-v1\0" ‖ domain-tag ‖ canonical bytes)`; domain-tags `asset`, `project`,
  `plan` voorkomen kruisbotsingen.
- Wat **niet** in de projecthash/planhash zit: transportadres, logtijd, venster-/camerastate, previews, recovery-
  metadata. De planhash bevat wel effectieve parameters, materiaal, plaatsing, machinecapabilities en
  compiler-/fontversies (architectuurdocument).
- `sha2` mag in `types` ontbreken: `CanonicalWriter` levert bytes; hashen gebeurt in `io`/`document`/`job`.
  (Alternatief: `sha2` in `types` toestaan — pure Rust, geen IO. Zie open punt.)

## Gevolgen

- Fuzz-targets (Testing): `container_open`, `manifest_parse`, `document_dto_parse`.
- Golden-hashes in tests zijn vooraf berekend uit een met de hand gecontroleerde canonieke bytereeks, niet uit de
  implementatie zelf.

## Amendement B — fase B (18 oktober 2026)

- **Assetsleutel** blijft `sha256-<hex>` met de *domeingescheiden* hash `SHA-256("burnburn-v1\0" ‖ tag("asset") ‖ bytes)`
  (orchestratorbesluit). Gevolg, te documenteren in `test-data/projects/README.md`: `shasum -a 256` op een asset geeft
  een andere waarde; de fixture-manifesten (`sha256` van het hele `.burnburn`-bestand) blijven kale SHA-256 en zijn
  geen assetsleutels. Het prefix `sha256-` betekent "SHA-256-familie", niet "kale hash van het bestand".
- **`serde_json` met `float_roundtrip`** in io is verplicht (niet optioneel): zonder deze feature kan een `f64` bij
  lezen 1 ulp verschuiven, waardoor de projecthash na heropenen verandert. Test: `reopened_project_has_identical_hash`.
- **`LoadError::NewerSchema`**: openen weigert zonder enige schrijfactie. Het eerdere "alleen-lezen openen" vervalt
  voorlopig; een alleen-lezen weergave vergt een best-effort parse en wordt beslist bij de eerste schema-2-wijziging.
- **`project_hash` → `Result<ContentHash, NonFiniteValue>`**: geen markerhash; akkoord.
- Nieuwe domain-tags (gereserveerd, nieuwe tag = nieuwe regel hier): `asset`, `project`, `plan`, `rd` (RD-bytes,
  voor `Execution.rd_hash`), `eval-sketch`, `eval-trace`, `eval-text` (inputs-hash, contracts §11), `profile`
  (`ProfileRevision`), `history-record` (ADR 0008).
- **`sha2` in `burnburn-types`: ja** (eindreview fase B, DR-17). Er waren vier eigen implementaties van de
  domeingescheiden hash. Eén functie `ContentHash::of(HashDomain, &[u8])` met een gesloten `HashDomain`-enum in types;
  tag-strings: `asset`, `project`, `plan`, `rd`, `source-pixels`, `trace-key`, `eval-sketch`, `eval-text`, `profile`,
  `history-record`. Bestaande `asset`/`project`-hashes blijven bit-gelijk (fixturetest eerst).

## Amendement C — fase C (18 oktober 2026)

- **Schemabeleid:** schema 2 is nog niet uitgebracht en bundelt alle uitbreidingen tot de eerste release
  (sketches, kinds/groups, modifiers, componenten/platen, tekst, generators, trace-definitie, `GeometryRef.sub`). Elke
  uitbreiding is **additief**: optioneel veld, weggelaten als leeg, zodat bestaande bestanden én projecthashes
  bit-gelijk blijven (bewijs: bestaande golden + `…_unchanged_when_empty`-test per nieuw veld). Een build die een
  onbekend veld ziet, meldt `NewerSchema` alleen als het schemanummer hoger is; binnen schema 2 is een onbekend veld een
  parsefout (`deny_unknown_fields`) — acceptabel zolang schema 2 niet is uitgebracht. Bij de eerste release wordt
  schema 2 bevroren (fixture `test-data/projects/v2/…` door Testing) en is elke volgende wijziging schema 3 + migratie.
- **Canonieke staartvelden van een object** (projecthash), in deze vaste volgorde en alleen als niet leeg:
  `groups` → `sub-refs` → `edge-keys`. Nieuwe staartvelden komen achteraan met een regel hier.
- **Nieuwe domain-tags:** `plan-source` (cam-bronhash, `PlanSource.source_hash`), `compiled-job` (`CompiledJobId`,
  ADR 0008), `eval-modifier`, `eval-generator` (inputs-hash van die evaluators). `eval-trace` uit amendement B valt
  samen met `trace-key` (zoals gebouwd); gereserveerd laten. Historieblobs worden gesleuteld met `asset` (opaque bytes,
  na lezen herberekenbaar), naast de semantische ID's in het record.

## Open

- Geen (het `sha2`-punt is beslist in amendement B; het releasemoment van schema 2 ligt bij de eigenaar,
  `docs/design/open-beslissingen.md` B-4).
