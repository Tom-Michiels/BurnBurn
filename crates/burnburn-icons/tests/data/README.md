# Icon-testdata

Herkomst-audit W3-robuust (2026-10-20). Invoer voor `tests/render.rs`; de verwachte pixels staan als analytische
posities en uitgeschreven kleurwaarden in de test, niet als golden-afbeelding.

| Bestand | Herkomst | Opmerking |
|---|---|---|
| `palette.toml` | Kleurrollen (bron-hex, licht, donker) uit het SolidCraft-look-werk (commit `6c483d9`) | De verwachte kleuren in de test zijn uit dit bestand **uitgeschreven** (hex-constanten), niet teruggelezen |
| `circle.svg`, `rect.svg` | Icon-masters (viewBox 64 × 64) uit dezelfde set | Geometrie (middelpunt, straal, randen) is uit de padgegevens analytisch bekend; de test controleert pixelposities daartegen |
| `undo-flat.svg` | Undo-icoon (`currentColor`-lijn) uit dezelfde set | De schacht ligt op y = 24,5 van x = 14 tot 29; alleen als renderinvoer, geen vaste uitvoer-golden |

Er zijn voor de renderer geen golden-PNG's: `crates/burnburn-render/tests/offscreen.rs` en `scene_projection.rs` toetsen per pixel
of met analytische verwachtingen (ADR 0003: golden-PNG-vergelijking is niet het enige orakel).
