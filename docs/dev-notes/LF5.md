# LF5: FormSpec en de Mac-formulierbouwer (look-and-feel, golf LF-B)

Basis `cb2166b`. Alle commando's met `CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/lf5 CARGO_INCREMENTAL=0`
en voor native tests `BURNBURN_NATIVE_QUIET=1` (stille vensters, nooit `--native`). Screenshots alleen offscreen via
`gui_snapshots` naar `/private/tmp/claude-501/-Users-tom--claude-spares-spare-7SCiVl/b7379f7e-55e7-4285-a7c2-e3f3d2728135/scratchpad/lf-lf5/{before,after}`.

## Wat er is

- `crates/burnburn-host-models/src/form.rs`: platformvrije `FormSpec` (secties met kop, rijen met label + `Control` + eenheid
  als `Quantity` (suffix), hulptekst en fout per rij, `unavailable`-reden één keer per sectie, `DialogButtons {cancel, primary,
  destructive, help}` met `ButtonRole`, `Key::{Return, Escape}`, `order(Platform)`, `problems()`, `shape()`). `format::unit_label`
  is additief toegevoegd (de eenheidstekst zonder spatie).
- `crates/burnburn-host-models/src/panels/g5_form.rs`: `g5_form(title, lines) -> G5Form`: de G5-regels (die de headless tests
  blijven controleren) als `FormSpec`; `dialog_role` zegt welke knop primair (NotesSave) of annuleren (NotesClose) is. LF6c
  breidt die tabel per paneel uit.
- `crates/burnburn-macos/src/appkit/panels/form.rs`: `build_form` op `NSGridView` (drie kolommen: label rechts uitgelijnd, veld,
  eenheid als suffix; `rowAlignment = FirstBaseline`, spatiëring uit `style::Space`, kop over alle kolommen, hulp en fout onder het
  veld, de reden onder de kop, geheugen-rij 160 pt), knoppenrij rechts (primair rechts, Hulp links, ⏎ en ⎋ als `keyEquivalent`),
  `FormView::{update, fits, controls, buttons, texts, values, edits, restore, clear_targets}`. `update` werkt op zijn plaats bij
  (nooit over een veld waarin de gebruiker typt) en vraagt bij een andere vorm om opnieuw bouwen; wat getypt is blijft bij het opnieuw bouwen.
- Migratie als bewijs: het **Notities**-paneel (`panels/g5.rs`: `is_form_panel`, `G5Section::show_form`). `G5Control`-gedrag en
  alle bestaande G5-tests zijn ongewijzigd (de `G5Target` krijgt de controls van de bouwer, `g5Control:` blijft de actie).
- `layout_invariants` kreeg (h) de basislijncontrole label/veld (±1 pt, ook caption voor pop-up) met een zelftest, en een
  `form_builder_test`.

## Red/green

| Test | Red (echt gedraaid) | Green |
|---|---|---|
| `host-models form::tests::{a_unit_is_a_suffix_after_the_control_and_not_part_of_the_label, return_answers_the_primary_button_and_escape_cancel, the_reason_a_section_is_unavailable_is_shown_once_and_all_its_rows_are_off}` | `cargo test -p burnburn-host-models --locked --lib form::` met stubs voor `suffix`, `key`, `row_enabled`: `FAILED. 5 passed; 3 failed` (de andere vijf tests, `problems`/`order`/`shape`/dubbele id, zijn samen met hun implementatie geschreven en waren niet apart rood: eerlijk gelogd) | zelfde commando: `ok. 8 passed` |
| `host-models panels::g5_form::tests::*` (3: knoppen Sluit/Bewaar, controls behouden, fout-regel zet Opslaan uit) | `cargo test -p burnburn-host-models --locked --lib g5_form` met een lege `g5_form`: `FAILED. 0 passed; 3 failed` | `ok. 3 passed` |
| `layout_invariants` (h) basislijn, zelftest `baseline_self_test` (label 6 pt naast zijn veld moet gemeld worden, een uitgelijnd label niet) | `cargo test -p burnburn-macos --test layout_invariants --locked` met een lege `check_baselines`: `FAILED, 1 layout violations: baseline self-test: a caption on another baseline than its field was not reported` | na de implementatie: de zelftest slaagt; de echte vensters geven **209 basislijnovertredingen** (zie hieronder) |
| `layout_invariants` `form_builder_test` (invarianten + knopvolgorde [Hulp, Annuleer, Primair] van links naar rechts, ⏎ op primair en ⎋ op annuleren, eenheid na het veld op dezelfde basislijn, caption links van het veld, de reden één keer) | `build_form` als lege stub: `FAILED, 3 layout violations: buttons run [], expected help, cancel, primary / no unit label 'mm/s', field or caption 'Snelheid' in the form / the reason is shown 0 times, expected once` | eerste echte versie: `FAILED, 3`: koppen en reden lagen 2 pt buiten de grid (labels tekenen 2 pt buiten hun alignment rect); `leadingPadding = 2` op de eerste kolom; daarna `ok. 1 passed` |
| Notities migreren | met `"notes"` uit `BASELINE_PENDING` en het oude handgelegde paneel (gemeten): `notes (Notities), as opened: baseline of the caption is +26.0 pt off its field` | met de bouwer: `layout_invariants ok` zonder `notes` in de lijst; `g5_panels_native` (o.a. `the_notes_panel_follows_show_notes_and_saving_is_one_undo_step`, de native Notities-test) en de headless `g5_panels` blijven groen |

