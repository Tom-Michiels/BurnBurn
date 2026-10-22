# ADR 0009 — Gedeelde hostmodellen: eigen crate `burnburn-host-models`

- Status: **Aanvaard (lane G8-refactor)**, golf 8
- Geldt voor: `burnburn-host-models` (nieuw, laag 7), `burnburn-macos`, `burnburn-windows` (nu laag 8)
- Vervangt: de interim-oplossing W4-L8a (`#[path]`-deling, contracts-phase-c §IM20)

## Context

`burnburn-windows` compileerde 19 AppKit-vrije bestanden van `burnburn-macos/src/` mee via `#[path]`
(`l10n*.rs`, `app_text`, `app_view`, `automation_host`, `hub_panel`, `panels/`, `panels_files`, `panels_image`,
`quit_guard`, `recent`, `toolbar`; ruim 7.900 regels). Zo tonen beide hosts dezelfde teksten, rijen en
enablement, maar de Windows-crate hangt aan de bronmap van een andere crate: elke hernoeming daar breekt Windows,
de code wordt in twee crates gecompileerd (dubbele clippy/doctests, `crate::`-paden die per host anders
oplossen) en het schendt de bedoelde richting "hosts onderaan, modellen gedeeld".

## Opties

| | A. Modules in `burnburn-presentation` | B. Nieuwe crate `burnburn-host-models` |
|---|---|---|
| Laag | 5 | 7 (boven runtime) |
| `automation_host` en `quit_guard` | Presentation mag `burnburn-runtime` niet kennen (runtime hangt al van presentation af: cyclus) | Mogen `burnburn_runtime::{automation, update_gate}` gebruiken |
| Omvang presentation | +7.900 regels hostspecifieke tekst/paneelvorm in de laag die ook CLI en runtime bouwen | Presentation blijft de host-neutrale controller en view models |
| Wijziging in andere crates | Veel: lagen, features, CLI-bouwtijd | Geen: alleen de twee hosts krijgen een dependency |
| Eigenaar | user-flow | gui |

## Besluit

**Optie B.** Een nieuwe crate `burnburn-host-models`, kind `core`, `unsafe = "forbid"`, laag 7. Reden:

1. `automation_host` (kanaal, pomp, luisterthread rond `burnburn_runtime::automation`) en `quit_guard`
   (`burnburn_runtime::update_gate`) gebruiken de runtime; runtime hangt van presentation af. In presentation
   zouden ze een cyclus geven; de crate moet dus boven runtime liggen.
2. De modellen zijn presentatie voor *hosts* (NL/EN-catalogus, paneelrijen, toolbartekst, recente lijst), geen
   host-neutrale documentlogica; ze horen niet in de laag die ook de CLI en de runtime bouwen.
3. Hosts verhuizen daardoor van laag 7 naar 8 (R2 vraagt een strikt lagere laag). `burnburn-cli` blijft laag 7
   en hangt er niet van af.

Grenzen van de crate (afgedwongen door `cargo xtask layers`): geen `apple_native`, `windows_native`,
`gpu`, `ui_toolkit`, `async_runtime`, `network` of `foreign_kernel` als directe dependency; `forbid_transitive`
zonder `windows_native`, net als runtime (windows-sys onder getrandom). Alleen platformvrije types: geen
AppKit-, Windows- of wgpu-type verschijnt in de gedeelde code.

## Gevolgen

- De verplaatsing is puur: gedrag ongewijzigd, tests ongewijzigd. De hosts herexporteren de modules
  (`pub use burnburn_host_models::{l10n, panels, ...}`), zodat `crate::l10n::..` en `burnburn_macos::l10n::..`
  blijven werken.
- Eén uitzondering: `app_view` gebruikte de hostspecifieke `app_menu::binding_glyphs` (macOS `⌘S`, Windows
  `Ctrl+S`). De gedeelde variant neemt de glyphfunctie als parameter (`shortcut_lines_with`); elke host heeft een
  dunne `app_view`-gevel met dezelfde publieke `shortcut_lines(view, filter, locale)` als voorheen.
- `cargo xtask path-attrs` (onderdeel van `verify`) verbiedt `#[path]` die de eigen crate verlaat, in `src/` en
  (sinds G9-trace-infra) ook in `tests/` en in de native tests onder `tests/native/`. Testfixtures die vroeger
  tussen crates werden gedeeld (`g5_fixtures`, `g6_fixtures`, `burnburn-presentation/tests/common`,
  `tests/native/macos/common/grbl_fake.rs`) staan nu in `burnburn-test-support`, module `host`, achter de feature
  `host` (laag 6; hangt van presentation en lager af, geen host- of platformtypes). Alleen crates die ze nodig
  hebben (presentation, macos, windows) vragen de feature als dev-dependency; de andere crates die de testkit
  gebruiken bouwen presentation niet mee (R3/R4 van `layers.toml` blijven gelden).
