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
