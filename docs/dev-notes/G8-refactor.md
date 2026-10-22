# G8-refactor — `#[path]`-deling tussen de hosts opgeheven

Basis 8ee1ee9. Besluit en reden: [ADR 0009](../adr/0009-gedeelde-hostmodellen.md) (nieuwe crate
`burnburn-host-models`, laag 7; hosts nu laag 8).

## Wat is verplaatst (pure verplaatsing, `git mv`)

19 bestanden uit `crates/burnburn-macos/src/` (+ `l10n_schikken.rs`, dat `l10n.rs` met een eigen `#[path]` in
dezelfde map laadde: 20 bestanden, ~7.900 regels) naar `crates/burnburn-host-models/src/`:
`l10n`, `l10n_files`, `l10n_g5`, `l10n_g6`, `l10n_g6_werk`, `l10n_g7_editor`, `l10n_g7_trace`, `l10n_g7_werk`,
`l10n_machine`, `l10n_schikken`, `app_text`, `panels_image`, `panels_files`, `toolbar`, `recent`, `quit_guard`,
`automation_host`, `panels/` (21 bestanden), `hub_panel`, `app_view`. Beide hosts herexporteren de modules
(`pub use burnburn_host_models::{...}`), dus alle `crate::l10n::..`- en `burnburn_macos::..`-paden en alle
bestaande tests bleven ongewijzigd. Commits per groep: l10n, leaves, panels+hub_panel, app_view.

Enige inhoudelijke afwijking: `app_view` gebruikte de hostspecifieke `app_menu::binding_glyphs` (Mac `⌘S`,
Windows `Ctrl+S`). Gedeeld: `shortcut_lines_with(view, filter, glyphs, locale)`; beide hosts hebben een dunne
`app_view.rs` die `shortcut_lines(view, filter, locale)` met ongewijzigde signatuur levert. Bestaande tests
(`macos/tests/app_panel_models`, `windows/tests/w4b_app`) bevestigen dat de tekens per host gelijk bleven.

## Rood/groen: `cargo xtask path-attrs`

Eerst de unit tests van de check (`xtask/src/path_attrs.rs`) tegen een stub die niets rapporteert:

```
cargo test -p xtask --locked path_attrs
test result: FAILED. 1 passed; 4 failed   (a_path_into_another_crate_is_reported, a_path_that_climbs_to_the_workspace_root_is_reported,
                                            spacing_variants_and_comments_are_handled, the_path_is_resolved_from_the_directory_of_the_file)
```

Daarna geïmplementeerd: 5 passed. De check op de echte werkspace (alleen `src/`) was rood op de 19 regels:

```
cargo run -p xtask --locked -- path-attrs
  crates/burnburn-windows/src/lib.rs:12: #[path = "../../burnburn-macos/src/l10n.rs"] leaves crates/burnburn-windows ...
  ... (19 regels in crates/burnburn-windows/src/lib.rs)
xtask: 19 #[path] attribute(s) leave their own crate
```

Na de verplaatsing groen:

```
path-attrs: 27 crates, 674 source files, no #[path] leaves its own crate
```

`path-attrs` is een stap van `cargo xtask verify`. Scope is bewust `src/`: testbinaries delen nog fixtures via
`#[path]` tussen crates (20 regels, zie hieronder).

## Verificatie

- `cargo xtask layers`: 27 crates, 2 triples, no violations (met `burnburn-host-models` L7 en hosts L8).
- `cargo build --workspace --all-targets --locked`: groen na elke stap.
- `cargo test -p burnburn-host-models -p burnburn-windows -p burnburn-macos` (BURNBURN_NATIVE_QUIET=1) na de
  eerste stap: geen mislukte tests; windows + host-models ook na stap 2.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` en
  `cargo clippy -p burnburn-windows --target x86_64-pc-windows-msvc --all-targets --locked -- -D warnings`: schoon.
- Volledige `cargo xtask verify`: zie het eindrapport van de lane (uitkomst hieronder aangevuld).

## Niet gedaan / vervolg

Testfixtures delen nog code via `#[path]` over crates heen (20 regels in `tests/`: `macos/tests/g5_fixtures`,
`g6_fixtures`, `presentation/tests/common/{mod,camera}.rs`, `tests/native/macos/common/grbl_fake.rs`). Dat is
geen productiecode; verhuizen naar `burnburn-test-support` is een apart werkpakket (gemeld in
`docs/plan/integratie-wachtrij.md`).
