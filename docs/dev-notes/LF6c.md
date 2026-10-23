# LF6c: G5/G6/G7-panelen, Schikken, Voorkeuren, Machinehub, barcode en Hoeken afronden naar de formulierbouwer

Basis `0b517ed`. Alle commando's met `CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/lf6c CARGO_INCREMENTAL=0`; native tests met
`BURNBURN_NATIVE_QUIET=1` (stille vensters, nooit `--native`). Screenshots offscreen via `gui_snapshots` naar
`/private/tmp/claude-501/-Users-tom--claude-spares-spare-7SCiVl/b7379f7e-55e7-4285-a7c2-e3f3d2728135/scratchpad/lf-lf6c/{before,after}`.

## Wat er is

- `form.rs`: `FormSection.tab` (+ `on_tab`, `FormSpec::tabs()`), `FormSpec.notice` en `notice_slot` (een melding voor het hele
  formulier boven de knoppen; met slot blijft de vorm gelijk als de melding komt en gaat), `Control::{Log, Picture}`,
  `FormRow.invalid` (bijschrift rood bij een geweigerde waarde), `split_caption` (`Tussenruimte (mm)` wordt label + eenheid;
  `(s, 0 = uit)` wordt eenheid + hint; haakjes zonder eenheid blijven staan).
- `form_lines.rs`: `lines_form` zet regelpanelen (kop, tekst, probleem, veld, keuze, vink, knop, plaatje) om in een `FormSpec`:
  een kop begint een sectie, Tekst na een rij is de hulp, Probleem de fout, een knoprol per rol (een tweede knop voor een bezette
  rol wordt een actie in het formulier, verdwijnt niet). `g5_form` en `schikken_form` gebruiken het; `dialog_role` kreeg
  Generator (Voeg in/Annuleer), startbevestiging, Zoeken (Sluit); `schikken_dialog_role` Sleuven, Nesten, Print and Cut, camera.
- `prefs_form.rs`: Voorkeuren met tabs Algemeen / Canvas / Verplaatsen, venstertitel "Instellingen", eenheid als suffix,
  `Standaardwaarden herstellen` als destructieve knop, de banner als melding (met slot). `hub_form.rs`: Machinehub met tabs
  Apparaat / Productie / Historie / Kalibratie; de ids zijn de tags van de Mac-controls.
- Mac `form.rs`: tabs (`NSTabView`, één grid per tab, kolombreedtes gedeeld), melding, plaatje, log (alleen-lezen scrollend),
  rode bijschriften, lange vinkjes en knoppen die breken/over de rij lopen, alleen-lezen tekst met gemeten hoogte, accessibility-label
  per veld, `relayout` na `update`, `install_form` (scrollt boven 640 pt). `g5.rs`: elk zwevend G5-paneel is een formulier (de in de
  inspector ingebedde sectie, de stapel, houdt de handgelegde kolom tot LF7). `schikken.rs`, `app_panels/windows.rs` (Voorkeuren,
  `prefControl:`), `hub.rs` zijn herschreven op de bouwer. Barcode en Hoeken afronden houden hun eigen sheet-layout maar hebben de
  eenheid als suffix en het bijschrift op de basislijn van het veld (`textform::Form`, `widgets::align_baseline`).
- `layout_invariants`: `tabbed_form_test`, `check_tabs` (elke tab van Voorkeuren en de hub), `check_units_are_suffixes` (barcode,
  Hoeken afronden), een wrapping-vinkje telt als afgekapt alleen als de hoogte tekortschiet. `BASELINE_PENDING` verloor `preferences`,
  `machine-hub`, `schikken`, `offset`, `structure`, `recognition`, `remnants`, `search`, `generator`, `round-corners`, `barcode`.

## Red/green

