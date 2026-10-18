# IM18 Rasterproductie (rol CAM/job) + kerfcompensatie (C03)

Status: rasterdeel gereed op macOS arm64. **Geen grayscale-modulatie** (alleen achter een capability, zie IM25.md).
Geen hardware gebruikt.

Commando's: `export PATH="$HOME/.cargo/bin:$PATH"; CARGO_TARGET_DIR=target/cam cargo test -p <crate> --test <bestand>`.

## Opgeleverd
- `burnburn-image::raster` (nieuwe module, eigenaar CAM, in de crate van Trace; zie `cam.md`): `ToneCurve`
  (`out = clamp(contrast·(v−½)+½+brightness, 0, 1)^gamma`, tabel via `libm::pow`, DR-19), `resample_area` (exacte
  integer-boxgewichten vanaf de **originele** pixels), `tone_map`, `dither` (Threshold, Floyd–Steinberg, Jarvis,
  Stucki, Atkinson, Ordered 2/4/8; `i32`-foutdiffusie met truncatie, serpentine), `BitImage`, `RasterError`.
- `document::process::OperationMode::Image(ImageSettings)`: bron = `ContentHash` van de originele pixels, `dot_pitch`,
  `line_interval`, `ImageRender::{Dither(DitherMode), Grayscale}`, `ToneSettings`, richting/serpentine, overscan.
  Het beeld wordt over de bounds van de target-geometrie gelegd; fysiek schalen = de target schalen.
- `burnburn_cam::compile_with(.., &dyn RasterSource, ..)` (`compile` = zonder bronnen); `image::plan` resamplet bij
  **elke** compile naar het raster van de operation en maakt binaire `ScanRun`s (rij 0 = boven). Ontbrekende
  bron → `CompileError::MissingImageSource`. RD: dezelfde scan-opcodes als Fill (`AA`/`8B`/`88`), geen `C7/C2`.
- Kerf: `CompileError::KerfNotSupportedYet` vervangen door `geometry::offset(+k/2, Miter)` per target (buiten naar
  buiten, gaten naar binnen); open contour met kerf → `KerfOnOpenContour`. `SizeCheck` toont nominaal naast gepland.

## Cycli

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| `cam::compile::kerf_offsets_outer_contours_outward_and_holes_inward`, `…::kerf_on_an_open_contour_is_rejected_not_ignored` (vervangen `kerf_is_rejected_until_geometry_offset_exists`) | stub: kerf genegeerd | `--test compile kerf` → 2 FAILED: `hole first, offset into the hole: (80.0, 80.0, 90.0, 90.0)`; `unwrap_err() on an Ok value` | `kerf_path` via `offset` → compile 12 passed, preflight 6 passed | geen |
| `image::raster::{resampling_averages_source_pixel_areas_exactly, tone_tables_follow_the_documented_curve, threshold_burns_below_level, floyd_steinberg_matches_hand_computed_row, ordered_dither_accepts_only_bayer_sizes, ordered_dither_burn_fraction_tracks_the_gray_value (prop), error_diffusion_preserves_mean_tone (prop), resampling_stays_within_source_range (prop)}` | stubs: resample = 255, dither = alles uit, tabel = identiteit | `-p burnburn-image --test raster` → 9 FAILED (o.a. `left: [255.0, 255.0]`, FS `left: [false, false, false, false]`, `v=0 n=2 0 vs 0.998046875`, gamma `left: 128`) | implementatie → 8 passed, 1 FAILED (zie volgende rij) | geen |
| `image::raster::serpentine_runs_odd_rows_right_to_left` | — | mijn eerste versie (`serpentine_mirrors_the_second_row`, constante 128) faalde **groen** op `left != right` (`[true,false,true,false]` beide): **mijn aanname was fout**, symmetrische invoer geeft hetzelfde patroon. Vervangen door een 5×2-gradiënt met verwachting uit een onafhankelijke Python-transcriptie van FS (truncerende integer-shares) | 9 passed (+ 25 overige image-tests groen) | geen |
| `cam::raster::dither_runs_are_regenerated_from_source_after_physical_resize` (eerste test IM18) | stub: één keer ditheren op bronresolutie en de bits schalen (precies de verboden werkwijze) | `--test raster` → FAILED `left: 0 right: 2` (`[]`): na 0,1 → 0,4 mm bleef het witte ene punt wit | `resample_area` naar het operation-raster → 3 passed | compile herschreven tot units/modes (alle bestaande cam-/job-/document-tests groen) |
| `cam::raster::{image_operation_without_its_source_pixels_is_an_error, image_rows_run_top_to_bottom_in_image_order_and_zig_zag}` | — | **geen rode fase** (met de implementatie geschreven; slaagden ook tegen de stub) | 3 passed | geen |
| `ruida::lower::dithered_image_lowers_to_scan_records_without_modulation` | — | **geen rode fase** (bestaand scanpad) | 6 passed | geen |

Verwachtingen: FS-rij `128×4 → uit, aan, uit, aan` met de hand (fout −127 → 73 → −96 → 86); resample 4→3:
`25, 150, 241` met de hand; ordered-dither binnen `1/n²` van `1 − (v+½)/256` (analytisch).

