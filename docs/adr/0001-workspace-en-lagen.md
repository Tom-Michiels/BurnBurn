# ADR 0001 — Workspace en crate-lagen

- Status: **Aanvaard (Design)**, 17 oktober 2026; **amendement 1** (fase B) en **amendement 2** (fase C), 18 oktober 2026 — zie onderaan
- Bron van waarheid voor de controle: [`docs/design/layers.toml`](../design/layers.toml)
- Afgedwongen door: `cargo xtask layers` (eigenaar Testing)

## Context

Het [implementatieplan](../../research/implementatieplan/README.md#cargo-workspace) noemt 19 crates en per crate
wat zij *niet* mag gebruiken. Dat is een verbodslijst; voor een afdwingbare controle is een expliciete
allow-list met laagnummers nodig, anders ontstaan stilzwijgend cycli of "zijwaartse" koppelingen
(bijvoorbeeld `cam → ruida` of `render → presentation`). `../solidcraft/xtask/src/layers.rs` gebruikt een
laagtabel met "alleen strikt lagere lagen"; dat principe nemen we over, aangevuld met een allow-list per
crate en extern verboden pakketgroepen per target.

## Besluit

### Lagentabel

| Laag | Crate | Soort | Mag (normaal) gebruiken | Eigenaar |
|---|---|---|---|---|
| 0 | `burnburn-types` | kern | — | Kern |
| 1 | `burnburn-geometry` | kern | types | Kern |
| 2 | `burnburn-document` | kern | types, geometry | Kern |
| 2 | `burnburn-image` | kern | types, geometry | Trace |
| 2 | `burnburn-job` *(besloten fase B, IM13)* | kern | types, geometry | CAM |
| 3 | `burnburn-sketch` | kern | types, geometry, document | Sketch |
| 3 | `burnburn-trace` | kern | types, geometry, image, document | Trace |
| 3 | `burnburn-cam` | kern | types, geometry, document, image, job | CAM |
| 3 | `burnburn-text` *(IM18)* | kern | types, geometry, document | Generators/tekst |
| 3 | `burnburn-ruida` | kern | types, geometry, job | Protocol (`lower.rs`: CAM) |
| 4 | `burnburn-io` | kern | types, geometry, document, image, job | Kern |
| 4 | `burnburn-editor` | kern | types, geometry, document, image, sketch, job | User-flow |
| 4 | `burnburn-machine` | kern | types, job, ruida | Protocol |
| 4 | `burnburn-generators` *(IM22, amendement 2)* | kern | types, geometry, document, text | Generators/tekst |
| 5 | `burnburn-presentation` | kern | alle lagen 0–4 (incl. generators) | User-flow |
| 5 | `burnburn-history` *(gepland IM17)* | kern | types, job, machine, io | Protocol |
| 5 | `burnburn-render` | GPU-adapter | types, geometry, image, editor, job (+ `wgpu`) | GUI |
| 6 | `burnburn-runtime` | IO-adapter | types, geometry, job, ruida, machine, io, document, history, presentation, text | Protocol |
| 6 | `burnburn-test-support` | testkit (alleen dev-dep) | lagen 0–5 behalve render | Testing |
| 7 | `burnburn-macos` | host | presentation, render, runtime, image (+ lagere; io tijdelijk) | GUI |
| 7 | `burnburn-windows` | host | presentation, render, runtime, image (+ lagere; io tijdelijk) | GUI |
| 7 | `burnburn-cli` | host | alles behalve render en GUI-hosts | Protocol |
| 8 | `burnburn-scenarios` *(amendement 2)* | testkit (geen lib-code; alles dev-dep) | — normaal; dev: alles behalve hosts | Testing |
| — | `xtask` | tool | vrijgesteld; niemand hangt ervan af | Testing |

Regels R1–R8 staan in de kop van `layers.toml`. Samengevat:

1. Normale/build-afhankelijkheid op een workspacecrate: moet in `allow` staan **en** een strikt lagere laag hebben.
2. Dev-afhankelijkheid: laag ≤ eigen laag, of `burnburn-test-support`. Hosts zijn nooit een afhankelijkheid.
3. Kerncrates verbieden **transitief** (per target `aarch64-apple-darwin` en `x86_64-pc-windows-msvc`) de groepen
   `apple_native`, `windows_native`, `gpu`, `ui_toolkit`, `async_runtime`, `network`, `foreign_kernel`, `c_toolchain`.
4. `render` mag `wgpu` gebruiken, en dus transitief `objc2-metal`/`windows`, maar niet zelf AppKit- of
   Windows-crates noemen; de host levert een `raw-window-handle`.
5. `unsafe` is alleen toegestaan in crates met `unsafe = "adapter"` (render, macos, windows), in benoemde modules.

### Ontwerpkeuzes die van de planformulering afwijken of die aanvullen

- **`burnburn-job` (nieuw).** Het immutable `JobPlan` is het gedeelde contract tussen compiler (`cam`), preview
  (`editor`/`render`), RD-encoder (`ruida`), machine en opslag. Als het in `cam` zou wonen, moeten `ruida`,
  `render` en `machine` de hele document-/compilerstapel importeren. Een kleine datacrate maakt "preview, RD en upload
  lezen dezelfde revisie" structureel. De crate wordt pas aangemaakt wanneer IM13 het eerste plan-contract test
  (plan: "maak een crate wanneer er een werkend contract en een eerste test voor is"). Tot dan werkt de
  Ruida-decoder (IM14) op zijn eigen `RdProgram`-typen.
- **Evaluators lezen document, niet omgekeerd.** `sketch`, `trace` en `cam` lezen een `DocumentSnapshot` en leveren
  een patch/resultaat; `presentation` zet dat om in een `DocumentCommand`. `document` importeert geen evaluator.
- **`SceneSnapshot` in `editor`.** De scene is document-zichtbaarheid + editor-overlays + planpreview; `render`
  vertaalt die naar een `RenderPacket`. Zo hoeft `render` niet van `presentation` af te hangen en blijft `wgpu`
  buiten `presentation`.
- **`CancellationToken` in `types`.** Zware algoritmen in geometry/trace/cam moeten annulering kunnen controleren
  zonder `presentation` te kennen.
- **`runtime` boven `presentation`.** `runtime` implementeert de poorten van `presentation` (taakuitvoering,
  wake-up), `io` (bestandssysteem) en `machine` (transport). De hosts koppelen ze samen.
- **`test-support` op laag 6.** Het implementeert fakes voor poorten uit lagen 0–5. Een crate die door
  `test-support` gebruikt wordt (bijv. `types`) mag het alleen in `tests/` (integratietests) gebruiken: in
  `#[cfg(test)]`-units zou het crate-type twee keer bestaan.

### Workspace-conventies

- Leden via `crates/*` en `xtask`; root-`Cargo.toml` alleen uitbreiden in `[workspace.dependencies]`, één regel per
  crate, alfabetisch, met exacte versie uit het [dependencyregister](../design/dependency-register.md).
- Crates gebruiken `dep = { workspace = true }`; versies staan alleen in de root.
- Platformdependencies in hostcrates onder `[target.'cfg(target_os = "macos")'.dependencies]` respectievelijk
  `cfg(windows)`; een host-crate compileert op het andere OS als lege `fn main` met een melding, zodat
  `cargo test --workspace` overal werkt.
- `[lints] workspace = true` voor alle crates; adaptercrates overschrijven alleen `unsafe_code` (ADR 0002/0003).
- Geen ongerichte `--all-features`; backend-features worden per CI-job gekozen.

## Gevolgen

- Testing implementeert in `xtask layers` een parser voor `layers.toml` (via `toml`/`serde` als
  xtask-dependency, xtask is vrijgesteld) en `cargo metadata --filter-platform` voor beide triples.
- Een nieuwe crate of nieuwe toegestane kant vereist eerst een wijziging in `layers.toml` + deze tabel (Design).
- Toestand eind 17-10: `types`, `geometry`, `document`, `editor`, `ruida`, `machine`, `render`, `macos`,
  `test-support` bestaan; `cargo xtask layers` (Testing) leest dit bestand en meldt *10 crates, 2 triples, no
  violations*. Afwijkingen en wijzigingsverzoeken: [design-review](../coordination/design-review.md).

## Amendement 1 — fase B (18 oktober 2026)

- **`burnburn-job` is een eigen crate** (orchestratorbesluit). Eigenaar CAM; `ruida/src/lower.rs` ook CAM.
- Eigenaren van de nieuwe kerncrates volgen de fase-B-rollen: Sketch (`sketch`, data-module `document::sketch`),
  Trace (`image`, `trace`), CAM (`job`, `cam`, data-module `document::process`).
- Nieuwe toegestane kanten: `ruida → geometry` (lower gebruikt `Bounds`/`Length`), `editor → image` en
  `render → image` (rasterobjecten in scene en packet), `presentation → ruida` (RD-export als taak) en
  `presentation → text`, `runtime → geometry, history`.
- Nieuwe geplande crates: `burnburn-text` (laag 3, IM18: harfrust+skrifa, evaluator zoals sketch) en
  `burnburn-history` (laag 5, IM17: append-only productiehistorie, [ADR 0008](0008-productiehistorie.md)). Ze bestaan
  pas als hun eerste test er is; de `layers.toml`-entry staat al klaar zodat niemand moet wachten.
- Evaluatorregel (contracts §11): `cam` hangt **niet** af van `sketch`/`trace`/`text`; die leveren patches in het
  document en CAM leest opgeloste geometrie. Daardoor blijft laag 3 vlak en is CAM testbaar zonder solver.
- Nesting (IM26), registratiewiskunde (IM27) en orders (IM31) beginnen als modules (`cam::nest`, `image::calib`,
  presentation/runtime) en worden pas afgesplitst als hun contract dat rechtvaardigt (plan).

## Amendement 2 — fase C (18 oktober 2026)

- **`burnburn-generators` is een eigen crate (laag 4)**, geen module in `sketch`. Generators (vingerlassen, tab/slot,
  doos/scharnier, stencilbruggen, padtekst, single-line tekst, datarijen/barcodes) hebben `text` nodig, dat op dezelfde
  laag als `sketch` staat, en mogen `cam` niet zien (kerf één keer, in `cam`: `material_thickness_changes_slot_but_does_not_apply_kerf_twice`).
  Data in `document::generator`, evaluator in de crate (zelfde patroon als sketch, DR-12). Eigenaar Generators/tekst,
  die ook `burnburn-text` (en `document::text`) overneemt.
- **Modifiers (IM21) krijgen géén crate**: data én evaluatie in `burnburn-document::structure::modifier` (Kern). Ze hebben
  alleen `geometry` nodig; de evaluator wordt net als de andere via `apply_with` doorgegeven, dus `document::session`
  roept hem niet zelf aan. Afsplitsen pas als hij iets boven laag 1 nodig heeft.
- **`burnburn-history` en `burnburn-runtime` blijven eigen crates** (geen modules): history is puur (journal +
  herstel via de `FileSystem`-poort) en moet zonder threads/sockets testbaar zijn; runtime is de enige plek met
  threads, `std::net` en de OS-klok. `presentation` kent history alleen via de poort `HistoryStore` met eigen
  leesmodellen (zelfde laag).
- **runtime aangescherpt:** `async_runtime`, `network` en `c_toolchain` nu ook verboden (direct en transitief; ADR 0005:
  `std::net::UdpSocket`, geen Tokio). `windows_native` blijft transitief toegestaan (bv. `windows-sys` onder `getrandom`).
- **CLI:** `async_runtime`/`network` direct verboden; sockets alleen via runtime.
- **Hosts → image** (IM27: de host-`CameraPort` levert `CameraFrame { image: RasterImage }`). `hosts → io` blijft
  tijdelijk tot runtime `StdFileSystem` re-exporteert (DR-21).
- **`burnburn-scenarios`** (Testing): test-only crate op laag 8, `kind = "testkit"`, lege lib, alle workspacecrates als
  dev-dependency (R3 laat alles ≤ 8 toe; R4 houdt hem uit elke graaf). Overgenomen zoals Testing hem aanmaakte.
  Hosts (en dus de CLI-binary) blijven onbereikbaar: CLI-scenario's (`cli_and_gui_compile_same_revision_…`) wonen in
  `burnburn-cli/tests` met `CARGO_BIN_EXE_*`.
- `cargo xtask layers` na deze wijziging: *19 crates, 2 triples, no violations*.

## Open

- Geen.
