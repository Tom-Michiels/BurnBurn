# Dependencyregister

Eigenaar: Design. Bijgewerkt: **18 oktober 2026** (fase C-start; vorige: fase B). Versies gecontroleerd met `cargo search`/`cargo info`; transitieve
graaf, buildscripts en `-sys`-crates met een proefresolutie (`cargo tree`, `cargo metadata --filter-platform`) voor
`aarch64-apple-darwin` en `x86_64-pc-windows-msvc` in een kladproject — **niet gebouwd of getest** tenzij
"in gebruik" staat. "In gebruik" = staat in de huidige `Cargo.lock`.

Regels:
- Een crate komt in root `[workspace.dependencies]` met de versie uit deze tabel; leden gebruiken `{ workspace = true }`.
- Toevoegen = eerst een regel hier (Design-review), dan pas in `Cargo.toml`. Upgrades zijn een eigen wijziging met tests.
- "Mag gebruiken" wordt voor workspace-grenzen afgedwongen door `cargo xtask layers`; voor externe crates door
  de groepen in [`layers.toml`](layers.toml) en door review van dit register.
- Licentiebeleid (voorstel voor `cargo deny`, nog niet ingericht): toegestaan MIT, Apache-2.0,
  Apache-2.0 WITH LLVM-exception, Zlib, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0, 0BSD, Unlicense (als OR-optie).
  Copyleft (GPL/LGPL/MPL) alleen na expliciete gebruikersbeslissing. De proefresolutie bevatte uitsluitend toegestane
  licenties; Apache-2.0-only: `flo_curves` (via vtracer), `insta`, `codespan-reporting` (via naga), `gl_generator`/
  `glutin_wgl_sys` (alleen bij wgpu `gles`, die we niet aanzetten).

## Productie-dependencies

