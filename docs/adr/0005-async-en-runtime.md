# ADR 0005 — Concurrency: eigen threads + channels, sans-IO actor, geen Tokio (voorlopig)

- Status: **Aanvaard (Design)**, 17 oktober 2026. Herzien bij USB/serieel (IM29) of meerdere machines (IM31).
- Geldt voor: `burnburn-machine`, `burnburn-runtime`, `burnburn-presentation` (taakpoort)

## Context

Plan: één eigenaar per documentsessie op de UI-thread, zware taken op workers met revisiecontrole, één eigenaar
per machineverbinding, fake clock vóór live IO. Tokio is in het plan genoemd als *voorbeeld*. Ruida-Ethernet is
UDP met korte request/ACK-uitwisselingen; er zijn weinig gelijktijdige verbindingen (meestal één machine).

## Opties

| | Tokio | Eigen threads + channels (sans-IO kern) |
|---|---|---|
| Tijd testen | `tokio::time::pause` werkt alleen binnen Tokio; onze `Clock`-poort moet er alsnog naast | Actor leest de geïnjecteerde `Clock`; `FakeClock` + `ScriptedPeer` (bestaan al in test-support) volstaan |
| Determinisme | Scheduler-volgorde niet deterministisch tussen taken | Actor is single-threaded en stapgestuurd (`poll`) |
| Afhankelijkheden | tokio 1.53.3 + mio, socket2 e.d. | alleen `std` + `crossbeam-channel` 0.5.17 |
| AppKit-integratie | Eigen runtime-thread nodig, wake-up blijft nodig | Zelfde wake-up |
| Schaal (vele sockets) | Sterk | Niet nodig voor 1–4 controllers |

## Besluit

1. **Machine-actor is single-threaded en poll-gestuurd** (`ExecutionActor<P: ControllerPort>` met geïnjecteerde
   `Clock`, contract §9; zo gebouwd door Protocol). Geen threads, sockets of systeemklok in `burnburn-machine`; de
   poort is niet-blokkerend. Alle verplichte foutpadtests draaien met `FakeClock` + `ScriptedPeer` zonder echte tijd.
2. **`burnburn-runtime`** levert per verbinding één *driver-thread* die de actor bezit: hij ontvangt
   `MachineIntent`s via een `crossbeam_channel::Receiver` (`recv_timeout` als tick), roept de actor-methoden en
   `poll()` aan, en stuurt toestandsrapporten via de host-wake-up terug. De `RuidaUdpPort` gebruikt
   `std::net::UdpSocket` in non-blocking modus en `burnburn-ruida` voor de bytes.
3. **Workers**: een vaste pool (`std::thread`, standaard `available_parallelism − 1`, min 1),
   `crossbeam-channel` als werkqueue, voert `presentation::run_task` uit op verzoeken uit `take_task_requests`; resultaten via de host-wake-up (ADR 0002) naar `AppController::deliver`. Annulering via
   `CancellationToken`; verwerping op revisie in `AppController`.
4. **Geen Tokio** in de productiegraaf tot een concreet werkpakket het nodig heeft. `layers.toml` verbiedt
   `async_runtime` transitief in alle kerncrates; voor `runtime` is het niet geblokkeerd, maar toevoegen vereist een
   herziening van deze ADR.
5. **Klok**: `Clock` (types) is de enige tijdsbron in domein- en actorlogica. `runtime::SystemClock` gebruikt
   `std::time::Instant` met een vaste oorsprong. Wandkloktijd (`SystemTime`) alleen voor historie-/logweergave, via
   een aparte `WallClock`-poort in runtime, nooit in hashes of toestandsbeslissingen.
6. Een UI-thread wacht nooit op een channel; hij wordt gewekt.

## Gevolgen

- Machine-tests wikkelen `ScriptedPeer<Envelope, Incoming>` als `ControllerPort` (Testing-advies in
  `docs/coordination/testing.md`), zodat dezelfde actor in tests en productie draait.
- Een live UDP-test is een aparte, standaard uitgeschakelde hardwaretest (AGENTS.md).

## Open

- Herziening bij USB (IM29): blocking USB-IO past ook in het driver-thread-model; alleen bij veel gelijktijdige
  apparaten (IM31) wordt Tokio opnieuw overwogen.
