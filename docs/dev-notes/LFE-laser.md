# LFE-laser: het laservenster op de formulierbouwer

Basis `1464dca`. Commando's met `CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/lfelaser CARGO_INCREMENTAL=0`; native tests met
`BURNBURN_NATIVE_QUIET=1` (nooit `--native`). Screenshots offscreen via `gui_snapshots` naar
`/private/tmp/claude-501/-Users-tom--claude-spares-spare-7SCiVl/b7379f7e-55e7-4285-a7c2-e3f3d2728135/scratchpad/lf-lfelaser/{before,after}/light-nl-10-laser-laservenster.png`.

## Wat er is

- `host-models/src/panels/laser_form.rs`: `laser_form(&LaserPanel, selected_file, locale) -> FormSpec`, `laser_id` (rij-ids), `LaserFormAction` en
  `laser_form_action(FormId)` (welk control welke actie is). Zeven secties: Machine (X/Y als alleen-lezen rijen met `mm`, status, Positie
  uitlezen, Pauze/Hervat/Software-stop), Positie en jog (jog-kruis, stap, stapgrootte, Ga naar X/Y, vinkjes), Oorsprong, Start vanaf en
  job-oorsprong (keuze, 3x3-pad, vinkjes, plaatsing), Frame, Bestanden op de controller (naam, bewaren, keuzelijst, knoppenrij) en Diagnose
  (nieuw: knop naar de bestaande exportdialoog). Intents, `simple_controls`, `LaserPanel` en de Windows-host blijven zoals ze waren.
- `form.rs` (host-models): `Control::Pad { columns, cells, hold }` met `PadCell`/`PadGlyph` (jog-kruis, 3x3), `FormRow.items: Vec<ItemState>`
  (aan/uit en reden per knop van een `Actions`-rij; leeg = gedrag zoals voorheen). Additief.
- Mac `panels/pad.rs`: `HoldButton` (mouse-down = press, na de trackinglus release, via een handler) en `build_pad`; `form.rs` kreeg alleen
  `Kind::Pad`-armen en per-knop-toestand voor `Actions` met `items`. Symbolen (pijlen, stip aan/uit) zet `laser.rs` erop.
- Windows `forms/spec_layout.rs`, `spec_panels.rs`: pad als raster knoppen (glyph als tekst) en `items` voor `Actions`.
- `laser.rs` (Mac) is van ~1100 naar ~600 regels: panel met pijltjestoetsen, target met één actie `laserForm:` (tag = form-id), hold-handlers,
  bouwen/updaten van het formulier, hoogte begrensd op `visibleFrame` via `laser_window_height` (pure functies ongewijzigd, nu met de echte
  inhoudshoogte). Een ander formaat (positie bekend geworden, andere lengte van de bestandenlijst) bouwt opnieuw met behoud van getypte tekst
  en scrollpositie, nooit terwijl een jogknop vastgehouden wordt.
- Bewuste afwijkingen: bestandentabel is een keuzelijst + knoppen (Info/Uitvoeren/Verwijderen, verwijderen vraagt nog steeds); een gemelde
  reden die twee secties delen staat één keer in het venster (regel f van `layout_invariants`); zonder machine zegt alleen de statusregel het
  en staan de velden uit (stapgrootte en framesnelheid waren voorheen aan).

## Red/green (echt gedraaid)

| Test | Rood | Groen |
|---|---|---|
| `host-models/tests/lfe_laser_form.rs` (15 tests: secties, eenheden, redenen, knoprollen, ids, pads, bestanden, vorm) | stub `laser_form` = leeg: `1 passed; 14 failed` (`cargo test -p burnburn-host-models --locked --test lfe_laser_form`) | `15 passed` |
| `without_a_machine_the_status_says_so_once…` (aangepast: één zin voor het hele venster) | `14 passed; 1 failed` | `15 passed` |
| `a_reason_that_several_sections_share_is_said_once_in_the_window` | `said once: …` 5 keer i.p.v. 1 (gevonden door de native test, daarna headless vastgelegd) | `16 passed` |
| `laser_form::tests::the_simple_tags_are_the_ones_of_simple_controls` | niet apart rood (pint bestaande tag-volgorde vast; eerlijk gelogd) | ok |
| native `laser_window` (uitgebreid: koppen, redenen één keer, `mm`-suffixen, geen glyph op knoppen, jog-kruis, hold press/release) | `the reason "Niet geverifieerd: Verplaatsen…" is said 2 times` | `ok` na dedupe |
| native `layout_invariants` (nieuw `check_laser_form`: koppen, eenheden, geen glyph, hoogte ≤ scherm) | `8 layout violations: cut off (title needs 39 pt of width, has 34)` (stapknoppen) | `ok` na pad-breedte 40 pt |

Wat een native test niet kan: de muisklik in AppKits trackinglus. De test roept dezelfde handler aan (`LaserParts::hold`); echt vasthouden met de
muis blijft handmatig. De pijltjestoetsen-test is ongewijzigd groen.

## Voor/na

`lf-lfelaser/before/…` (met de hand gelegd, afgekapt, lege velden zonder label, pijl-glyphs) tegenover `after/…`: koppen per sectie, captions met
`mm`-suffix, jog als kruis met symbolen, 3x3 als raster, reden eenmaal. De offscreen-opname toont de hele inhoud (geen scrollbeperking).

## Open

- `grbl_window.rs` (GRBL-laservenster, 665 regels met eigen console/macro's) is niet omgezet: past niet eenvoudig (console, macro's, andere
  intents).
- Positie wordt "onbekend" zonder eenheid en krijgt `mm` zodra hij bekend is: dat is een herbouw van het formulier (zie boven).
- Handmatig: jog met de muis vasthouden, VoiceOver-labels van de pads, uiterlijk naast de HIG, Windows-host van het laservenster op `FormSpec`.
- `appkit/app/laser_host.rs` kreeg één regel (`refresh_laser_panel`); `form.rs`/`tests/native/macos/layout_invariants.rs` zijn additief gewijzigd.
