# G7-cam (golf 7): Tab Cut Power, nest met exemplaren/gaten/strak, common-line en nerf, recepten in bestaande projecten

Basis: commit 40c71b9 (main na golf 6). Alle commando's met `export PATH="$HOME/.cargo/bin:$PATH"`,
`CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/g7-cam`, `CARGO_INCREMENTAL=0`, toolchain 1.99.1, `--locked`.
Alleen werkelijk uitgevoerde commando's en hun uitkomst staan hieronder. Een test zonder rode fase is zo gemarkeerd.
Geen hardware, geen netwerk, geen gebruikersprojecten aangeraakt.

## 1. Tab Cut Power (C04)

Ontwerp: `TabSpec` kreeg `cut_power_percent: Option<f64>` (0 < p < 100, procent van het bewerkingsvermogen; leeg/0 = overslaan
zoals tot nu toe). Het plan kreeg `PlanEvent::CutPower { to, power, origin }` (vermogen per beweging, absolute vermogensratio).
`Feature::TabCutPower` is standaard `Unverified`, GRBL zegt `Supported`. Het veld is `CutField::TabsPower`.

| Test | Rood (echt gezien) | Red-commando | Groen |
|---|---|---|---|
| `document tests/tab_cut_power.rs` (3: standaard overslaan, bereik (0,100), hash per plaatsing + oude hash blijft) | stub `with_cut_power` nam alles aan en onthield niets: 3 FAILED (`left: None right: Some(25.0)`, hash gelijk) | `cargo test -p burnburn-document --locked --test tab_cut_power` | 3 ok |
| `io tests/g7_tab_power_roundtrip.rs` (3: rondreis + hash, bestand zonder sleutel, bereik met pad) | DTO-veld ontbrak: 2 FAILED (vermogen weg; 0/100/-3/250 geaccepteerd); de test van het oude bestand was al groen | `cargo test -p burnburn-io --locked --test g7_tab_power_roundtrip` | 3 ok |
| `job tests/tab_power.rs` (3: Line-only + positie, planinhoud + bounds, preview-vermogen) | variant zonder gedrag: 3 FAILED. Eén verwachting in de test zelf was fout (15 i.p.v. 20 mm in een wortel); de test is gecorrigeerd, de implementatie niet | `cargo test -p burnburn-job --locked --test tab_power` | 3 ok |
| `cam tests/g7_tab_power.rs` (5 incl. proptest: tabstukken + rest = omtrek, geen laser-uit-gat, elk tabstuk = aandeel x vermogen) | tabs bleven reizen/overgeslagen en geen capability-eis: 3 FAILED. De preflight-test (tabstuk buiten bed) was eerst *leeg* (gaf groen zonder de nieuwe arm); herschreven met één tab van 39 van 40 mm zodat alleen het tabstuk buiten ligt: rood zonder arm in `preflight.rs`, groen met | `cargo test -p burnburn-cam --locked --test g7_tab_power` | 5 ok |
| `machine tests/g7_tab_power.rs` (2: plaatsing verplaatst CutPower mét vermogen; rubber band neemt de top van een tabstuk mee) | `UnsupportedEvent` en een hull zonder apex: 2 FAILED | `cargo test -p burnburn-machine --locked --test g7_tab_power` | 2 ok |
| `gcode tests/capabilities.rs` (matrix 18) + `tests/g7_tab_power.rs` (4: eigen `S`, spindelschaal, S0-weigering, buiten bed) | matrix: `Unverified != Supported`; lowering: 4 FAILED (`NotRepresentable("event")`) | `cargo test -p burnburn-gcode --locked` | ok |
| `ruida tests/g7_tab_power.rs` (1) | `what` was `"event"` i.p.v. `"tab cut power"` | `cargo test -p burnburn-ruida --locked --test g7_tab_power` | ok |
| `presentation tests/g7_cam_tab_power.rs` (5: veld + SetAdvanced + undo, bereik, greyed met reden, plan + preview + pick, RD geweigerd in woorden) | `TabsPower` leeg, geen weigering, geen tabstukken: 5 FAILED | `cargo test -p burnburn-presentation --locked --test g7_cam_tab_power` | 5 ok |
| `macos tests/g7_cam_panels.rs::the_preview_draws_tab_stretches_orange_and_the_pick_names_the_power` | tabstuk rood i.p.v. oranje: FAILED | `cargo test -p burnburn-macos --locked --test g7_cam_panels` | ok |
| `macos g7_cam_panels::the_sheet_has_a_captioned_tab_power_row…`, `windows tests/g7_cam_forms.rs` (2), native `cut_settings_options` (uitgebreid met de tabvermogenrij) | **geen rode fase**: het bijschrift en de rij kwamen uit hetzelfde model dat al af was | idem | ok |

Mutatie/weigeringen: RD weigert op twee lagen (capability `Unverified` bij compile; `LowerError::Unsupported { what: "tab cut power" }`
voor een profiel dat ten onrechte `Supported` zegt). De plan-blob van de historie ondersteunt `CutPower` niet (hij kent maar een
handvol events); alleen RD-jobs worden daar bewaard en die bestaan er niet mee.

