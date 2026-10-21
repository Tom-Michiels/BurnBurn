# W4-acceptatie (golf 4): het acceptatieplan als scenario-tests

Basis: `integratie/golf-3` 6dcdb60, daarna `git merge 23bcd3a` (golf 3 op main, drie verouderde barcodetests gerepareerd).
Toolchain 1.99.1, `--locked`, `CARGO_TARGET_DIR=target/w4-acceptatie`. Overzicht per ID: `docs/user-flows/acceptatie.md`.

## Hoe de TDD hier is toegepast (eerlijk)

Bijna alles hier test **bestaand** gedrag van functies die in golf 1–3 zijn gebouwd. Een nieuwe acceptatietest die meteen
groen is, bewijst niets over zijn eigen gevoeligheid; daarom staat per blok hieronder hoe de testen zijn gecontroleerd.
Er is **één echte rode→groene cyclus** (de bug in IM03); verder zijn rode fasen mijn eigen verkeerde verwachtingen (die de
test terecht afkeurde, daarna met onderzoek van het ontwerp gecorrigeerd) en **mutatieproeven** (productiecode tijdelijk
stuk gemaakt, test moest rood worden, teruggezet met `git checkout`). Niets hier is "groen aangenomen".

### Bug gevonden en opgelost (IM03)

| Test | Verwachte fout (red) | Red-commando en resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| `acc_image::im03_refresh_after_replace_and_undo_reads_the_original_file_again` | Vervang-beeld + Undo + Bron-bestand wijzigen + Vernieuw laadt het *vervangende* bestand | `cargo test -p burnburn-scenarios --locked --test acc_image im03` → `im03_refresh_after_replace_and_undo_reads_the_original_file_again ... FAILED` ("refresh must re-read the file this picture came from (plaat.png), not the replaced one"); ook `im03_rotate_replace_refresh_and_undo_…` rood | Fix in `burnburn-presentation/src/controller/image.rs`: `origins` is nu per `(OperationId, pixelhash)` (+ `origin_of`), zodat een undo die de oude pixels terugbrengt ook hun bestand terugbrengt. Zelfde commando: `test result: ok. 9 passed`; `cargo test -p burnburn-presentation --locked --test image_flow`: `ok. 22 passed` | – |

Kleinste fix buiten de lane; ook in het eindrapport. Beperking: `origins` is sessiestaat en wordt niet opgeslagen, dus na
heropenen van een project werkt Vernieuw niet (bestaand gedrag, niet gewijzigd).

### Eigen foute verwachtingen die rood werden (en wat het ontwerp bleek te zijn)

