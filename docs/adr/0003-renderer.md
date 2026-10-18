# ADR 0003 — Renderer: wgpu, tessellatie, offscreen-tests en f64 → f32

- Status: **Aanvaard (Design)**, 17 oktober 2026; **amendement 18-10-2026**: offscreen-formaat `Rgba8Unorm`
  (GUI-voorstel, orchestratorbesluit). Vello-benchmark: **later** (IM07-uitbreiding).
- Geldt voor: `burnburn-render`, `burnburn-editor::scene`, hosts (surface)

## Context

Het plan kiest een eigen vectorcanvas op `wgpu` met platformsurfaces; `lyon` en Vello zijn kandidaten. Domein-
geometrie is `f64` mm; GPU's werken in `f32`. Pixeltests op verschillende backends geven kleine verschillen.

## Besluit

### Versies

| Crate | Versie | Licentie | Opmerking |
|---|---|---|---|
| `wgpu` | 30.0.1 | MIT OR Apache-2.0 | `default-features = false`, `["std", "wgsl"]` + per target `metal` (macOS) / `dx12` (Windows). Geen `vulkan`/`gles`/`webgpu` op de producttargets (minder native transitieve code). De bestaande render-`Cargo.toml` volgt dit al. |
| `lyon` (`lyon_tessellation`) | 1.0.19 / 1.0.22 | MIT OR Apache-2.0 | Eerste proef: CPU-tessellatie, deterministisch en zonder GPU testbaar |
| `pollster` | 0.4.0 (workspace) | Apache-2.0 OR MIT | Alleen voor tests/opstart (`block_on` adapter/device); 1.0.1 bestaat, upgrade is een losse wijziging |
| `bytemuck` | 1.x | Zlib OR Apache-2.0 OR MIT | Vertexbuffers |
| `vello` | 0.11.0 (gebruikt wgpu 30) | Apache-2.0 OR MIT | **Niet** in de eerste proef; benchmarkkandidaat |

### lyon eerst, Vello als benchmark

- **lyon**: tessellatie is een pure functie (pad + tolerantie → driehoeken). Daardoor zijn fillregels, gaten, stroke-
  joins en tolerantie headless te testen zonder GPU, en is de GPU-stap een triviale pijplijn. Past bij TDD en bij
  CI-runners zonder GPU.
- **Vello** rendert via compute-shaders, heeft eigen scene-/fonttypen (`peniko`, `skrifa` 0.44 — botst met onze
  `skrifa` 0.48, zie ADR 0007) en is lastiger offscreen deterministisch te toetsen. Kandidaat zodra IM30
  (100k-segmentbench) laat zien dat CPU-tessellatie de bottleneck is.
- Tot de eerste lyon-integratie mag de spike-tessellator in `render/src/tess.rs` blijven bestaan; vervanging is
  een refactor onder dezelfde tests.

### f64-domein → viewportrelatieve f32

1. `editor` levert `SceneSnapshot` in wereld-mm (`f64`) + `Camera` + `Viewport` (contract §5).
2. `render::build_packet` transformeert in **`f64`**: `px = (p_mm − camera.center_mm) · points_per_mm ·
   backing_scale + viewport_center_px`. Pas het resultaat (al viewportrelatief, orde 10³–10⁴) wordt `f32`.
   Een coördinaat van 1 000 000 mm verliest zo geen sub-pixelprecisie (test: `far_from_origin_keeps_subpixel_precision`).
3. Schermtessellatie gebeurt **na** de projectie, met tolerantie in device-pixels (bijv. 0,25 px). Retessellatie
   bij zoom gebeurt per zoomstap-bucket; uitzoomen maakt alleen de *scherm*-mesh grover, nooit productiegeometrie.
4. Hit-testing (editor) gebruikt de analytische `f64`-geometrie met tolerantie in schermpunten, niet de mesh.
5. GPU-pixels zijn nooit invoer voor CAM/Ruida.

### Offscreen-tests

- **Laag 1 (altijd, headless):** `build_packet` en tessellatie: semantische asserties (aantal primitieven, bounds
  in px, volgorde, clip, kleurrol, Retina-schaal). Geen GPU nodig. Dit is het doorslaggevende bewijs.
- **Laag 2 (GPU aanwezig):** `render_offscreen(ctx, packet) -> Image` naar een **`Rgba8Unorm`**-texture
  (`burnburn_render::OFFSCREEN_FORMAT`), readback met correcte `bytes_per_row`-padding. Asserties op een klein aantal
  monsterpixels en een bounding box van gekleurde pixels, met gedocumenteerde tolerantie (±1 px geometrie,
  ±1/255 per kanaal voor ongemengde vlakken; blending binnen ±2/255).
- **Kleurruimte (amendement):** offscreen én schermsurface gebruiken een **niet-sRGB** 8-bit formaat
  (`Rgba8Unorm`/`Bgra8Unorm`); themakleuren zijn sRGB-gecodeerde waarden die ongewijzigd worden geschreven en
  blending gebeurt in weergaveruimte, zoals AppKit en Direct2D. Gevolg: dezelfde bytes in test en venster. Een
  `*Srgb`-surface mag alleen als de shader dan expliciet lineariseert, met eigen test; mengvormen niet door elkaar.
  Het CAM-/Ruida-pad leest nooit pixels, dus dit raakt alleen de weergave. Geen volledige
  golden-PNG-vergelijking als enige oracle.
- **Zonder adapter** (CI zonder GPU): test eindigt als *skipped* met een duidelijke melding en wordt in de
  dev-note als "onbevestigd" gerapporteerd — niet als geslaagd. Een software-/noop-backend bewijst geen Metal.
- Metal (macOS) en DX12 (Windows) draaien op eigen runners; semantische uitkomsten moeten gelijk zijn, pixels
  mogen binnen tolerantie verschillen.
- Device-loss en surface-resize (`SurfaceError::Lost/Outdated` → reconfigure) krijgen eigen tests in de host.

## Gevolgen

- `render` bezit `RenderPacket`, `Theme` (rol → kleur/dash/breedte) en de GPU-pijplijn; het bezit **niet** de
  scene-invoer of de camera (die liggen in `editor`, zie [design-review](../coordination/design-review.md)).
- `wgpu`-types verschijnen niet in `presentation`/`editor`.

## Open

- Vello-benchmark en eventuele overstap: na IM30-meting, met vergelijkingsfixtures.