## 2. Nest: exemplaren, gaten, strak pakken, nerf, gedeelde rand (C12/C10)

| Test | Rood | Groen |
|---|---|---|
| `cam tests/nest_tight.rs` (7): `tight_packing_closes_the_gap_a_whole_cell_leaves`, `…keeps_the_spacing_exactly` (rood: stub gaf de rasterposities 0/12/24/36), `…fits_a_part_exactly_into_what_is_left_of_a_row`, proptest `placements_never_collide_stay_inside_and_are_deterministic` (raster én tight; AABB-afstand als onafhankelijk orakel) | 2 FAILED op de stub. `a_part_goes_into_the_hole_of_another_part`, keep-out en annulering waren al groen (het domein kon dit; **geen rode fase**). De exact-passen-test is na het contactzoeken geschreven (geen rode fase in `cam`; wel rood gezien in de presentation-test `tight_packing_places_what_the_raster_alone_cannot`) | 7 ok |
| `document tests/g7_nest_copies.rs` (4: orig. verplaatst + kopieën op dezelfde laag in één undo, groep per kopie, onbekend deel verandert niets, alleen kopieën) | stub `NothingChanged`: 4 FAILED | 4 ok |
| `presentation tests/g7_nest_copies.rs` (6) | 6 FAILED (velden zonder gedrag) -> 4 ok, 2 faalden op de definitie van een "set" (`complete_sets` telde `count` i.p.v. één per onderdeel; gecorrigeerd) | 6 ok |
| `presentation tests/g7_nest_holes.rs` (3: binnencontour = gat, kleine delen in het gat + gat verhuist mee, strak) | 3 FAILED | 3 ok |
| `presentation tests/g7_nest_grain.rs` (3) | 2 FAILED, 1 vacuüm groen ("staat rechtop" gold ook zonder nerf; de andere twee bewijzen het) | 3 ok |
| `cam tests/nest_common_line.rs` (4 incl. proptest tegen aangrenzende AABB-zijden) | stub `0.0`: 3 FAILED | 4 ok |
| `presentation tests/g7_nest_common_line.rs` (1) | 1 FAILED | ok |

Bevindingen onderweg (echte bugs of aannames):
- **Native spiegelcheck**: `is_check` in `appkit/panels/schikken.rs` kende de `NestMirror`-check niet; het paneel las een klik als
  "uit". Gezien in de nieuwe native flow (`schikken_panel_native`: spiegel-check, strak, exemplaren, platen, Apply). Opgelost voor
  de drie checks (spiegelen, gaten, strak, common-line).
- **Dubbele namen in een pop-up**: kopieën dragen de naam van hun bron; `NSPopUpButton::addItemWithTitle` voegt gelijke titels samen
  en `fill` las daarna voorbij het eind (NSRangeException, abort). Het model maakt gelijknamige platen nu uniek (`naam (#id)`);
  rood in `macos g7_cam_panels::objects_with_the_same_name_are_told_apart_in_the_choice_of_a_sheet`.
- **Afstand om ronde hoeken**: de proptest vond twee delen op 1,077 mm bij een afstand van 1,09 mm: de ronde offset wordt op de
  productietolerantie (0,01 mm) afgevlakt. De eigenschap gebruikt daarom `CORNER_TOL = 0,012` en de exacte controle laat een
  tiende van de productietolerantie overlap toe (`slack`, getoetst op een kleiner gekrompen deel zodat een uitstekende hoek niet
  doorglipt).
- **Raster vindt geen exacte pas**: een deel dat precies in de rest van een rij past, staat nooit op een rasterpositie; `Tight`
  probeert daarom ook contactposities (onderkant/rand van buren en de bruikbare plaat).
- **Catalogus-test**: `cargo xtask verify` (eerste run) liet `macos tests/l10n.rs::edit_round_keys_are_listed_with_nl_and_en` falen
  (`UndoAction(Arrange(NestCopies))` ontbrak in `known_message_keys`); toegevoegd, daarna `cargo test -p burnburn-macos --locked --test l10n` 7 ok.
- `SetNestOptions` draagt nu een `Box<NestInput>` (de enum-variant werd groter dan 200 bytes: clippy); 14 bestaande testregels
  kregen `Box::new`.

Ontwerp van het exacte pakken: `Packing::{Grid, Tight}` en `nest_packed` (`nest_with` blijft de rasterversie en geeft dezelfde
resultaten als voorheen; alle bestaande nest-tests zijn onveranderd groen). `Tight` zoekt op een optimistisch raster (cel vrij als
het midden vrij is), bevestigt elke positie exact met `boolean` (gekrompen deel tegen het bruikbare gebied en de uitgebreide
buren) op vier subcelposities, probeert daarna contactposities en schuift de uitkomst naar beneden en naar links (bisectie tot de
slack). Deterministisch, annuleerbaar, begrensd door `max_candidates` en 1024 contactposities per deel.

## 3. Common-line cutting en nerf

