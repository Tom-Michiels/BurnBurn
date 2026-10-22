# LF3 Tokens en tekst (met Q2, Q4, Q7, Q8, Q10)

Commando's draaiden met `CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/lf3 CARGO_INCREMENTAL=0`, `--locked`
(behalve `cargo metadata --offline` om `Cargo.lock` bij te werken voor één dev-dependency).

| Test | Verwachte fout (red) | Red-commando en resultaat | Green | Refactor |
|---|---|---|---|---|
| `format::tests::*` (8: komma, groepering, NaN, suffixen, inch, round-trip parse, maat, meervoud) | stub gaf lege tekst | `cargo test -p burnburn-host-models --lib format::`: 8 failed | 8 passed | `findings_text` en `LayerLine` gebruiken `format_count` |
| `lf3_text::{an_array_extent_…, a_large_array_extent_…, the_export_sheet_counts_objects_…}` | oud: "85.000 × 28.500", "1 objecten" | `--test lf3_text`: 3 failed | 3 passed | |
| `lf3_menu::{the_mac_settings_item_…, windows_keeps_its_own_word_…}` | "Voorkeuren…" | `--test lf3_menu`: 2 failed | 2 passed | Windows gebruikt `AppText::Options/OptionsTitle` |
| `style::tests::*` (8: WCAG-formule, contrast 4,5/3 licht+donker, symbool naast kleur, toon per bericht, 4pt-raster, typografie) | stub: zwart, lege symbolen | `--lib style::`: 8 failed | 8 passed | |
| `macos panels::plan_panel_names_the_compiled_plan…` (id_tooltip, "Kopieer id") | veld leeg | `-p burnburn-macos --test panels plan_panel`: 1 failed | 6 passed | |
| `lf3_names::*` (6: standaardnamen EN, NL ongemoeid, gebruikersnaam nooit, alle symbologie-hints NL) | stub gaf input terug | `--test lf3_names`: 4 failed, 2 passed | 6 passed | |
| `l10n_lint::*` (10) | echte bevindingen: "Pas toe"/"Toepassen", "uw/u", "...", "pass(es)" onvertaald | `--test l10n_lint`: 5 failed | 10 passed na fixes | |

Bestaande tests die het oude gedrag vastlegden zijn bijgewerkt (met reden): `g5_cam_panels` (85.000), `file_panels`
en windows `w4b_files` ("1 objects"), native `file_panels`, `panels_and_preview` (planhash nu tooltip), `app_panels`
(menu "Instellingen…"), `text_panels` ("Pas toe").

## Keuzes
- Q7: het document bewaart de NL-namen; `l10n_names::display_name` vertaalt bij weergave alleen een naam die exact
  een standaardnaam is (basiswoord + teller, `Naamloos`, optioneel `(×N)`). Een typenaam als "Rechthoek 1" wordt
  niet te onderscheiden en wordt dus ook vertaald in de EN-UI; "Rechthoek mijn deur" nooit.
  Barcodehints (Engels in de generator) vertalen via `payload_hint_text`; de test loopt alle `Symbology::ALL` af.
- Q10: macOS "Instellingen…"/titel "Instellingen"; Windows "Opties…"/"Opties".
- Q4: de planhash staat niet meer als tekst; tooltip op de titel (macOS) en `PlanPanel::{id_tooltip, copy_id_title}`.
  "Plan van <datum tijd>" kan niet: `PlanViewModel` heeft geen compile-tijdstip.
- Q2: `style::message_tone(MessageKey)`: onbekende sleutel is Error (nooit per ongeluk rustig); Neutral, Warning expliciet.

## Onbevestigd / open
- Knop "Kopieer id" zelf is niet getekend (alleen titel in het model); statuskleur/-symbool op Windows niet getekend
  (`win32` static control); symbool naast de toon is nog niet in de macOS-statusregel getekend.
- Plan-tijdstip vraagt een veld in `PlanViewModel` (andere lane).
- NL-toon nalezen door de gebruiker (plan). Handmatig: screenshots `scratchpad/lf-lf3/light-nl-06-compiled-naamloos.png`,
  `…-33-export-svg-…png` (singular, geen hash) bekeken; menu-item zelf niet offscreen zichtbaar.
- Inline NL/EN-paren in `panels/*.rs` vallen buiten de lint (geen `ALL`-lijst).
