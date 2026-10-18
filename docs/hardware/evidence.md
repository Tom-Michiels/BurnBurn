# Hardwarebewijs (eigenaar Protocol)

Per model, firmware, interface en feature: wat is op echte hardware **waargenomen**. Zonder rij hier geldt een feature als
`Unverified` (gedraagt zich als `Unsupported`, AGENTS.md / contracts-phase-c principe 4).

Stand 18-10-2026: **er is nog nooit een echte controller gecontacteerd.** Alle transporttests draaien tegen een fake
controller op `127.0.0.1` in het testproces (`crates/burnburn-runtime/tests/udp.rs`) of een gescripte fake.

| Model | Firmware | Interface | Feature | Bewijs | Datum | Status |
|---|---|---|---|---|---|---|
| — | — | Ethernet/UDP | RD-chunks + checksum, 1-byte ACK/NAK/ERR | OSS-beschrijving (`research/lightburn/ruida/03-transport.md`), geen capture | — | onbevestigd |
| — | — | Ethernet/UDP | identiteit, read-back (stored), status, start, stop | geen | — | onbevestigd (geen dialect) |
| — | — | — | laser-uit-framing (vermogen 0, alleen travel) | geen | — | onbevestigd |
| — | — | — | bestandslijst / benoemde upload / verwijderen | geen | — | onbevestigd |
| — | — | — | instellingen lezen/schrijven | geen | — | onbevestigd |
| — | — | USB | alles | geen | — | niet ondersteund |