`AdvancedOptions::common_line: Option<CommonLine { tolerance, min_overlap }>` (alleen Line, sluit dot en toollagen uit, hash
alleen als gezet, `advanced.common_line` in het bestand). Compile hergebruikt `overlap::remove` met de eigen tolerantie en een
minimale overlap; het stuk blijft bij de eerste snijder. `required_features` eist `Feature::CommonLine` (standaard `Unverified`).

| Test | Rood | Groen |
|---|---|---|
| `document tests/g7_common_line.rs` (3) | 3 FAILED (stub accepteerde alles) | 3 ok |
| `io tests/g7_common_line_roundtrip.rs` (3) | 2 FAILED (optie weg, bereik niet geweigerd); de oude-bestandstest was al groen | 3 ok |
| `cam tests/g7_common_line.rs` (6 incl. proptest: raster van n x m rechthoeken met 0,02 mm tussen kolommen = omtrekken min gedeelde randen; tolerantie, minimale overlap, exact rakende delen, weigering zonder bewijs) | 5 FAILED. Twee aannames waren fout en zijn gecorrigeerd: exact rakende delen worden al door `CutOnce` één keer gesneden (met of zonder optie 110 mm), en de toleranties verlengen een gedeeld stuk met de tolerantie (1,5 mm overlap geeft 1,55 mm minder snijden bij 0,05 tolerantie) | 6 ok |
| `presentation tests/g7_cam_common_line_sheet.rs` (5: schakelaar + waarden + één SetAdvanced, bereik per veld, greyed met reden en weigering, plan snijdt de rand eens, sluit dot uit) | 4 FAILED; de dot-test was al groen | 5 ok |
| `macos g7_cam_panels::nest_panel::common_line_and_grain_are_controls…` | alleen compilerrood (controls bestonden niet); **geen gedragsrood** | ok |

Nerf (`NestInput.grain`, hoek in graden, leeg = geen): de plaat krijgt de nerf, elk onderdeel heeft zijn X-as langs de nerf; het
domein (`grain_allows`) bestond al. Rood 2 van 3 in `presentation g7_nest_grain.rs`. De gedeelde rand (`shared_edge_length_mm`)
telt rechte randen van verschillende geplaatste delen op één lijn binnen 0,02 mm; het paneel zegt dat de laag common-line aan
moet hebben om hem één keer te snijden.

## 4. Recepten in de rijen van bestaande projecten (C08)

Fixture `test-data/projects/v2/recipes-in-rows.burnburn` (met de hand geschreven uit gecontroleerde JSON, niet door de encoder;
schema 2 van vóór bibliotheeklinks; sha256 in `manifest.json`): rijen die één recept op verschillende revisies refereren, een
ongebruikt recept en een handmatige rij.

| Test | Uitkomst |
|---|---|
| `io tests/g7_recipe_migration.rs` (4: hash van de fixture, rijen houden referentie en eigen getallen, opnieuw bewaren schrijft dezelfde betekenis en geen `library-links.json`, een receptwijziging raakt geen rij en overleeft bewaren/openen) | **geen rode fase**: het bestaande gedrag klopte. Mutatiecontrole: de revisie bij het laden vastzetten op 1 -> 2 FAILED; teruggezet |
| `presentation tests/g7_recipes_in_rows.rs` (3: het materiaalpaneel zegt per rij waar de instellingen vandaan komen, een later toegepast bibliotheekitem krijgt een nieuw receptid >= 12 en laat de oude rijen staan, bewaren en openen) | **geen rode fase**; groen |

Een eerste aanname in de io-test was fout: het handgeschreven bestand heeft een lege `constraints`; de writer laat lege lijsten weg, dus
de vergelijking negeert dat veld. Geen bug in migratie of recepten gevonden.

## Open (eerlijk)

- Niets van de RD-uitvoer van tabvermogen of common-line is bewezen; beide blijven geweigerd (`docs/hardware/evidence-cam.md`).
- Contractregel IM32 voor common-line (laatste vorm, ná het binnenwerk van beide) is niet gebouwd; de eerste snijder houdt de lijn.
- `Tight` is niet op echte plaatdata gemeten (geen benchmark); het paneel toont rijen voor de eerste 12 onderdelen.
- Handmatig: uiterlijk van de nieuwe velden en regels (Schikken-paneel met veel rijen, Cut Settings met drie nieuwe rijen, de oranje
  tabstukken in de voorvertoning) is niet bekeken. Windows: alleen typecheck, clippy en headless tests.

## Buiten de lane geraakt (additief)

`burnburn-job` (event, preview-veld, `Feature::TabCutPower`), `burnburn-machine` (plaatsing/frame), `burnburn-gcode` (lowering,
profiel), `burnburn-ruida` (benoemde weigering), `burnburn-io` (DTO-velden), `burnburn-document` (`CommonLine`, `NestCopies`,
labels), `burnburn-macos` (`l10n`, `l10n_schikken`, `app_text`, `app_view`, `panels/schikken`, `appkit/panels/schikken`),
bestaande tests (`SetNestOptions` met `Box`, `CutField::ALL` 45, `Feature::ALL` 18, `PreviewSelection.power`, `NestUnplacedRow.count`),
`tests/native/macos/{schikken_panel,cut_settings_options}.rs`.
