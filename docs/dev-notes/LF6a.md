# LF6a: CAM-dialogen en Snij-instellingen in de formulierbouwer (look-and-feel, golf LF-B)

Basis `0b517ed`. Alle commando's met `CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/lf6a CARGO_INCREMENTAL=0`; native tests met
`BURNBURN_NATIVE_QUIET=1` (stille vensters, nooit `--native`). Screenshots alleen offscreen via `gui_snapshots` naar
`/private/tmp/claude-501/-Users-tom--claude-spares-spare-7SCiVl/b7379f7e-55e7-4285-a7c2-e3f3d2728135/scratchpad/lf-lf6a/{before,after}`
(`before` zijn de opnames uit de LF5-run, dus van vlak voor de LF8-symbolen: alleen de pijlen naast de Sorteer-op-niveaus zijn sindsdien anders; `cam_dialog.rs` en `cut_settings.rs` veranderden verder niet).

## Wat er is

- `host-models/src/form.rs` (additief): `FormRow::{hidden, tooltip, tools, disabled_options}` met `FormTool`, `FormSection::reason`
  (waarom sommige rijen grijs zijn, één keer onder de kop, zet de rijen niet uit), `FormButton::tooltip`. Een verborgen rij blijft in de
  `shape()`, zodat een dialoog niet opnieuw gebouwd hoeft te worden als een modus, schakelaar of keuze rijen laat komen en gaan; een lege
  help, fout, reden of een lege alleen-lezentekst toont niets. `Quantity::Milliseconds` erbij (Puls).
- `host-models/src/panels/cam_form.rs`: `cam_form(vm, locale) -> CamForm` (platformvrij, headless getest) maakt van elke
  `DialogViewModel` een `FormSpec`: velden per groep in secties met kop (Volgorde, Optimalisaties, Vakken van de test, Basisinstelling,
  Opmaak, Bed, Snelheid en vermogen, Rotary, Machine, GRBL, Raster, Cirkel, Langs een pad, ...), `split_caption` haalt eenheid en
  opmerking uit de bestaande captions (`Overcut (mm, 0 = uit)` wordt label `Overcut`, eenheid `mm`, hulp `0 = uit`; de l10n-teksten zelf
  zijn niet veranderd, de Windows-host en de bestaande tests gebruiken ze nog), de reden van grijze velden één keer in de eerste sectie
  waarop hij slaat, de pijlen van de Sorteer-op-niveaus als `FormTool`, Sluiten als annuleer-knop (Escape), Pas toe / Reeks maken / Maak test
  als primaire knop (Return), de overige knoppen (Toon bron, Voeg toe, Verwijder, Eerder, Later, Opslaan als RD, Terug naar vlakbed) als
  actie-rijen, notities / mogelijkheden / resultaat als alleen-lezen secties.
- `host-models/src/panels/cut_form.rs`: `cut_form(sheet, top, live, kind, locale) -> CutForm`: modus, de vijf getallen en de twee
  schakelaars onder de laagnaam, daaronder een rij voor elk optieveld van elke modus (verborgen voor de andere modi); een notitie staat
  één keer onder de eerste rij van een reeks met dezelfde notitie (tooltip bij de rest); de fout van een geweigerde OK is de fout van een
  eigen rij. De vorm hangt niet van de modus af.
- `macos/.../panels/form.rs`: `build_form_with(.., FormLayout::{Natural, Capped(h), Fixed(h)})` (rijen scrollen boven de knoppenrij),
  verborgen rijen (`NSGridRow.setHidden`), kleine knoppen naast een rij (`NSStackView` in de eenheidskolom; Cargo-feature `NSStackView` en
  `NSUserInterfaceLayout` van `objc2-app-kit`, geen nieuwe dependency), uitgegrijsde keuze-items met reden als tooltip, tooltip per rij en
  knop, sectiereden, een sectie zonder iets te tonen verliest haar kop, tekst zonder caption loopt over de volle breedte,
  toegankelijkheidslabel op elk veld, `FormView::{control, help_label, error_label, heading_label, reason_label, tool, load,
  scroll_to_top}`; `update` herberekent de hoogte.
- `cam_dialog.rs`: `CamPanelUi` houdt een `FormView` in het `NSPanel` (breedte 560, hoogte hoogstens 640 met scroll), werkt hem op zijn plaats
  bij en bouwt opnieuw alleen als de dialoog andere rijen krijgt (getypte tekst blijft). De intents en acties zijn ongewijzigd
  (`camField:`, `camAction:`, `camMove:`).
- `cut_settings.rs`: het blad bouwt zijn formulier één keer (rij voor elk veld van elke modus, `Fixed(680)`), zodat de widgets die de
  window-controller bij OK leest (`cut_mode`, `cut_fields`, `cut_toggles`, `cut_options`) dezelfde blijven; `window.rs`, `classes.rs`
  en `app.rs` zijn niet aangeraakt.

