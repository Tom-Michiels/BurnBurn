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

## Benodigd bewijs voor het laservenster (W1-L4a, 18-10-2026)

Het domein (`burnburn-machine::{LiveRequest, Capabilities}`, `burnburn-runtime::RegistryLiveDialect`) is gebouwd en getest
tegen een fake controller. **Elke feature hieronder blijft `Unverified` (geweigerd) tot er een waarnemingsrij in de tabel
bovenaan staat.** "Register" = de publieke commandonaam in `research/lightburn/ruida/command-registry.json`
(MeerK40t `rdjob.py`, geen bewijs van firmwareondersteuning). Per model + firmware + interface vastleggen: gestuurde bytes,
ontvangen bytes, wat de kop deed (foto/video of meting) en datum. Een ACK alleen bewijst alleen ontvangst.

| Feature (`LiveFeature`) | Register / wire | Wat er nodig is | Zonder dit |
|---|---|---|---|
| `Jog` | `KEYDOWN_*` (D8 20..27) / `KEYUP_*` (D8 30..37); encoder bestaat | Capture van key-down/up op het concrete model: beweegt de kop tijdens ingedrukt en stopt hij bij loslaten; richting per toets (X_LEFT = machine −X?); gedrag bij verloren key-up | geweigerd; geen blijvende jog mogelijk |
| `Home` | `HOME_XY` (D8 2A) / `HOME_Z` / `HOME_U`; encoder bestaat | Capture + bewijs dat de kop thuis komt; het completion-signaal (statusbit) — de ACK is dat niet | geweigerd; "gehomed" wordt nooit aangenomen |
| `MoveAbsolute` (numeriek, go to origin, step jog, position laser) | `RAPID_MOVE_XY` (D9 10): `options(1)`, `abscoord(5)`, `abscoord(5)`; **geen encoder** | Betekenis van de options-byte (snelheid/modus/relatief?), coördinaatframe (machine vs. user origin), gemeten kopposities | geweigerd (`NotSupported` bij elke dialect) |
| `PauseResume` | `PAUSE_PROCESS` (D8 02) / `RESTORE_PROCESS` (D8 03); encoder bestaat | Capture tijdens een echte job; welke statusbits "gepauzeerd"/"hervat" bevestigen; of RESTORE echt resume is | geweigerd; pauze/hervat tonen nooit "gepauzeerd" op een ACK |
| `ReadPosition` | `GET_SETTING` (DA 00) + `MEM_CURRENT_X/Y` (04 21 / 04 31); request-encoder bestaat, **geen reply-decoder** | Antwoordframing, eenheid (µm?), teken en of de waarde ten opzichte van machine-nul is | positie blijft "onbekend" (nooit 0); `RegistryLiveDialect` laat het verzoek weg |
| `FileList` / `FileMetadata` / `FileRun` / `FileDelete` | niet in het register | Capture van lijst/metadata/run/delete op het concrete model, plus bewijs via een tweede lijst | geweigerd |
| `FileUpload` (Save RD naar controller, benoemd) | `DOCUMENT_FILE_UPLOAD` (E5 00) in register, bestandsnaamafhandeling onbekend | Capture van benoemde upload + lijst-read-back; hoe `Stop`/onderbreking een deelbestand achterlaat | geweigerd (`NoWireForm`, ook als de capability `Supported` zou zijn) |
| Frame laser-uit (bestaand, M05) | `lower` van het frame-plan (vermogen 0, alleen travel) | Meting dat de buis uit blijft tijdens zo'n programma op het concrete model | frame blijft een gewone job-upload; rij hierboven (`laser-uit-framing`) geldt |
| Frame-runs als job | upload + start zoals elke job | de bestaande rijen voor RD-chunks/identiteit/read-back/start | zie bovenaan |
| `Stop` via `RegistryLiveDialect` | `STOP_PROCESS` (D8 01); encoder bestaat | Capture + bevestiging dat de kop stopt en de status dat bevestigt (hardware-noodstop blijft leidend) | `Cancel` na start blijft `Unknown(StopRequested)` |

Opmerking over `START_PROCESS` (D8 00): niet bereikbaar via de live-vocabulaire. Een job starten blijft `Request::Start` op een
`Stored` execution, en heeft eveneens een eigen rij nodig (zie bovenaan: start).
