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
