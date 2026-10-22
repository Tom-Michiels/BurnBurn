# Desktop-automatisering (API01)

Implementatie: `crates/burnburn-runtime/src/automation.rs`. Tests: `crates/burnburn-runtime/tests/automation.rs`.
Voorkeur: `PrefKey::DesktopAutomation` (id `desktop_automation`, standaard **uit**).

## Doel en scope

Een ander programma op **dezelfde computer** kan BurnBurn vragen een project te openen, een tekenbestand te
importeren of de toestand te melden. Het protocol is BurnBurns eigen; de werkwoorden volgen de LightBurn-
automatisering, maar byte-voor-byte compatibiliteit met LightBurn is **niet** geclaimd en niet geverifieerd
(geen bewijs in `research/lightburn/`).

Buiten scope, bewust: elke machineactie. Er is geen werkwoord voor start, frame, jog, home, verzenden,
uploaden of stoppen. Die werkwoorden worden met `ERR:FORBIDDEN` geweigerd, en de host-trait `AutomationHost`
heeft alleen `status`, `load` en `import`; een machine, job of upload is er niet mee te bereiken.

## Veiligheid

- **Opt-in**: standaard uit. Uit betekent: de service antwoordt op niets en voert niets uit; de host bindt dan geen socket.
- **Alleen loopback**: `AutomationEndpoint::new` weigert een socket die niet aan loopback gebonden is
  (`EndpointError::NotLoopback`); `bind_loopback` bindt op 127.0.0.1. Datagrammen van andere afzenders worden
  zonder antwoord genegeerd (`Served::DroppedNonLoopback`).
- **Geen stil overschrijven**: `LOADFILE` op een document met niet-bewaarde wijzigingen geeft `ERR:DIRTY` en roept de
  host niet aan. Er is bewust geen `FORCELOAD`. De gebruiker beslist in de applicatie.
- **Padcontrole**: absoluut, geen stuurtekens, geen `..`, maximaal 1024 bytes, en een bekende extensie
  (`LOADFILE`: burnburn, lbrn2, lbrn; `IMPORT`: svg, dxf, png, jpg, jpeg, bmp, tif, tiff). De host mag nog steeds weigeren.
- Document-undo speelt nooit machineacties af; deze laag raakt geen machine.

## Protocol

Eén datagram is één verzoek (UTF-8, maximaal 1100 bytes, eindigend op een optionele regeleinde); er komt precies
één antwoorddatagram terug naar de afzender. Werkwoorden zijn hoofdletter-ongevoelig. Standaardpoort 19850
(niet die van LightBurn, zodat beide programma's naast elkaar kunnen draaien).

| Verzoek | Antwoord |
|---|---|
| `PING` | `OK:PONG` |
| `STATUS` | `OK:<READY\|BUSY>:<CLEAN\|DIRTY\|NODOC>` |
| `LOADFILE:<pad>` | `OK`, of `ERR:DIRTY`, `ERR:BUSY`, `ERR:NOTFOUND`, `ERR:FORMAT`, `ERR:FAILED`, `ERR:PATH` |
| `IMPORT:<pad>` | `OK` (één undo-stap, ook bij een gewijzigd document), of `ERR:NODOC`, `ERR:BUSY`, `ERR:NOTFOUND`, `ERR:FORMAT`, `ERR:FAILED`, `ERR:PATH` |
| machinewerkwoord | `ERR:FORBIDDEN` |
| onbekend werkwoord | `ERR:UNKNOWN` |
| onleesbaar, te groot, argument ontbreekt of is overbodig | `ERR:MALFORMED` |

`OK` betekent alleen dat de host de opdracht heeft uitgevoerd. Een verloren antwoord kan de client herhalen: `PING`, `STATUS`
en `IMPORT` hebben geen schade bij herhaling behalve een tweede import; `LOADFILE` is idempotent zolang het document
niet is gewijzigd. De client controleert met `STATUS` voordat hij `IMPORT` herhaalt.

## Host-aansluiting (G5-native)

Gedeelde code: `crates/burnburn-macos/src/automation_host.rs` (door de Windows-crate via `#[path]` meegecompileerd).

- `ChannelHost` is de `AutomationHost` van de luisterthread: `status()` leest de laatste status die de UI-thread
  publiceerde (na elke refresh), `load`/`import` geven een `Job` aan de UI-thread en wachten maximaal 20 s op het antwoord.
  Een bestand dat niet bestaat wordt hier al `ERR:NOTFOUND` (via de geïnjecteerde bestandssysteempoort).
- `AutomationPump` draait op de UI-thread en wordt na elke update en elke wake-up gestapt. Hij start één job tegelijk
  met gewone intents: `LOADFILE` = `UiIntent::Open` (een nieuw venster; een schoon document blijft staan, er wordt niets
  vervangen), `IMPORT` = `UiIntent::Import` + `Invoke(CommitImport)` zodra de preview klaar is (één undo-stap). Een
  tekening zonder eenheid kan alleen de gebruiker beslissen: de preview wordt geannuleerd en het antwoord is `ERR:FORMAT`.
  Een mislukte taak (`ShowError`) is `ERR:FAILED`; een job die de luisteraar opgaf of de tijdslimiet haalt wordt gedropt.
- `AutomationRuntime` start de luisterthread (UDP op 127.0.0.1, leestimeout 100 ms) alleen zolang de voorkeur aan staat,
  stopt hem bij uitzetten en bij afsluiten (thread gejoind, poort weer vrij). Een poort die bezet is laat automatisering
  uit en meldt waarom; het wordt pas opnieuw geprobeerd nadat de voorkeur uit en weer aan is gezet.
- Nooit een machineactie: de pomp stuurt alleen `Open`, `Import`, `CommitImport` en `CancelImport`. De tests leggen elke
  verzonden intent vast (`sent`) en keuren alleen die vier goed; machinewerkwoorden blijven `ERR:FORBIDDEN`.

Tests: `crates/burnburn-macos/tests/automation_host.rs`, `crates/burnburn-windows/tests/automation_host.rs`, native
`tests/native/macos/automation.rs`.

## Open

- Een echte client buiten het proces en de Windows-run van de wiring (`win32/mod.rs`): handmatig/onbevestigd.
- `LOADFILE` opent in een nieuw venster in plaats van het huidige document te vervangen (de trait zegt "in place"); een
  leeg schoon venster blijft dus naast het geopende project staan.
- Echte compatibiliteit met LightBurn-clients: niet geverifieerd.