| Test | Red (echt gedraaid) | Green |
|---|---|---|
| `form::tests::{a_unit_in_brackets_leaves_the_caption_and_becomes_the_suffix, brackets_that_are_not_a_unit_stay_in_the_caption, consecutive_sections_with_one_title_make_one_tab_in_order, the_shape_sees_a_section_move_to_another_tab}` | `cargo test -p burnburn-host-models --locked --lib form::` met stubs: `14 passed; 1 failed` (`tabs()` gaf `[]`), daarna `split_caption` als stub: `14 passed; 1 failed` (`a_unit_in_brackets…`). De test voor haakjes zonder eenheid en de vorm-test waren niet apart rood (het stub-gedrag voldeed al): eerlijk gelogd | `ok. 15 passed` |
| `form_lines::tests::*` (7) | `lines_form` als lege stub: `0 passed; 7 failed` | `7 passed` |
| `g5_form::tests::{the_generator_start_confirmation_and_search_panels_have_dialog_buttons, the_offset_and_repair_buttons_stay_actions_in_their_own_section}` | oude `g5_form`: `3 passed; 2 failed` | `lib: 41 passed` |
| `schikken_form::tests::the_ending_flows_have_cancel_left_and_the_answer_right` | rollen leeg: `1 passed; 1 failed` | `2 passed` |
| `prefs_form::tests::*` (4) en `a_notice_that_comes_and_goes_does_not_change_the_shape_of_the_form` | stub: `0 passed; 4 failed`; slot uit: `4 passed; 1 failed` | `lib: 48 passed` |
| `hub_form::tests::*` (4) | stub: `0 passed; 4 failed` | `4 passed` |
| native `layout_invariants::tabbed_form_test` | bouwer zonder tabs: `1 layout violations: tabbed form: no tab view`; daarna rood op onbereikbare rij/rood bijschrift (testfout: tab niet geselecteerd, test aangepast met reden in de code) | `ok` |
| G5-panelen als formulier (`is_form_panel` = waar) | `7 layout violations`: lange vinkjes en actieknoppen afgekapt (`title needs 330 pt of width, has 200`); daarna overlap van een brede knop; | vinkjes breken, knoppen lopen over de rij: `layout_invariants ok` |
| Voorkeuren (tabs) | `app_panels::preferences_window_edits_apply_persist_and_refuse_bad_input` faalde op `the unit label follows` (suffix werd niet bijgewerkt) | unit-label in `update`; test opent eerst het tabblad Canvas/Algemeen (`select_prefs_tab`), zoekt `Automatisch bewaren` (eenheid en `0 = uit` zijn uit het label) en `contains("Geen geldig getal.")` (de melding heeft een statusglyph): groen |
| Machinehub tabs | `machine-hub (tab 1): cut off (wrapped text needs 32 pt of height, has 16)` en `…needs 17 pt, has 1` (rij te laag na update) | gemeten rijhoogte + `relayout`: groen |
| barcode / Hoeken afronden | `barcode…: baseline of the caption is +4.0 pt off`, `a caption holds its unit: 'Modulegrootte (mm)'`, `no unit suffix label after any field`; idem `round-corners` | suffixlabel en `align_baseline`: `layout_invariants ok` |

Volledige native `cargo test -p burnburn-macos` vóór Schikken/Voorkeuren/hub: geen enkele fout (G5-, G6-, G7-, G9-paneeltests groen). Daarna
`layout_invariants`, `app_panels`, `schikken_panel_native` groen. `burnburn-windows` `form_spec_lf6c` (achteraf geschreven, test bestaand
gedrag), `form_spec`, `menus` groen.

## Voor/na (offscreen)

`lf-lf6c/before/light-nl-06…`, `…09…`, `…14…` tegenover `after/`: Voorkeuren heeft nu tabs en velden op één lijn met hun caption, "s" als
suffix, "0 = uit" als hulp, de reset als knop rechtsonder; de hub heeft vier tabs in plaats van een pop-up; de G5-panelen hebben secties
met kop en één kolom captions. Opgemerkt: in de offscreen-opname is het label van het geselecteerde tabblad onzichtbaar (witte pil op
een inactief venster); de tab is er wel (de test leest de titels). Een screenshot is geen test.

## Open

- De stapel (in de inspector ingebed) en andere ingebedde G5-secties blijven handgelegd tot LF7 de inspector migreert.
- "Niet beschikbaar"-reden per sectie (`FormSection::unavailable`) gebruiken de G5-modellen nog niet: hun redenen zijn Tekst- of
  Probleem-regels. Dat vraagt een modelwijziging per paneel.
- Barcode en Hoeken afronden gebruiken de bouwer niet zelf (hun widgets worden door tests en de host direct benaderd); ze delen wel de
  basislijn- en suffixregel.
- De Windows-layout uit `FormSpec` (tabs, melding, log, plaatje) is LF13.
- Handmatig: de panelen naast de HIG-referentie, tabbladtitel in het geselecteerde tabblad, toetsenbordnavigatie door de tabs.
- `appkit/panels/form.rs` is ook in LF6a/LF6b aangeraakt: verwacht een merge.