## Red/green (echt gedraaid)

| Test | Red | Green |
|---|---|---|
| `host-models form::tests::{a_hidden_row_says_nothing_but_keeps_the_shape_of_the_form, a_section_reason_is_said_once_and_leaves_the_rows_to_say_if_they_are_enabled, tools_and_a_reason_belong_to_the_shape_and_tool_ids_may_not_repeat}` | velden als stub (nog geen gedrag): `cargo test -p burnburn-host-models --locked --lib form::` gaf `11 passed; 3 failed` | zelfde commando na de implementatie: alles groen (`31 passed` voor de hele lib) |
| `host-models panels::cam_form::tests::*` (6 tests: `a_caption_loses_its_unit_and_keeps_its_remark_as_a_hint`, `every_dialog_is_a_valid_form_in_both_languages_and_every_row_has_a_target`, `close_cancels_and_apply_or_generate_is_primary_and_the_rest_are_buttons_inside_the_form`, `a_reason_for_greyed_fields_is_said_once_and_no_field_repeats_it`, `the_order_levels_have_arrows_that_follow_what_can_move`, `typing_ticking_and_an_error_do_not_change_the_shape_of_a_dialog`) | `cam_form` als stub: `cargo test -p burnburn-host-models --locked --lib cam_form` gaf `0 passed; 6 failed` | eerste implementatie `4 passed; 2 failed` (de reden kwam in elke sectie; de fout verscheen pas na Pas toe, testfout): reden nu één keer per dialoog, test stuurt Apply mee; daarna groen |
| `cam_form::tests::{a_caption_..(hint "In volgorde van uitvoeren"), numbers_are_number_fields_and_words_are_text_fields_and_buttons_have_their_own_section}` | `5 passed; 2 failed` (kleine letter in de hint, getal zonder eenheid was geen getalveld) | `38 passed` (de hele lib) |
| `cam_form::tests::a_hint_that_two_fields_in_a_row_share_is_said_once_under_the_first` | `Nl: said twice: ["Zoals jij het opgeeft"]`; dezelfde fout meldde `layout_invariants` als `cam-device ... the same text 2 times` | `44 passed` (de hele lib) |
| `host-models panels::cut_form::tests::*` (5: `the_shape_is_the_same_for_every_mode_and_for_a_sheet_that_is_not_open`, `the_rows_of_the_chosen_mode_are_shown_and_the_rows_of_the_other_modes_hidden`, `units_are_suffixes_and_remarks_are_help`, `a_run_of_rows_with_the_same_note_says_it_once_under_the_first_and_the_others_keep_the_tooltip`, `ok_is_primary_cancel_cancels_and_a_blocked_mode_is_greyed_with_its_reason`) | `cut_form` als stub: `0 passed; 5 failed` | `5 passed; 0 failed` |
| `layout_invariants` `form_builder_extras_test` (verborgen rij, kleine knoppen na het veld op zijn regel met label, uitgegrijsd keuze-item met reden, tooltip, lege regels tonen niets, lege sectie zonder kop, sectiereden één regel, `update` laat getypte tekst staan en laat de vorm groeien, `load` vervangt hem, `Capped` scrolt en blijft binnen de hoogte, `Fixed` is precies zo hoog) | builder met stubs: `cargo test -p burnburn-macos --test layout_invariants --locked`: `14 layout violations` (o.a. `no choice control`, `the small buttons of row 3 are missing`, `a capped form has no scroll view`, `a fixed form is 334 high, expected 400`) | na de bouwer: eerst `2 layout violations` (de test mat de knoppen in de coördinaten van hun stack, testfout: nu in die van het formulier), daarna `ok` |
| `layout_invariants` met `cut-settings`, `cam-optimization`, `cam-device`, `cam-array`, `cam-material-test`, `cam-interval-test`, `cam-focus-test`, `cam-feeder` uit `BASELINE_PENDING` | de rode fase voor de oude, met de hand gelegde vensters is niet opnieuw gedraaid: LF5 mat ze (`BURNBURN_BASELINE_ALL=1`): cut-settings 22, cam-device 13, cam-material-test 16, cam-interval-test 12, cam-focus-test 9, cam-optimization 1, cam-array 4, cam-feeder 5 basislijnovertredingen. Met de nieuwe vensters en de lijst leeg voor deze acht: `1 layout violations: cam-device ... the same text 2 times` (zie hierboven) | na de hulp-dedupe: `layout_invariants ... ok` (104 overgebleven overtredingen zitten in de panelen van LF6b/c, LF7 en de rest, in `BASELINE_PENDING`) |

