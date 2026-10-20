# Fuzz-targets (cargo-fuzz)

Eigenaar: Testing. Doel en beleid: `research/implementatieplan/01-test-driven-design.md` § Propertytests, fuzzing.

Deze crate is **geen** workspace-lid (`exclude = ["fuzz"]` in de root): libFuzzer-instrumentatie vraagt een
nightly-toolchain, terwijl de workspace vastligt op stable 1.99.1 (`rust-toolchain.toml`). `fuzz/` heeft daarom een
eigen `[workspace]` en een eigen lockfile die bij de eerste run ontstaat. Nightly en `cargo-fuzz` zijn een
**bewuste lokale installatie** door wie fuzzt; de gewone build en `cargo xtask verify` hebben ze niet nodig.

| Target | Functie | Contract |
|---|---|---|
| `ruida_decode` | `burnburn_ruida::decode` | `Ok` of `DecodeError`, begrensd door `MAX_DECODE_INPUT` |
| `ruida_rd_file` | `burnburn_ruida::decode_rd_file` (magic 0x88 en eerste byte als magic) | `Ok` of `FileError` |
| `ruida_udp_frame` | `burnburn_ruida::udp::parse_frame` | payload = datagram zonder 2 checksumbytes |
| `io_container` | `burnburn_io::decode_container` | `Ok` of `LoadError`; limieten op entries/grootte |
| `io_dto_json` | `serde_json` → `dto::v1::{Document, Manifest}` | `Ok` of `serde_json::Error` |
| `io_parse_svg` | `burnburn_io::import::parse_svg` | `Ok` met eindige geometrie of `ParseError` |
| `io_parse_dxf` | `parse_dxf` + `to_draft(None)` en `to_draft(Some(inch))` | `Ok` of `ParseError`/`ImportError` |
| `io_parse_hpgl` | `parse_hpgl` | `Ok` met eindige geometrie of `ParseError` |
| `io_parse_pdf` | `parse_pdf` + `to_draft(All)` | `Ok` of `ParseError`; begrensd door decode-caps en operator-/segmentlimieten |
| `io_material_library` | `decode_material_library` (+ re-encode) | `Ok` of `LoadError` |
| `io_art_library` | `decode_art_library` + thumbnail/place per item | `Ok` of `LoadError` |
| `image_decode` | `burnburn_image::decode` + `io::import::inspect_raster` (standaardlimieten) | `Ok` of getypeerde fout, begrensd |

## Draaien

```sh
rustup toolchain install nightly          # eenmalig, bewust
cargo install cargo-fuzz                  # eenmalig, bewust
cd fuzz
mkdir -p corpus/io_container corpus/ruida_rd_file
cp ../test-data/projects/v1/*.burnburn corpus/io_container/
cp ../test-data/ruida/*.rd corpus/ruida_rd_file/
mkdir -p corpus/io_parse_svg corpus/io_parse_dxf corpus/image_decode
mkdir -p corpus/io_parse_pdf; cp ../test-data/import/*.pdf corpus/io_parse_pdf/; mkdir -p corpus/io_parse_hpgl; cp ../test-data/import/*.plt corpus/io_parse_hpgl/
cp ../test-data/import/*.svg corpus/io_parse_svg/; cp ../test-data/import/*.dxf corpus/io_parse_dxf/
cp ../test-data/trace/synthetic/*.png corpus/image_decode/
cargo +nightly fuzz run io_container -- -max_total_time=60 -max_len=1048576
cargo +nightly fuzz run ruida_rd_file -- -max_total_time=60
```

`corpus/` en `artifacts/` staan in `.gitignore`. Een gevonden crash (`artifacts/<target>/crash-*`):
1. minimaliseer met `cargo +nightly fuzz tmin <target> <crash-bestand>`;
2. leg de minimale input vast als fixture (met manifestregel) of als byteliteraal in een gewone regressietest in de
   crate van de eigenaar (AGENTS.md: elke bug een regressietest);
3. meld het in `docs/coordination/testing.md` aan de eigenaar.

## Stabiele tegenhanger

Zonder nightly draaien de propertytests in `crates/burnburn-test-support/tests/untrusted_bytes.rs` dezelfde
functies met willekeurige en gemuteerde fixturebytes binnen `cargo test --workspace` ("paniekt nooit" plus
contractinvarianten). Dat is geen vervanging van coverage-gestuurd fuzzen, wel een vaste ondergrens.
