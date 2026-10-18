# Fase C/D — opdrachten (18-10-2026)

Stand: IM01–IM05, IM09–IM15 groen (macOS arm64); IM16 state machine groen; GUI (IM06–IM08) en User-flow
(IM09-interactie) lopen nog. Contracten voor deze fase: `docs/design/contracts-phase-c.md`. Historie: ADR 0008.

## Toewijzing
| Rol | Werkpakketten | Eigen paden |
|---|---|---|
| Protocol/machine | IM16 live transport, IM17, IM29 (software-deel), IM30 CLI | burnburn-runtime, burnburn-history, burnburn-machine, burnburn-cli |
| Kern | IM21, IM28 | geometry, document (behalve process/sketch), io |
| Generators/tekst | IM18 tekstdeel, IM22 | burnburn-text, burnburn-generators (nieuw; of module in sketch — Design beslist via layers.toml) |
| Trace | IM19, IM23, IM24, IM32 (kleurtracing) | burnburn-image, burnburn-trace |
| CAM | IM18 rasterdeel, IM25, IM26, IM32 common-line | burnburn-cam, burnburn-job, document/process, ruida/lower |
| Testing | cross-crate scenario's, benchmarks, fixtures, review | test-support, test-data, fuzz, xtask, tests/ |
| Design | contracten, review, ADR's | docs/design, docs/adr |
| GUI (na fase B) | IM20, IM27 native, Windows-uitbreiding | macos, windows, render |
| User-flow (na fase B) | IM31 operator/orders, flows C/D | editor, presentation |

## Harde grenzen
- **Nooit een echte laser of netwerkapparaat contacteren.** Live UDP-tests alleen tegen een fake controller op
  127.0.0.1 binnen het testproces. Hardwarestatus blijft "onbevestigd".
- Geen globale installaties (geen nightly, geen cargo install) zonder toestemming van de eigenaar.
- Geen C/C++-algoritmebibliotheken; nieuwe dependencies eerst in het register (Design) of in je coordination-notitie.
- Clippy op je eigen crates moet schoon zijn zonder `-A`-uitzonderingen; als andermans lopende werk clippy breekt, noteer dat.
