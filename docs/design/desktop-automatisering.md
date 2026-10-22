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

## Open

- Host-aansluiting: de macOS- en Windows-host moeten `AutomationHost` implementeren (open/importeer via de bestaande
  intents), een luisterthread starten wanneer de voorkeur aan staat en `HostStatus` uit de documentstaat vullen. Zie
  `docs/plan/integratie-wachtrij.md` (regel G5-tekst-api).
- Echte compatibiliteit met LightBurn-clients: niet geverifieerd.