Bestaande tests, alleen gedraaid (geen rode fase nodig, gedrag is gelijk): `cam_dialogs` (device, optimization, tests, priority),
`g5_cam_dialogs` (array, order, preflight, sublayers), `g5_cam_material`, `cut_settings_options`, `layer_palette` (alle `ok`). Daarbij
drie kleine verwachtingswijzigingen: `CamPanelParts::reasons` is nu een `String` (de reden is een regel in een sectie, geen eigen
label; `cam_dialogs.rs` leest `p.reasons.clone()`), en `CutSettingsSheet.title` is de kop van de eerste sectie. De uitkomst
`SKIPPED on-screen checks (window occluded: screen locked or display asleep; not a pass)` hoort bij de native tests: de pixelchecks van
die tests zijn in deze run overgeslagen (scherm vergrendeld), niet geslaagd.

Windows: `cargo clippy -p burnburn-windows --all-targets --target x86_64-pc-windows-msvc --locked -- -D warnings` schoon;
`crates/burnburn-windows/tests/form_spec.rs::the_cam_and_cut_forms_put_the_primary_button_first_on_windows` (achteraf geschreven, geen rode
fase) controleert de volgorde [primair, annuleer] voor Windows. De Windows-layout uit `FormSpec` is LF13.

## Voor/na (offscreen)

Voor: `lf-lf6a/before/*`, na: `lf-lf6a/after/*` (zelfde bestandsnamen).
- `light-nl-24-cam-device-apparaatinstellingen.png`: voor een rij captions links met de eenheid in de caption ("Bedbreedte (mm)") en velden
  zonder uitlijning, geen koppen, de knoppen volbreed; na secties met kop (Machine, Bed, Snelheid en vermogen, Rotary), captions rechts
  uitgelijnd op de basislijn van het veld, eenheid als suffix, "Zoals jij het opgeeft" als hulp onder het veld (één keer), Sluiten en
  Pas toe rechtsonder.
- `light-nl-21-cam-optimization-...png`: de pijlen staan naast hun niveau als kleine knoppen (omhoog uitgegrijsd op het eerste, omlaag op
  het laatste), schakelaars in de besturingskolom, "Tolerantie" met `mm` erachter en "Hoogstens 5" eronder.
- `light-nl-28-cam-focus-test-...png`: de reden ("Ontbrekende capability: Z-stap ...") staat één keer onder de kop van de eerste sectie,
  de velden zijn grijs zonder eigen tekst, Maak test is grijs; de lege secties Resultaat verliezen hun kop.
- `light-nl-26-cam-material-test-...png`, `...-23-cam-sublayers-...png`, `...-25-cam-array-...png`, `...-29-cam-feeder-...png`,
  `...-22-cam-order-...png`, `...-34-cam-preflight-...png`: dezelfde opbouw.
- `light-nl-04-cut-settings-snij-instellingen.png`: de laagnaam als kop, modus en getallen met eenheid als suffix, Tabbreedte enz. met `mm`,
  "0 = overslaan" en de ontbrekende capability onder Tabvermogen, de opties scrollen boven Annuleer / OK (OK rechts).
Een screenshot is geen test; de tests staan hierboven.

## Bewuste afwijkingen en opmerkingen

- De caption-teksten in `l10n` zijn niet aangepast (Windows gebruikt ze nog); `split_caption` scheidt eenheid en opmerking bij het maken
  van het formulier. Windows krijgt de nette vorm in LF13 via dezelfde `cam_form` / `cut_form`.
- De knoppen die geen dialoogknop zijn (Voeg toe, Verwijder, Eerder, Later, Toon bron) staan als actie-rijen onder de kop "Acties"
  (onder elkaar); een horizontale knoppenstrook per sectie is een verfijning voor later.
- Het gedeelde `BASELINE_PENDING` is alleen op mijn acht regels gewijzigd; `dialog_role` is niet aangeraakt (de CAM-dialogen hebben hun
  eigen knoprollen in `cam_form`, niet via G5).
- `CamTarget` (host-models) is de naam van het doel van een form-id; het objc-doelobject van het paneel heet nu `CamActions`
  (`#[name = "BurnBurnCamTarget"]` ongewijzigd).

## Open

- Handmatig: de dialogen naast de HIG-referentie, donker thema, toetsenbord (Tab-volgorde, Return in een veld, Escape), VoiceOver voor de
  pijlen en de uitgegrijsde keuze-items; het scrollen van Apparaatinstellingen en Snij-instellingen met een muis. De pixelchecks van de
  native tests zijn in deze run overgeslagen (scherm vergrendeld).
- Tab-volgorde (`nextKeyView`, LF15) volgt de standaardvolgorde van de grid.
- `cargo xtask verify` (echt gedraaid na de laatste codewijziging, `BURNBURN_NATIVE_QUIET=1`, eigen target-dir): exit 0, geen `FAILED` in het log.