## Open / onbevestigd
- Grijswaardemodus (`ImageRender::Grayscale`) blokkeert standaard (`GrayscaleModulation` = `Unverified`), lower weigert
  hem altijd (`Unsupported "grayscale modulation"`); semantiekproef `gray-ramp-grayscale.rd` blijft OPEN.
- Absolute travel (`88`) tussen runs binnen één scanrij en de "image"-uitvoer als Fill-blok: niet op hardware getest.
- Pixelschaalmodus voor vooraf geditherde beelden (T09): niet gebouwd (threshold-dither op 1:1-raster benadert het).
- Cache op inputs-hash (contract): niet gebouwd; elke compile resamplet. Alfa: altijd composiet over wit.
- Kerf: open contouren worden geweigerd; kerf + gedeelde grenzen (IM32) maakt de grenzen ongelijk → geen dedupe.
- io schema 2 kan Image/OffsetFill/gedraaide Fill/advanced niet opslaan: **save weigert** (`SaveError::Encode`), zie IM25.md.

## W1-L5a — image modes, beeldaanpassing en processed bitmap (lane L5)

Status: domein klaar op macOS arm64; **geen UI** (zie `docs/plan/integratie-wachtrij.md`), geen hardware gebruikt.
Commando's: `export PATH="$HOME/.cargo/bin:$PATH"; CARGO_TARGET_DIR=target/l5 cargo test -p <crate> --test <bestand>`.
Onafhankelijke referentie: `docs/dev-notes/IM18-image-modes-reference.py` (eigen transcriptie van elke kernel uit de
gepubliceerde definitie; de halftone-spotfunctie is numeriek geïntegreerd, niet de gesloten vorm uit de code).

### Contract (gedocumenteerde BurnBurn-varianten; de LightBurn-kernel is niet bewezen)
- **Pijplijn per frame** (`burnburn_image::engrave`, aangeroepen door `cam::image`): crop-venster van de bron →
  gray (kleurmethode, alpha over papier) → resample naar het fysieke scanraster (oppervlaktegemiddelde; *nearest* voor
  pass-through) → **unsharp → toon (gamma/helderheid/contrast) → negatief → modus** → mask → runs → dot width
  correction. Dithering ziet dus altijd de eindtoon op de fysieke schaal (T09 blijft gedekt door
  `cam::raster::dither_runs_are_regenerated_from_source_after_physical_resize`).
- Error-diffusion (FS, Jarvis, Stucki, Atkinson, serpentine) en Bayer 2/4/8: ongewijzigd uit IM18, nu met 2-D goldens.
- **Halftone**: ronde dots, cel in dots (= `cell_mm / dot_pitch`), hoek tegen de klok in; een pixel brandt als zijn
  middelpunt binnen de schijf ligt waarvan de oppervlaktefractie (binnen de cel) gelijk is aan de donkerte `1 − v/255`.
  **Newsprint** = hetzelfde scherm vast op 45°. Het scherm veronderstelt vierkante dots (`pitch == interval`).
- **Sketch**: Sobel op de 8-bit-grijswaarden, `m = min(255, ⌊√(gx²+gy²)/4⌋)`, brandt bij `m ≥ level`.
- **Grayscale**: `Engraved::Levels` (0 = vol vermogen); min..max-mapping gebeurt in `compile::emit` (bestaand).
  Negatief inverteert de niveaus. Capability `GrayscaleModulation` blijft `Unverified` (IM25), dus ongewijzigd geblokkeerd.
- **Negative**: na de toon, vóór de modus. **Pass-through**: geen resampling-filter, geen toon/unsharp/negatief,
  drempel 128; alleen met scanhoek 0 (anders `UnsupportedMode`).
- **Dot width correction**: elke run krimpt in totaal `dwc` (de helft per kant); runs korter dan `dwc` vervallen.
- **Scanhoek**: raster in het scan-frame (`fill::ScanFrame`), beeld blijft staan en wordt *nearest* gesampled op de
  geroteerde dotmiddens; cellen buiten het beeld blijven wit; een run kan ≤ ½ dot buiten het frame uitsteken.
  Vereist `Feature::ScanAngle` (nog `Unverified` → geen RD-uitvoer tot L4 dat bewijst). Grayscale/pass-through + hoek: geweigerd.
- **Crop** (bronpixels), **mask** (cel binnen even-odd van de doelgeometrie), **kleur→grijs** (Rec709/601/gemiddelde/
  lightness/R/G/B), **sharpen** (`amount`, `radius_mm` → dots).
- **Save processed bitmap**: `burnburn_cam::processed_bitmap` gebruikt dezelfde `frame_raster` als `plan`; test
  `the_processed_bitmap_is_exactly_the_scanned_pattern` bewijst dat PNG-pixels = gescande pixels (incl. mask).
- **Opslag in `ImageSettings`** (Copy, reist in `UiIntent::AddOperation`): vaste eenheden — dot width correction in µm (u16), hoeken in 0,01° (u16),
  sharpen-hoeveelheid 0,01 / -straal 0,01 mm, schermcel 0,01 mm; zo blijft `OperationDraft` 192 bytes. Boven ~200 bytes slaat
  `clippy::large_enum_variant` toe in `burnburn-windows::Reaction` (opgemerkt door `cargo xtask verify`; eerste versie met f64-velden gaf 208 bytes).
