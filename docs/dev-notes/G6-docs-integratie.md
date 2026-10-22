# Golf 6 — integratie (coördinerende sessie, 22-10-2026)

Windows-updatepoort bij afsluiten (X07), gebouwd tijdens de integratie omdat de docs-ronde hem als open meldde.

| Test | Rood | Groen |
|---|---|---|
| `burnburn-windows/tests/quit_guard.rs::{closing_the_last_window_during_a_run_asks_with_the_reason, only_an_explicit_yes_quits_and_everything_else_stays}` | `cargo test -p burnburn-windows --test quit_guard --locked` op de stub (`close_guard` → `None`, `on_guard_box` → `Stay`): 2 failed, 1 passed | zelfde commando na implementatie: 3 passed |
| `closing_one_of_several_windows_or_an_idle_machine_never_asks` | bewaakt het "nooit vragen"-pad; groen op de stub (geen rode fase) | 3 passed |

- `quit_guard.rs` wordt via `#[path]` gedeeld; de AppKit-specifieke `guard_choice` (NSAlert-knopcodes) verhuisde naar
  `burnburn-macos/src/adapter.rs` zodat het gedeelde bestand platformvrij is (gedrag ongewijzigd, `g5_machine_rest` groen).
- Win32: `WM_CLOSE` van het laatste frame vraagt met `MB_YESNO | MB_DEFBUTTON2` (Nee = blijven, standaard). Alleen
  typecheck + clippy `--target x86_64-pc-windows-msvc`; nooit op Windows gedraaid (handmatig).
- Docs-correctie: het Windows-machinehubvenster bestond al (G5-native, `host_panels/hub.rs`, `tests/g5_hub.rs`); de
  docs-ronde had het als ontbrekend gemeld.

# Golf 7 — integratie: geblokkeerde afgeleide geometrie in preflight (22-10-2026)

Bug (gemeld door G7-editor): `DocumentSnapshot::production_blocked` zegt "CAM must refuse to export it", maar `burnburn-cam`
las de vlag nergens. De laatste geldige omtrek van een mislukte generator, een tekst met ontbrekend lettertype of een
schets die niet meer oplost kon dus in een job/RD-export belanden.

| Test | Rood | Groen |
|---|---|---|
| `burnburn-cam/tests/blocked_geometry.rs::the_output_of_a_failed_generator_is_a_preflight_error_named_by_object_and_operation` | `cargo test -p burnburn-cam --test blocked_geometry --locked` met de nieuwe variant `Finding::BlockedGeometry` (Error) maar zonder controle: 1 failed (`report.findings` leeg), 1 passed | na de controle in de doellus van `preflight_with`: 2 passed |
| `blocked_geometry.rs::valid_derived_output_and_drawn_geometry_pass` | bewaakt dat geldige afgeleide en getekende geometrie niet geraakt worden; groen vanaf het begin | 2 passed |
| `burnburn-macos/tests/g7_blocked_geometry_text.rs::a_blocked_outline_says_why_it_is_not_produced` | `cargo test -p burnburn-macos --test g7_blocked_geometry_text --locked` met `FindingKind::BlockedGeometry` op de generieke tekst: 1 failed | NL/EN-tekst in `panels/cam.rs`: 1 passed |

- Een preflight-fout blokkeert export/upload al (`PlanViewModel.errors == 0` is de poort), dus geen extra route nodig.
- Het preflight-paneel toont de bevinding met het object als bron ("Toon bron"), op beide hosts via het gedeelde model.

# Golf 8 — stille native tests (22-10-2026, op verzoek van de eigenaar)

Native macOS-tests zetten vensters boven de apps van de gebruiker (`float_windows`) en maakten de computer tijdens
`cargo xtask verify` onbruikbaar. macOS kent geen virtueel scherm (geen Xvfb), dus: een stille modus.

- `HostOptions::quiet_windows` (tests: `BURNBURN_NATIVE_QUIET=1`): elk venster van de host is volledig transparant
  (`alphaValue = 0`), laat klikken door (`ignoresMouseEvents`), blijft op normaal niveau en de test-app wordt nooit actief.
  Het documentvenster wordt zo geopend; panelen, sheets en alerts maakt de event-pomp (`pump_until`) bij elke ronde stil.
- `cargo xtask verify` zet `BURNBURN_NATIVE_QUIET=1` op alle cargo-stappen; `cargo xtask verify --native` toont de
  vensters zoals vroeger (voor een echte zichtbaarheids-/focuscontrole vóór een release).

| Test | Rood | Groen |
|---|---|---|
| native `quiet_windows` (`tests/native/macos/quiet_windows.rs`) | `cargo test -p burnburn-macos --test quiet_windows --locked` met het veld zonder effect: FAILED "Naamloos: transparent (alpha 1)" | na implementatie: ok |
| `xtask tests::verify_runs_native_tests_quietly_unless_native_is_asked` | met `verify_env` → leeg: 1 failed | 3 passed (`cargo test -p xtask verify_`) |

- Volledige macOS-crate stil gedraaid (`BURNBURN_NATIVE_QUIET=1 cargo test -p burnburn-macos --locked --no-fail-fast`):
  523 geslaagd, 0 gefaald, geen overgeslagen schermcontroles (transparante vensters tellen voor de window server als
  zichtbaar; Metal presenteert gewoon).
- Beperking: of een venster er voor een mens goed uitziet en echt de focus krijgt, toetst alleen `--native` (en handmatig).

# Golf 9 — integratie (22-10-2026)

**Bug: palet-klik nam modifiers van een willekeurige gebeurtenis.** De native `layer_palette`-test zag één keer een gewone
klik als Shift-klik (gemeld door G9-cam-werk). `paletteClicked:` las `NSApplication.currentEvent().modifierFlags` zonder te
kijken wat voor gebeurtenis dat was; bij `performClick` of een klik via het toetsenbord is dat de laatst verwerkte
gebeurtenis (een eerdere gebaar, een systeemgebeurtenis met de Shift die de gebruiker toevallig vasthoudt).

| Test | Rood | Groen |
|---|---|---|
| `burnburn-macos/tests/g9_click_modifiers.rs::any_other_current_event_adds_no_modifiers` | `cargo test -p burnburn-macos --test g9_click_modifiers --locked` met `click_modifier_flags` als doorgeefluik: 1 failed, 1 passed | alleen muis-down/up (NSEventType 1–4, 25, 26) houdt zijn flags: 2 passed |
| `a_mouse_click_keeps_its_own_modifiers` | groen vanaf het begin (bewaakt dat een echte Shift-klik blijft werken) | 2 passed |
| native `layer_palette` (stil) | — | ok (`BURNBURN_NATIVE_QUIET=1 cargo test -p burnburn-macos --test layer_palette`) |

**Merge-breuk opgelost**: G9-cam-werk (`macos/tests/g9_cam_werk_panels.rs`, `presentation/tests/{g9_recognition,g9_part_name}.rs`)
gebruikte nog `#[path]`-fixtures die G9-trace-infra naar `burnburn-test-support::host` verhuisde; omgezet naar
`burnburn_test_support::host::{camera, rig}`.
