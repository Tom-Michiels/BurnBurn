# LF-E — integratie (coördinerende sessie, 24-10-2026)

**Lege bewerkingentabel overlapte de kolomkoppen** (gezien op de LF17-snapshot `light-nl-01-empty-document-naamloos.png`):
het symbool van de lege toestand stond over "Vermogen (%)". De overlay centreerde in het hele scrollvenster; de kopregel
zit in de bovenste inset van de clip view.

| Test | Rood | Groen |
|---|---|---|
| native `lf17_empty_progress::the_empty_layer_table_says_so_and_leaves_when_a_layer_exists` (nieuwe controle: het blok raakt de kopregel niet) | `BURNBURN_NATIVE_QUIET=1 cargo test -p burnburn-macos --test lf17_empty_progress --locked`: "empty block [432.0, 147.5, 227.5, 104.0] overlaps the column headers [212.5, 220.5, 669.0, 28.0]"; ook na alleen een verschuiving met de halve kophoogte nog rood (het gebied is te laag) | alle 6 scenario's ok; `layout_invariants` ok |

- Fix (`appkit/panels/empty_state.rs`): de overlay kent de kophoogte van de tabel, centreert onder de koppen en laat het
  symbool weg als het blok anders niet in de rijen past (zin en knop blijven).
- `EmptyProbe.block_in_window` voor de test; features `NSTableHeaderView`, `NSClipView`.