| Crate | Versie | Licentie | Native / buildscript | Mag gebruiken | Status | ADR |
|---|---|---|---|---|---|---|
| `wgpu` | 30.0.1, `default-features = false`, `["std","wgsl"]` + `metal`/`dx12` per target | MIT OR Apache-2.0 | build.rs (`cfg_aliases`) in wgpu/-core/-hal/naga; linkt systeem-Metal (macOS) en `d3d12`/`dxgi` (Windows, OS-componenten); trekt `objc2-metal` 0.3.2 resp. `windows` 0.62.2 | render, macos, windows | in gebruik | 0003 |
| `pollster` | 0.4.0 | Apache-2.0 OR MIT | — | render (tests/opstart), hosts | in gebruik | 0003 |
| `raw-window-handle` | 0.6.2 | MIT OR Apache-2.0 OR Zlib | — | render, macos, windows | in gebruik (root, DR-01 afgehandeld) | 0002 |
| `bytemuck` | 1.25.2 | Zlib OR Apache-2.0 OR MIT | proc-macro (derive) | render | alleen transitief in de lock (niet direct) | 0003 |
| `objc2` | 0.6.5 | MIT | build.rs (platformdetectie); runtime = systeem-libobjc | macos (render alleen transitief via wgpu) | in gebruik | 0002 |
| `objc2-foundation` | 0.3.2, `default-features = false` | Zlib OR Apache-2.0 OR MIT | linkt Foundation.framework | macos | in gebruik | 0002 |
| `objc2-app-kit` | 0.3.2, `default-features = false` | Zlib OR Apache-2.0 OR MIT | linkt AppKit.framework | macos | in gebruik | 0002 |
| `block2` | 0.6.2 | MIT | — | macos | in gebruik (fase B) | 0002 |
| `dispatch2` | 0.3.1 | Zlib OR Apache-2.0 OR MIT | linkt libdispatch (OS) | macos | in gebruik (fase B) | 0002/0005 |
| `windows` | 0.62.2 | MIT OR Apache-2.0 | import-libs via `windows-link`; geen C-build | windows-host; **niet** direct in runtime (fase C: `layers.toml` verbiedt `windows_native` direct in runtime; `std::fs::rename` volstaat) | in gebruik (IM08, Win32-route) | 0006 |
| `windows-reactor` | =0.100.0 | MIT OR Apache-2.0 | eigen `windows-core` 0.100 (tweede versie naast 0.62); WinUI 3 vereist Windows App SDK-runtime (niet geverifieerd) | windows | proef IM08b | 0006 |
| `serde` (+`derive`) | 1.0.229 | MIT OR Apache-2.0 | build.rs, proc-macro | io (DTO's), history (records, IM17), runtime (plugin-JSON, IM30); andere kerncrates alleen na review | in gebruik | 0004 |
| `serde_json` | 1.0.154; in io **verplicht** met feature `float_roundtrip` | MIT OR Apache-2.0 | build.rs | io; history (IM17, ook `float_roundtrip`); cli (plan-JSON, IM30); test-support, scenarios, xtask, ruida (dev). Root nog `"1.0.145"` → `"1.0.154"` (DR-16, open) | in gebruik | 0004 |
| `sha2` | 0.11.1 | MIT OR Apache-2.0 | `cpufeatures` (runtime-detectie, geen C) | **alleen** types (`ContentHash::of`, DR-17 afgehandeld); io en test-support als **dev** (onafhankelijke golden-hashes). Image/trace/job hebben geen eigen `sha2` meer | in gebruik | 0004 |
| `zip` | 9.0.5, `default-features = false`, `["deflate-flate2-zlib-rs"]` | MIT | pure Rust (`flate2` + `zlib-rs`, `miniz_oxide`); **default-features verboden** (zstd-C, AES, bzip2) | io (normaal); io-tests ook als dev om beschadigde containers onafhankelijk van onze encoder te bouwen | in gebruik | 0004 |
| `crossbeam-channel` | 0.5.17 | MIT OR Apache-2.0 | build.rs in `crossbeam-utils` | runtime (driver-/workerthreads); `std::sync::mpsc` mag ook — kies bij de eerste runtime-test | gepland IM16 | 0005 |
| `kurbo` | 0.13.1 | Apache-2.0 OR MIT | geen; deps `arrayvec`, `smallvec`, `polycool` | — | **niet gebruikt** (IM09: eigen nearest/intersect/flatten volstaat; Kern 18-10). Terugval, alleen via ADR 0007-amendement | 0007 |
| `i_overlay` | 9.1.0 | MIT OR Apache-2.0 | geen; `i_float`, `i_shape`, `i_tree`, `i_key_sort`, `libm` | geometry (alleen `geometry::boolean` en `geometry::offset`) | in gebruik (IM09) | 0007 |
| `sketch-solver` | =0.1.0 (exact gepind in root) | MIT OR Apache-2.0 | geen; alleen `libm` | sketch (alleen `solver::adapter`) | in gebruik IM11 | 0007 |
| `libm` | 0.2.16 | MIT | pure Rust; al transitief via sketch-solver/i_overlay | geometry (`geometry::math`), sketch, image (tooncurve-LUT, `libm::pow`); trace/cam/text/generators liefst via `geometry::math` | in gebruik (DR-13) | 0007 |
| `harfrust` | 0.14.0 | MIT | geen; `read-fonts` 0.45.1 | text (alleen `text::shape`), `default-features = false, ["std"]` | in gebruik (root, IM18) | 0007 |
| `skrifa` | 0.48.0 | MIT OR Apache-2.0 | geen; `read-fonts` 0.45.1 | text (glyph-outlines); `read-fonts` moet één versie blijven met harfrust | in gebruik (root, `default-features = false, ["std"]`; `read-fonts` 0.45.1 éénmaal in de lock) | 0007 |
| `unicode-bidi` | 0.3.18, `default-features = false`, `["std","hardcoded-data"]` (G5-tekst-api: RTL/bidi-layout, TX01) | MIT OR Apache-2.0 | geen C, geen buildscript; `hardcoded-data` bevat de Unicode-bidi-tabellen in de crate zelf (geen `unicode-bidi-mirroring` nodig) | text (alleen `text::shape`: Unicode Bidirectional Algorithm, UAX #9) | in gebruik | 0007 |
| `image` | 0.25.10, `default-features = false`, `["bmp","png","jpeg","tiff"]` (W4-UI-gaten: BMP/TIFF-import, P03) | MIT OR Apache-2.0 | geen C; `png`, `zune-jpeg`, `moxcms`, `tiff` 0.11.3 (MIT) met `fax` 0.2.7 (MIT), `weezl` 0.1.12 (MIT OR Apache-2.0), `quick-error` 2.0.1 (MIT/Apache-2.0), al aanwezige `half`/`flate2`; BMP is ingebouwd; geen `rayon` | image (`image::decode`) **en io** (`io::import::raster`, `inspect_raster`, IM10); xtask (alleen dev-tool, LF4: PNG-codering van de app-iconen) | in gebruik | 0007 |
| `usvg` | 0.48.1, `default-features = false` | Apache-2.0 OR MIT | pure Rust; `roxmltree`, `svgtypes`, `kurbo` 0.13.1, `tiny-skia-path` (BSD-3-Clause), `simplecss`, `imagesize`, `data-url` | — | **niet gebruikt** (IM10: eigen begrensde SVG-subsetparser, motivatie `IM10.md`; fuzzdoel Testing) | 0007 |
| `tiny-skia` | 0.12.0, `default-features = false`, `["std","simd"]` | BSD-3-Clause | pure Rust (`tiny-skia-path`, `bytemuck`, `arrayref`, `arrayvec`, `strict-num`, `cfg-if`, `log`); geen buildscript met C; `simd` is `core::arch`-intrinsics, geen C | icons (`burnburn-icons`: UI-iconen rasteren; niet voor het canvas, dat blijft wgpu); xtask (alleen dev-tool, LF4: `assets/app-icon/*.svg` rasteren tot `.icns`/`.ico`/MSIX-logo's) | in gebruik (S-icons, SolidCraft-look); `toml` is hiervoor ook een normale dep van icons (palette/registry) | — |
| `lyon_tessellation` | 1.0.22 | MIT OR Apache-2.0 | pure Rust (`euclid`, `num-traits`) | render | in gebruik (IM07); de paraplu-crate `lyon` niet | 0003 |
| `crc32fast` | 1.5.2 | MIT OR Apache-2.0 | pure Rust (SIMD via `cfg`); al transitief in de lock (flate2) | history (recordframing) | goedgekeurd IM17 | 0008 |
| `roxmltree` | 0.21.1 | MIT OR Apache-2.0 | pure Rust (`memchr`) | io (LBRN2); niet meer transitief (usvg vervalt) → wordt een nieuwe directe dep; eigen subsetlezer mag ook (zelfde afweging als SVG) | kandidaat IM28 | 0007 |
| `lopdf` | 0.45.0, `default-features = false` | MIT | pure Rust, maar trekt `aes`/`cbc`/`ecb`/`md-5`/`rand`/`nom`/`flate2`/`encoding_rs` | io (PDF) | kandidaat IM28 (alternatief: eigen lezer) | 0007 |
| `nalgebra` | 0.35.0, `default-features = false, ["std"]` | **Apache-2.0** | pure Rust (`simba`, `matrixmultiply`) | image (`image::calib`) | kandidaat IM27 | 0007 |
| `fluent-bundle` | 0.16.0 | Apache-2.0 OR MIT | pure Rust | presentation (NL/EN) | kandidaat IM20 | — |
| `objc2-av-foundation` | 0.3.x (zelfde familie als `objc2-app-kit` 0.3.2; exacte versie bij IM27) | Zlib OR Apache-2.0 OR MIT | linkt AVFoundation.framework | macos (`CameraPort`) | kandidaat IM27 | 0002 |
| `nusb` | (te controleren bij IM29) | MIT OR Apache-2.0 | pure Rust USB (geen libusb/C) | runtime (USB-driver) | kandidaat IM29; `rusb`/`libusb1-sys` uitgesloten (C) | 0005 |
| QR/barcode (eigen) | — | — | eigen Code 128 (`barcode.rs`) en QR v1–10 (`qr.rs`, Reed-Solomon); geen crate nodig | generators | in gebruik (IM22) | 0007 |
| `anyd` | 0.2.0, `default-features = false`, features `alloc encode aztec code39 code93 code128 codabar datamatrix ean microqr msi pdf417 rmqr twoof5` | MIT | geen afhankelijkheden; geen build.rs; `no_std` + `alloc`; `forbid(unsafe_code)`; de default-features (`std`, `scan`, `decode`, beeldverwerking) staan uit | generators (alleen `symbology::engine`: 2 of 5 Industrial/IATA, Code 128 GS1, Code 93, Codabar, EAN/ISBN/UPC, Plessey, VIN (Code 39), Aztec, DataMatrix, PDF417 (+Compact, Micro), QR Micro, rMQR). Elk gebruikt symbool is met een onafhankelijke decoder of tweede encoder gecontroleerd (`docs/dev-notes/IM22.md`, W2-L5d) | in gebruik (W2-L5d) | 0007 |

Terugvalkandidaten (niet toevoegen zonder ADR-amendement): `rustybuzz` 0.20.1 (MIT), `ttf-parser` 0.25.1
(MIT OR Apache-2.0), `vello` 0.11.0 (Apache-2.0 OR MIT; trekt `skrifa` 0.44 → dubbele versie), `tokio` 1.53.3 (MIT).

## Test-, referentie- en tooldependencies

| Crate | Versie | Licentie | Mag gebruiken | Status |
|---|---|---|---|---|
| `proptest` | 1.12.0 | MIT OR Apache-2.0 | alle crates als **dev**-dependency | in gebruik |
| `tempfile` | 3.27.0 (root: `tempfile = "3.27"`) | MIT OR Apache-2.0 | **dev**-dependency van io (`std_fs_integration.rs`), later runtime/history-tests; nooit normaal (productie schrijft via `FileSystem::create_temp_in`) | in gebruik |
| `dxf` | 0.6.2 | MIT | alleen eventueel als dev-referentie voor de eigen DXF-lezer | kandidaat |
| `insta` | 1.49.0 | Apache-2.0 | dev-dependency (JobPlan, viewmodels); nooit auto-accept in CI | kandidaat |
| `vtracer` | 1.0.0-alpha.4 | MIT OR Apache-2.0 (transitief Apache-2.0 `flo_curves`, BSD-2 `roots`) | alleen trace-benchmark/vergelijking als dev-dependency, **niet** runtime | kandidaat (ADR 0007) |
| `toml` | 1.1.8 | MIT OR Apache-2.0 | xtask | in gebruik |
| `cargo-fuzz` / `libfuzzer-sys` | — | — | `fuzz/` (aparte toolchain, buiten de workspace-lock) | gepland (Testing) |
| `rxing` | 0.9.3, `default-features = false`, features `decoders encoding_rs aztec datamatrix oned pdf417 qrcode` | Apache-2.0 | **dev**-dependency van generators: onafhankelijke beelddecoder (ZXing/zxing-cpp-port, pure Rust) voor de barcodetests: EAN/UPC, Code 128/93, Codabar, Code 39, DataMatrix, Aztec, PDF417, QR, Micro QR, rMQR. Transitief alleen dev: `chrono` (+`iana-time-zone`, `core-foundation-sys`), `regex`, `csv`, `num*`, `encoding_rs` (build.rs, geen C), `once_cell`; nooit in de normale/build-closure, `forbid_transitive` blijft schoon | in gebruik (W2-L5d) |
| `qrcode-generator` | 6.1.2, `default-features = false`, features `micro-qr rmqr` | MIT | **dev**-dependency van generators: tweede, onafhankelijke Micro QR/rMQR-encoder voor module-voor-module vergelijking; geen afhankelijkheden, geen build.rs | in gebruik (W2-L5d) |
| `bwipp-rs` | 0.1.1 | MIT OR Apache-2.0 | alleen het losstaande referentieproject `research/barcode-reference` (eigen `[workspace]`, niet in de root-lock); pure Rust-port van BWIPP, levert de bevroren fixtures `tests/fixtures/barcode-bwipp.txt`. Past niet in de workspace: pint `image` < 0.25.10 en trekt `datamatrix`/`qrcode` mee | referentietool (W2-L5d) |
| `calib-targets-aruco` 0.15.5, `locus-core` 0.10.0, `apriltag-sys` 0.4.0 | — | MIT OR Apache-2.0, MIT OR Apache-2.0, BSD-2-Clause | geen dependencies: `research/barcode-reference` leest alleen hun databestanden als tekst (OpenCV-codetabellen, officiële AprilTag-C-bronnen; `apriltag-sys` wordt niet gebouwd, dus geen C-compilatie) en genereert `generators/src/symbology/fiducial_tables.rs` en `tests/fixtures/fiducial-reference.txt` | databron (W2-L5d) |

## Expliciet uitgesloten

| Crate | Reden |
|---|---|
| `slvs` | Wrapper om SolveSpace (C++); geen Rust-native solver (plan, AGENTS.md) |
| `potrace-sys`, `autotrace*`, `harfbuzz-sys`, `harfbuzz_rs`, `freetype-*`, `opencv*`, `clipper*-sys` | Niet-Rust algoritmekern achter een wrapper |
| `winit`, `tao`, `egui`, `iced`, `slint`, `tauri`, `wry`, `rfd`, … | Generieke UI-toolkits/webviews i.p.v. native AppKit/Windows-UI |
| `zip` met default-features | Trekt `zstd-sys` (C) en crypto mee |
| `rusqlite` / `libsqlite3-sys` / SQLite, `rusb` / `libusb1-sys` | C-databasekern + `cc` in de build; historie is een eigen append-only journal (ADR 0008) |
| `usvg` met default-features (`text`) | Trekt `fontdb` + systeemfontscan mee; tekst in SVG wordt als `Dropped` gemeld |
