# Fase B — opdrachten en orchestratorbesluiten (18-10-2026)

Stand na fase A: IM01–IM05 groen, IM06/IM07 spike groen (native venster + Metal), IM14 codec en IM16-statemachine groen.
`cargo xtask verify` slaagt op macOS arm64.

## Besluiten van de orchestrator (voorlopig; eigenaar kan herzien)
- Minimale macOS-versie: 13. Eigen crate `burnburn-job` voor JobPlan: ja (IM13). Tekst: harfrust + skrifa (ADR 0007).
  Tracer: eigen Rust-pipeline; vtracer alleen als vergelijkingsreferentie in tests/benchmarks, niet in runtime.
- Offscreen-formaat: `Rgba8Unorm` blijft (GUI-voorstel); Design past ADR 0003 aan.
- FS-adapters blijven in `burnburn-io`; `runtime` re-exporteert later. Assetsleutel blijft domeingescheiden SHA-256.
- Maatveld: commit bij Return én bij focusverlies (Tab), maar geen command wanneer de waarde niet wijzigt.
- Alle DR-items uit `design-review.md` die nog open zijn, worden door de eigenaar in deze fase afgehandeld.

## Toewijzing fase B
| Rol | Werkpakketten | Crates/paden |
|---|---|---|
| GUI | IM06 afronden, IM07, IM08 | burnburn-macos, burnburn-render, burnburn-windows (nieuw), tests/native |
| User-flow | IM05-uitbreiding voor IM09-interactie, scenario's poort B | burnburn-editor, burnburn-presentation |
| Kern-geometrie | IM09 (domein), IM10 | burnburn-geometry, burnburn-document (behalve `process`-module), burnburn-io |
| Sketch | IM11 | burnburn-sketch (nieuw) |
| Trace | IM12 | burnburn-image, burnburn-trace (nieuw) |
| CAM | IM13, IM15, encoderkoppeling IM14 | burnburn-job, burnburn-cam (nieuw), `burnburn-document/src/process/` (nieuwe module), `burnburn-ruida/src/lower.rs` |
| Testing | fuzz, corpora, reviews | burnburn-test-support, test-data, fuzz, xtask, .github |
| Design | contracten fase B/C, ADR-updates, reviews | docs/design, docs/adr, docs/coordination |

Gedeelde bestanden (root `Cargo.toml`, `burnburn-document/src/lib.rs`, `DocumentCommand`-enum): kleine, gerichte edits;
bij een "file modified"-fout opnieuw lezen en opnieuw toepassen, nooit andermans wijziging terugdraaien.
Workspace mag tijdelijk niet compileren door andermans crate: test dan met `-p <jouw crate>`; meld blijvende breuk in
`docs/coordination/<rol>.md`.