| Test | Rood omdat | Ontwerp zoals het is | Aanpassing |
|---|---|---|---|
| `ed02_property_…` | mijn invoer "… m" (meter) | eenheden: mm, cm, in, " | meter vervangen door `"` |
| `ge02_a_huge_coordinate_…` | 1e9 mm gaf oppervlakte 128 i.p.v. 25 | `Path::signed_area` telt kruisproducten van absolute coördinaten (verlies bij ≥1e8 mm); de boolean zelf is goed; boven 1e13 mm bestaat een 10 mm-vierkant niet meer in f64 | oppervlakte lokaal gemeten; exacte verwachting tot 1e12 mm |
| `ar03_parts_with_holes_…` | ik gaf `locked=false` mee | `nest_parts(…, locked)` | parameter gecorrigeerd |
| `im01_a_one_pixel_border_…` | rij 1 en 7 bij schaal 1,5 mengen de randrij | dot = 0,667 bronpixel; rij 1 raakt 1/3 van de randrij | verwachting alleen voor binnenrijen 2..=6 |
| `tr01_*` | eiland (1,0 mm²) had dezelfde oppervlakte als de dunne lijn; Silhouette negeert kleur (wit ondoorzichtig = inkt) | zoals gedocumenteerd in `AlphaPolicy` | eiland groter; alfa-test herschreven |
| `ro01_property_…` | gespiegelde roller klapt om de bedhoogte (900), niet om de omtrek | `RotaryConfig::to_controller_y(y, frame_height)` met een roller die het bed houdt | verwachting per soort |
| `cam04_offset_fill_rings_…`, `pre02_…` | ringen op halve stap (inset 1,3,5,7,9) → 200 mm; telling 9 i.p.v. 10 | zie `offset_fill_cuts_concentric_rings_at_half_step_insets` | verwachting 200; 10e geval toegevoegd |
| `acc_ruida_diff` fill Y/hoek | `lower` weigert "scan axis Y"/AngledScan | typed `Unsupported`; wachtrij-regel | test pint de weigering (`…refused_at_export_not_written_wrong`) |

### Mutatieproeven (gevoeligheid; één per blok waar de eerste run meteen groen was)

| Mutatie (tijdelijk, teruggezet) | Test die rood werd |
|---|---|
| `burnburn-machine/src/port.rs`: `UploadChunk` als idempotent | `tr_ack_lost_…`, `tr_duplicate_ack_…`, `tr_delayed_ack_…`, `tr_disconnect_in_the_middle_…` (4 rood) |
| `burnburn-ruida/src/udp.rs`: checksumcontrole uit | `tr_a_corrupted_datagram_never_validates`, `tr_a_truncated_datagram_…` |
| `burnburn-ruida/src/lower.rs`: `relative()` weigert +8191 | `rd_diff_02_relative14_boundaries_…` |
| `burnburn-geometry/src/align.rs`: `Left` gebruikt het midden | `ar01_each_align_command_…` |
| `burnburn-geometry/src/repair.rs`: join-grens `<` i.p.v. `<=` | eerst **niet** rood (mijn grenswaarde 0,1 was niet exact in f64: 10,1−10 < 0,1); test uitgebreid met exact representeerbaar 0,25 → daarna `ge05_join_threshold_is_inclusive_…` rood |

Niet voor elk blok is een mutatie gedraaid (o.a. TX, BC, TR, LIB, CA, REG, MAT, PL niet). Daar rust de gevoeligheid op
de onafhankelijke oracle en op expliciete tegenvoorbeelden binnen de test (bijv. kerning aan/uit, alfa-beleid, andere
configuratie → miss), niet op een gedraaide mutatie.

## Commando's en uitkomsten (echt gedraaid)

- `cargo test -p burnburn-scenarios --locked` per bestand, alle groen; `acc_text` heeft 1 `#[ignore]` (RTL-gat, rood bij
  `-- --ignored`, geverifieerd: "the first letter of an RTL word sits to the right of the last").
- `cargo fmt --all`; `cargo clippy --workspace --all-targets --locked -- -D warnings` → schoon.
- `BURNBURN_REQUIRE_PYTHON=1 cargo xtask verify` → exit 0; `layers: 25 crates, 2 triples, no violations`. Python 3.13.7 is
  aanwezig en de oracle-tests hebben echt gedraaid (geen vacuüm-skip).
- `Cargo.lock` is bijgewerkt (nieuwe dev-dependencies van `burnburn-scenarios`: alleen workspace-crates, `proptest`,
  `rxing`; allemaal al in het register als dev-dependency; geen nieuwe externe crate in de workspace).

## Bevindingen die geen bug maar een beperking zijn (zie acceptatie.md en wachtrij)

- RTL-tekst (bidi) ontbreekt; `.shx` niet ondersteund.
- RD-lowering weigert Y-as-scan, scanhoek ≠ 0° en Z-focusoffset (typed); geen foutieve uitvoer.
- Feeder (FE01) en desktop-UDP-automatisering (API01) bestaan niet; tool-lagen en pad-arrays ook niet.
- Een som-checksum ziet het afkappen van nul-bytes niet (protocol, niet BurnBurn).
- QR "Data"-modus weigert tekens > U+00FF met naam en index; Unicode vraagt de modus *Unicode* (ontwerp, getest).
- `Path::signed_area` verliest precisie ver van de oorsprong (≥1e8 mm; niet praktisch bereikbaar).
- Machine-vermogensschaal: alleen 20 % → 3277 is hardware-bewezen; de overige percentages zijn dezelfde schaling.

## Handmatig / onbevestigd

Zie onderaan `docs/user-flows/acceptatie.md`: echte camera, stopwatch, rotary-omtrek, controller-gedrag bij busy/disk full,
Ruida-bank, OS-dialogen/hotkeys/DPI, Windows-run. Niets daarvan is als geslaagd gemeld.
