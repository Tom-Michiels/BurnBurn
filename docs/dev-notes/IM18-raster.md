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
