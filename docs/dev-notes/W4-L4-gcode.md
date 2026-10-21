# W4-L4: G-code-backend (GRBL 1.1)

Crate `burnburn-gcode` (laag 3, naast `burnburn-ruida`), CLI-uitbreiding in `burnburn-cli`. Alle commando's met
`CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/w4l4` en `--offline` (het nieuwe workspace-lid vroeg een
lockfile-update zonder nieuwe externe dependency; `--locked` werkt daarna).

## Onbevestigd (lees dit eerst)

Er is **geen enkele GRBL-controller gecontacteerd en er is geen seriële adapter**. Alles hieronder is bewezen
tegen een onafhankelijke interpreter en een fake controller die uit de publieke beschrijving is gemodelleerd.
Upload/start/streaming naar echte hardware, `$32=1`-gedrag, M4-dynamisch vermogen, luchtondersteuning, G54 en de
GRBL-foutcodetabel (uit het geheugen) zijn onbewezen; zie de rijen in `docs/hardware/evidence.md`. De CLI weigert
`upload`/`start` met een GRBL-profiel. Een echte seriële poort kan niet zonder OS-specifieke code of een
serial-crate met C-toolchain (verboden voor kerncrates): poort-trait `SerialPort` + eerlijke melding.

## Cycli (rood -> groen)

| Test(s) | Rood: verwachte fout | Rood-commando en waargenomen resultaat | Groen-commando en resultaat |
|---|---|---|---|
| `tests/lower.rs` (21 tests: rechthoek, tekst, M3/M4, `$30`-schaal, passes, fill X/Y, grayscale, angled, lucht, G54, afronding, weigeringen, proptest) | lege lowering-stub: leeg programma | `cargo test -p burnburn-gcode --offline --test lower`: 0 passed, 21 failed (bv. `-0 != 120`) | na implementatie 18 groen, 3 rood door capability-matrix/testbug; testbug (bounds incl. startpunt) in test gerepareerd |
| `tests/capabilities.rs` (4) | conservatieve defaults: ScanAngle `Unverified` | `--test capabilities`: 3 failed | `grbl_capabilities` ingevuld; `cargo test -p burnburn-gcode --offline`: alles ok (lower 21/21) |
| `tests/messages.rs` (12) | parser-stub geeft `Other` | `--test messages`: 10 failed, 2 passed (proptests slagen triviaal) | 12 passed |
| `tests/prepare.rs` (9) | stub `Err(Empty)` | `--test prepare`: 8 failed, 1 passed | 9 passed |
| `tests/session.rs` (26, incl. proptest `character_counting_never_overflows_the_rx_buffer`) | sessie-stub doet niets | `--test session`: 0 passed, 26 failed | 26 passed; mutatiecheck: `usable()` = `rx_buffer` i.p.v. `rx_buffer-1` laat de proptest falen (1 failed), teruggezet: 26 passed |
| `tests/driver.rs` (11, fake GRBL met echt overlopende 128-byte buffer) | driver-stub | `--test driver`: 0 passed, 11 failed | 11 passed |
| `tests/console.rs` (17) | console/macro-stubs | `--test console`: 0 passed, 17 failed | 17 passed |
| `cli/tests/gcode.rs` (9) | profielbestand kent `backend` niet, compile schrijft geen G-code | `cargo test -p burnburn-cli --offline --test gcode`: 8 failed, 1 passed (`invalid_backend_sections…` slaagde triviaal op `deny_unknown_fields`) | na implementatie 8 passed, 1 failed (testfout: jobhash `00` ongeldig), daarna 9 passed; bestaande CLI-tests (6) ongewijzigd groen |
| `tests/interpreter_oracle.rs` (3) | n.v.t. (orakel zelf, met de hand uitgerekend) | – | 3 passed |

Het fuzz-doel `fuzz/fuzz_targets/grbl_status.rs` compileert (`cargo check --offline --bin grbl_status` in `fuzz/`) maar is
**niet gedraaid** (nightly + libFuzzer niet beschikbaar in deze run). Property-tests (proptest) voor parser en
RX-venster draaien wel.

## Ontwerpkeuzes

- Lowering: absolute coördinaten, eenmalig gekwantiseerd (round half away from zero); `M4`/`M3 S0` eenmalig vóór de
  eerste brandbeweging; `G0` en `S0` zijn laser uit; overscan als `G1 S0`; maximaal vermogen van het settingsblok.
- Capability-matrix: ScanAngle en GrayscaleModulation `Supported` (één G1), ZStep/DotMode/RampLength `Unsupported`,
  CommonLine `Unverified`. Luchtondersteuning staat niet in `job::Feature` (andere lane): per GRBL-profiel geweigerd
  met `AirAssistUnavailable`. Rotary en buiten-bed worden geweigerd. `check()` geeft alle weigeringen tegelijk.
- Sessie: realtime-bytes tellen niet mee; nooit meer dan `rx_buffer-1` onbevestigde bytes; `ok` is niet "klaar"
  (Draining -> Completed pas bij `Idle` na de laatste `ok`); `error:N` op een joblijn = Failed + `!`, resume geweigerd;
  transportverlies = `Unknown`; start eist `$32=1` en `$30` teruggelezen, status `Idle`, geen alarm.
- Console/macro's weigeren tijdens een job; macrotekstformaat v1 is eigen vondst (geen LightBurn-formaat).

## Handmatig / open

- Echte GRBL-hardware, echte seriële adapter (macOS/Windows), Windows-run: niet gedaan.
- UI-routes: wachtrij-regels W4-L4 in `docs/plan/integratie-wachtrij.md`.
- `presentation`-export (`TaskInput::ExportGcode`) en `UiIntent::SetMachineProfile` met backend: bij L1.
- Geen GRBL-history-journal (die legt RD-bytes vast); CLI weigert `--history` voor GRBL.