### Basislijnovertredingen op de handgelegde panelen (`BASELINE_PENDING`, LF6 maakt de lijst leeg)

`BURNBURN_BASELINE_ALL=1` toont ze. 209 meldingen over 26 vensters: document (6), sheets (44: export, bundle, import, image,
tracer, …), cut-settings (22), preferences (9), machine-hub (1), laser (4), material-library (4), art-library (1), schikken (1),
offset (2), structure (24), recognition (5), remnants (5), search (2), generator (7), cam-optimization (1), cam-device (13),
cam-array (4), cam-material-test (16), cam-interval-test (12), cam-focus-test (9), cam-feeder (5), round-corners (1),
barcode (8), export-svg (2). Typisch: label 2 pt (vaste hoogte 20 tegen veld 24) of 4 pt (hoogte 24, y gelijk) naast de
basislijn van het veld, en 26 pt bij een label van vier regels. Na LF5 staat `notes` niet meer in de lijst.

## Voor/na (offscreen)

`scratchpad/lf-lf5/before/light-nl-13-notes-notities.png` en `after/light-nl-13-notes-notities.png`. Voor: het geheugenveld
is onzichtbaar (geen rand), de hulp en drie knoppen volbreed, het caption van vier regels staat links met het veld op een andere
lijn. Na: het geheugenveld heeft een rand, de hulp staat eronder in caption-grootte, het veld staat op de eerste regel van het
caption, "Bewaar onderdeelnaam" staat als actie onder het veld, en de knoppenrij is [Sluit] [Bewaar notities] met primair rechts.
Een screenshot is geen test; de tests staan hierboven. Opgemerkt: het caption "Onderdeelnaam (zo noemt een order dit product; …)"
is een lange tekst met uitleg erin; die hoort als hulptekst onder het veld (tekstwerk, niet van dit pakket).

## Windows

`FormSpec` en `g5_form` compileren in `burnburn-windows` (`cargo clippy -p burnburn-windows --all-targets --target
x86_64-pc-windows-msvc --locked -- -D warnings` schoon); `crates/burnburn-windows/tests/form_spec.rs` (achteraf geschreven,
geen rode fase: het test bestaand gedrag) controleert de Windows-volgorde [primair, annuleren] tegenover de Mac-volgorde. De
Windows-layout uit `FormSpec` is LF13.

## Open

- LF6: de andere panelen naar de bouwer (a CAM + Snij-instellingen, b bestanden, c G5/G6/G7, Schikken, Voorkeuren, hub, laser) en
  `BASELINE_PENDING` leegmaken; `dialog_role` uitbreiden (Offset-toepassen, Zoeken, enz.).
- Het tab-volgordetest (LF15) en `nextKeyView`-keten van de bouwer zijn er nog niet (de grid geeft de standaardvolgorde).
- Handmatig: het paneel naast de HIG-referentie, het geheugenveld met het toetsenbord (Return in een geheugenveld voegt een regel in;
  de primaire knop reageert op ⏎ buiten het geheugenveld), donker thema.
