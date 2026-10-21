# W4-UI-machine (golf 4, lane L1: laser- en beeldroute in het Mac-venster)

Alles met `CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/w4-ui-machine`, `cargo ... --locked`.

## Stand bij aanvang
De wachtrij-regels W1-L4a (Show Last Position, Save RD / Run machine file, File List, Diagnose exporteren,
"Aansluiting vereist") stonden dubbel in `docs/plan/integratie-wachtrij.md`: een oude regel "wacht op L1" en een
latere regel "klaar — W2-L1-laser". De code bevestigt de latere regels (`laser.rs`, `laser_service.rs`,
`laser_window.rs`, macOS `laser_panel.rs`, native `laser_window.rs`, `history_action`-armen voor `Frame` en
`Live(kind)`). Nieuw werk daarvoor: alleen het ontbrekende regressietest op unieke ids (punt 2) en het opruimen
van de verouderde regels.

## Cycli (rood → groen)
| Test | Rood (waargenomen) | Groen |
|---|---|---|
| `document tests/compound_commands.rs` (5: `importing_an_image_is_one_revision_and_one_undo_step_and_redo_restores_all`, `a_failing_step_leaves_no_trace_of_the_earlier_steps`, `add_operation_for_created_needs_something_created_by_the_earlier_steps`, `an_empty_or_nested_compound_is_refused`, `mask_image_assign_unassign_and_update_are_one_undo_step`) | stub `CommandPayload::Compound(_) => Ok(())`: `cargo test -p burnburn-document --test compound_commands` → 5 failed (bv. "the frame: left 0, right 1", "expected Err(Compound(Empty)), got Ok") | `execute_compound` (stappen in dezelfde transactie, eerste fout verwerpt alles): 5 passed |
| `presentation tests/image_flow.rs one_undo_step::{importing_an_image_is_one_undo_step_and_one_redo_restores_frame_and_operation, masking_an_image_is_one_undo_step_and_one_redo_restores_the_mask}` | `--test image_flow one_undo_step` → 2 failed (`undo` label was `Process(AddOperation)` / `Process(UpdateOperation)`, niet `Compound(..)`) | `commit_bitmap` en `mask_image` sturen één `CommandPayload::Compound`; `--test image_flow`: 24 passed (testoracle `Edit(CreateShape)` → `CreateRectangle` hersteld: mijn fout, geen productiecode) |
| `macos tests/l10n.rs all_message_keys_have_nl_and_en` | sleutels `UndoAction/RedoAction(Compound(..))` toegevoegd zonder teksten: 1 failed ("missing translations: … Compound(ImportImage) …") | NL/EN-arm voor `CompoundLabel`: 7 passed |
| `editor tests/bed_picture.rs` (2: `gray8_expands_to_opaque_rgba_in_the_same_pixel_order`, `gray8_of_the_wrong_length_or_an_empty_size_is_not_a_picture`) | stub `from_gray8 → None`: 1 failed (de lengte-/nul-test was vacuüm groen) | 2 passed |
| `render tests/image_preview.rs::an_image_preview_is_painted_above_the_design_and_below_the_selection_marks` | stub `Primitive::Bitmap` zonder scene-vertaling: 0 bitmaps (`left: 0, right: 1`) | 3 passed |
| `render tests/image_preview.rs::the_gpu_draws_the_preview_above_the_object_fill` (echte wgpu-pass, offscreen, tolerantie ±2) | pixel (41,41) was `[128,128,128,255]` (ontwerpvulling, bitmap niet getekend) | `Prepared`-lijst + runs tussen bitmaps in `CanvasRenderer::encode`: passed; `bed_image.rs` (camera onder het ontwerp) bleef groen |
| `render tests/image_preview.rs::an_invisible_or_unusable_preview_produces_no_work` | direct groen (nul-werk volgt uit dezelfde `picture_cells` als de camera; geen rode fase) | passed |
| `presentation tests/image_flow.rs canvas_preview::{the_preview_lies_on_the_image_bounds_and_follows_the_values, closing_or_cancelling_the_panel_removes_the_preview_from_the_canvas}` | 2 failed (geen `Overlay::ImagePreview` in de scene) | `controller::image::preview_overlay` + scene-uitbreiding: 26 passed (hele bestand) |
| `runtime history_store::tests::{every_action_kind_has_its_own_history_id, the_generation_and_the_subject_separate_ids_of_the_same_kind}` | gedrag bestond al (armen in `history_action`); mutatiecontrole: `LiveKind::Resume => 8` (botst met Pause) → `every_action_kind…` FAILED ("two action kinds share a history id") | teruggezet: 2 passed |
| native `tests/native/macos/image_sheets.rs` (uitbreiding) | niet apart rood gemaakt (uitbreiding van een bestaand scenario; de controller-test hierboven was de rode fase) | `cargo test -p burnburn-macos --test image_sheets` → ok: preview-bitmap staat in de canvas-scene terwijl het Adjust-venster open is en is weg na sluiten |

## Beslissingen
- `CommandPayload::Compound(CompoundCommand{label, steps})` in `burnburn-document/src/compound.rs`; stappen zijn
  `Run(CommandPayload)` of `AddOperationForCreated{draft}` (de operatie wijst naar de geometrie die eerdere stappen
  aanmaakten; de aanroeper kent die ids niet vooraf). Geen geneste compounds, geen lege. Eén revisie, één undo-stap;
  een falende stap laat niets achter.
- Beeld: `Overlay::ImagePreview` (editor, additief) → `Primitive::Bitmap` (render): in lijstvolgorde getekend, boven
  de objectvulling en onder selectiemarkeringen. Camera blijft `Primitive::Image` (onder alles).
- Picture-id = eerste 8 bytes van de inhoudshash van de gray8: gelijke pixels behouden hun GPU-textuur.

## Handmatig / onbevestigd
- Camerabeeld als overlay: bestond al (W3-UI-camera: `Overlay::BedImage` + `render/tests/bed_image.rs` pixeltest +
  native `camera_window.rs` met fake frames in de scene). De echte AVFoundation-bron blijft handmatig.
- Visuele controle van de Adjust-preview in het echte venster is niet gedaan (alleen scene + offscreen-GPU).
- Windows: `Primitive::Bitmap`/`Overlay::ImagePreview` zijn platformvrij; geen Windows-run gedaan.
