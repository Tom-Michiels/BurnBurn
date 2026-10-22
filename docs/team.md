# BurnBurn ontwikkelteam (rollen en afspraken)

Iedere rol heeft een eigen eigenaarsgebied. Werk buiten je gebied gaat via een notitie in
`docs/coordination/<rol>.md` (vraag, voorstel of overdracht) aan de eigenaar, niet via stille edits.

| Rol | Verantwoordelijk voor | Eigen paden |
|---|---|---|
| **Design** (architect) | Architectuur, crate-grenzen, publieke contracten, ADR's, dependency-/licentieregister, review van API's | `docs/design/`, `docs/adr/` |
| **Testing** | Testinfrastructuur, fakes (clock/fs/transport), fixtures en manifesten, property-tests, fuzz-targets, laag-/lintchecks in xtask, CI, review van TDD-logs | `crates/burnburn-test-support/`, `test-data/`, `fuzz/`, `xtask/`, `.github/` |
| **Implementatie** (kern) | Domeincrates: types, geometry, document, io, sketch, image, trace, cam | `crates/burnburn-{types,geometry,document,io,sketch,image,trace,cam}/` |
| **Implementatie** (protocol/machine) | Ruida-codec, machine-actor, runtime-adapters, CLI | `crates/burnburn-{ruida,machine,runtime,cli}/` |
| **Implementatie** (sketch) | Parameters, constraints, solver | `crates/burnburn-sketch/` |
| **Implementatie** (beeld/trace) | Bronbeelden, contour- en later centerlinetracing | `crates/burnburn-{image,trace}/` |
| **Implementatie** (CAM/job) | Proceslaag, JobPlan, compiler, preflight, RD-lowering | `crates/burnburn-{job,cam}/`, `crates/burnburn-document/src/process/`, `crates/burnburn-ruida/src/lower.rs` |
| **GUI** | AppKit-host, gedeeld wgpu-canvas, Windows-host, bundling | `crates/burnburn-{macos,windows,render}/`, `tests/native/` |
| **User-flow** | Presentatie/editor-interactie, HeadlessEditorHarness, gebruikersscenario's per user story en integratiepoort | `crates/burnburn-{presentation,editor}/`, `tests/scenarios/`, `docs/user-flows/` |

## Gedeelde regels
- Volg `AGENTS.md` en `research/implementatieplan/`. TDD: test eerst, rode fase waarnemen, dan groen, dan refactor.
- Log iedere cyclus in `docs/dev-notes/IMxx.md` (formaat: `docs/dev-notes/README.md`). Alleen echt uitgevoerde commando's.
- Rust: `export PATH="$HOME/.cargo/bin:$PATH"`; toolchain staat vast in `rust-toolchain.toml` (1.99.1).
- `cargo xtask verify` draait de native macOS-tests stil (transparante vensters, geen focus; `BURNBURN_NATIVE_QUIET=1`);
  `cargo xtask verify --native` toont ze zoals vroeger.
- Werk met `cargo test -p <jouw-crate>` en eigen `CARGO_TARGET_DIR=target/<rol>` om lockwachten te vermijden.
- Root `Cargo.toml` alleen uitbreiden in `[workspace.dependencies]` met één regel per crate, alfabetisch;
  workspace-leden komen automatisch via `crates/*`.
- Productiecode: geen `unwrap/expect/panic`; fouten zijn getypeerde `Result`s. `unsafe` alleen in native/GPU-hostcrates,
  klein, met gedocumenteerde invarianten.
- Kerncrates hangen nooit af van AppKit, Windows, wgpu of netwerk-/bestandssysteem-IO (behalve `io`/`runtime` via poorten).