- **Bestand**: nieuwe opties en modi staan additief in schema 2 (`mode.options`, `Render::{Newsprint,Halftone,Sketch,
  PassThrough}`); onbenutte opties laten de bestandsvorm en de hash van oudere operaties ongemoeid.

### Cycli (TDD)

| Test | Rode fase (commando → waargenomen resultaat) | Groene fase |
|---|---|---|
| `image::modes::{floyd_steinberg,jarvis_judice_ninke,stucki,atkinson}_gradient_matches_independent_reference`, `ordered_bayer_4_matches_the_textbook_matrix` | **geen rode fase**: karakterisatie van bestaande kernels tegen de Python-referentie (slaagden direct; bevestigt de kernels, vangt regressies) | 5 passed |
| `image::modes::{halftone_*, sketch_*, unsharp_*, colour_to_gray_*, crop_*, negative_*, tone_is_applied_before_the_negative, sharpening_comes_before_*, pass_through_*, grayscale_*, processed_*}` (stubs: halftone/sketch = alles wit, unsharp = identiteit, engrave = leeg, conversie genegeerd) | `-p burnburn-image --test modes` → 23 FAILED, 7 passed (o.a. `halftone left: [".....", …]`, `unsharp left: 190.0 right: 10.0`-achtig, `crop_outside…: Ok(..)`, `png: left: [] right: [0,0,0,255…]`) | 30 passed na implementatie van `screen.rs`/`engrave.rs`/`resample_area_with` |
| `transparent_pixels_composite_over_paper_after_the_colour_conversion` | de eerste versie (zwart, alpha 128) **faalde niet** tegen de stub (alle kanalen 0). Versterkt met (200,0,0,α128); die versie is niet tegen de stub gedraaid (alleen groen waargenomen) | passed |
| `image::modes::nearest_resampling_replicates_and_skips_without_filtering` | stub = wit: `left: [255.0, 255.0] right: [20.0, 40.0]` | passed |
| `document::image_settings::*` (6) | `every_new_option_counts…` → `variant 0`; `angles…` → `left: 190.0 right: 10.0`; `validated…` → `Ok(ImageSettings{dot_width_correction_mm: -0.01})`; `each_new_option_and_mode_changes_the_canonical_form` → `negative must hash differently`; 2 al groen (defaults, IM18-layout) | 6 passed; bestaande `process.rs`-hashtest groen |
| `cam::image_modes::*` (18) | `--test image_modes` → 15 FAILED: `UnsupportedMode` voor halftone/newsprint/sketch/pass-through, runs onveranderd bij negatief/dwc/crop, `Scan{axis: X}` bij hoek 90, `processed_bitmap` stub-fout. `pass_through_with_a_scan_angle_is_refused` en `every_new_dither_mode_is_a_plain_dither_feature` slaagden al tegen de stub (ze documenteren het gedrag) | 18 passed |
| `cam::image_modes::the_processed_bitmap_equals_…` | mijn verwachting `"####..####"` was fout (gat = cellen 3..=6): test gecorrigeerd naar `"###....###"` na handmatige herberekening, implementatie ongewijzigd | passed |
| `cam::image_modes::an_oblique_scan_angle_…` | eerst `UnsupportedFeature{ScanAngle, Unverified}` (profielgate; test gebruikt nu een profiel met ScanAngle=Supported) en daarna runs 0,025 mm buiten het frame (toleranties: ½ dot) — testverwachting aangepast, geen codewijziging | passed |
| `io::image_options_roundtrip::*` (4) | `every_image_option_survives…` → **opties stil weggevallen** (`negative: true` ↔ `false`), modi → `Encode("dither mode has no file form")`, `damaged…` → `UnknownField options` | 4 passed |

Verwachtingen met de hand: halftone cel 4, grijs 191 → vier centrumpixels per cel (r = 0,177 → π r² = 0,098 ≤ 0,251;
volgende ring 0,491 > 0,251); unsharp piek `[0,0,90,0,0]`, r=1 → blur 30 → 90 + 60 = 150; Sobel stap 0|255 → gx = 1020
→ m = 255; mask-gat 4×4 van 10×10 dots → 84 dots = 8,4 mm.

### Open / onbevestigd
- De LightBurn-kernels (Newsprint, Halftone, Sketch, dot-width-correction-semantiek, Pass-Through) zijn **niet** uit de
  gesloten implementatie bewezen; dit zijn gedocumenteerde varianten. Vergelijking met LightBurn-uitvoer: OPEN.
- Hoek-scans en grayscale hebben geen RD-bewijs (zie IM25). Halftone op anisotrope rasters (`pitch ≠ interval`) is niet
  dot-vierkant. Mask maskeert het *resultaat* (foutdiffusie loopt door gemaskeerde cellen heen).
- Image-specifieke UI (paneel, live preview, replace/refresh/replace-to-fit) staat in de integratiewachtrij.
