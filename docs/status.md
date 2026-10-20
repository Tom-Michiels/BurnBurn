# Uitvoeringsstatus BurnBurn (18-10-2026)

Verificatie: `cargo xtask verify` (fmt, layers, workspace-deps, clippy -D warnings, tests, doctests) **slaagt** op
macOS 27 arm64 met toolchain 1.99.1: 25 crates, ca. 84k regels Rust, 880+ tests groen. Windows, Linux-CI en hardware: **onbevestigd**.

Per werkpakket staat de TDD-log in `docs/dev-notes/`. "Gebouwd" = de geplande eerste test en de kernlevering zijn groen;
het betekent niet dat alle acceptatiecriteria van de stories gehaald zijn. De werkelijke storystand staat in
`docs/user-flows/story-coverage.md`: 33 stories deels, 30 alleen in het domein, 11 nog gepland, 0 volledig.

| Pakket | Stand | Belangrijkste open punt |
|---|---|---|
| IM01 workspace/harness | gebouwd | CI nooit gedraaid |
| IM02 eenheden/geometrie | gebouwd | Windows-determinisme (libm-golden) |
| IM03 commands/undo | gebouwd | selectie in undo-records |
| IM04 opslag/recovery | gebouwd | echte stroomuitval, Windows-replace |
| IM05 presentatie/input | gebouwd | — |
| IM06 AppKit-host | gebouwd | handmatig: Cmd-Q, Finder-dubbelklik |
| IM07 GPU-canvas | gebouwd | echt surface-/deviceverlies |
| IM08 Windows-proef | typecheck | nooit op Windows gedraaid |
| IM09 tekenen/repareren | gebouwd | fillet, curve-trim |
| IM10 import/export | gebouwd | SVG `<use>`/tekst, DXF-blocks |
| IM11 sketches | gebouwd | externe entiteiten (tangentie tussen objecten) |
| IM12 contourtracing | gebouwd | EXIF-oriëntatie |
| IM13 operations/compiler | gebouwd | — |
| IM14 Ruida-codec | gebouwd | grayscale-semantiek OPEN |
| IM15 preflight/preview | gebouwd | acceptatie door echte controller |
| IM16 transport/actor | gebouwd (fake) | geen geverifieerd wire-dialect; upload/start geweigerd |
| IM17 jobs/historie | gebouwd | jobgoedkeuring niet duurzaam |
| IM18 tekst/raster | gebouwd | bidi/RTL; geen grayscale |
| IM19 lokale tracecorrectie | gebouwd | — |
| IM20 native afwerking | gebouwd, hosts aangesloten | Windows zonder operations/camera (macOS tekent het camerabeeld sinds W3-UI-camera) |
| IM21 modifiers/componenten | gebouwd | — |
| IM22 generators | gebouwd | barcode-leesproef handmatig |
| IM23 centerline | gebouwd | — |
| IM24 traceerrecepten | gebouwd | — |
| IM25 laserprocessen | gebouwd | Unverified-features op hardware |
| IM26 nesting | gebouwd | spiegelen, onderdelen in gaten |
| IM27 camera | kalibratie, camerasectie en cameravenster (overlay, lens- en uitlijnwizard, beheer, meerdere camera's) gebouwd | AVFoundation-adapter nooit automatisch gestart (handmatig); kalibraties niet persistent (io-DTO, L6) |
| IM28 migratie | LBRN2 gebouwd | PDF wacht op beslissing (lopdf) |
| IM29 calibratie/beheer | software gebouwd | USB, hardware |
| IM30 CLI | gebouwd | plugins alleen manifestschema |
| IM31 orders/operator | gebouwd | — |
| IM32 kleurtracing | gebouwd | common-line snijden zelf niet gebouwd |

Integratiepoorten A–D draaien headless groen (`burnburn-presentation/tests/gate_*`, `burnburn-scenarios`).
Open eigenaarsbeslissingen: `docs/design/open-beslissingen.md` (B-1..B-12).
