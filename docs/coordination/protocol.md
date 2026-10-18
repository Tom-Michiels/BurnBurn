# Coördinatie Implementatie (protocol/machine/runtime) → andere rollen

Stand 18-10-2026 (fase C: IM16-vervolg, IM17, IM29-software, IM30). Logs: `docs/dev-notes/IM16.md` (vervolg),
`IM17.md`, `IM29.md`, `IM30.md`, mutaties IM14 in `IM14.md`. Hardware: `docs/hardware/evidence.md` (alles onbevestigd).
fmt, `cargo clippy -p burnburn-{machine,runtime,history,cli} --all-targets -- -D warnings` (ook zonder `--no-deps`)
schoon, tests groen, `cargo xtask layers`: 24 crates, 2 triples, no violations.

## Aan Design
- **layers.toml: geen wijziging.** `burnburn-runtime`, `burnburn-history`, `burnburn-cli` stonden al gepland; hun
  afhankelijkheden vallen binnen de allow-lijsten. Root: één regel per crate in `[workspace.dependencies]` +
  `crc32fast = "1.5.2"` (register: kandidaat IM17 → nu in gebruik in history).
- Afwijkingen ter review (details in de logs):
  1. `frame_plan` en de calibratiepatronen staan in `burnburn-machine` (`frame`, `calibration`), niet in `cam`; geen
     `ruida::lower_frame` nodig (frameplan = vermogen 0 + alleen `Travel`, via `lower`).
  2. `IdentityMismatch` als `ExecutionState::Failed` met bewijs i.p.v. `ActorError`; nieuwe toestanden
     `VerifyingIdentity`, `Reverifying` (read-back van een eerdere upload in een nieuwe sessie, nooit jobbytes).
  3. Geen herhaling van een chunk na verloren ACK (strenger dan "begrensd zelfde pakket"): `Unknown`.
  4. Journal: `TornTail` ook in een niet-laatste segment toegestaan als niets erachter staat; anders botst de regel
     "volgende append in nieuw segment" bij de volgende open met "zelfde fout elders → Corrupt". Graag in het contract.
  5. `CompiledJob.plan` = hash van `JobPlanId` (geen constructor uit hash in job). `CompiledJobId` gebruikt domein
     `HistoryRecord` + tag `compiled-job` tot `HashDomain::CompiledJob` (C-5) bestaat.
  6. Setup-revisie = `machine::CalibratedProfile` (profiel + calibratie); `CompiledJob.profile` legt die vast.
  7. CLI gebruikt `run_task` direct (niet `AppController`); plugin-manifestschema in `burnburn_cli::plugin_manifest`
     (nog geen `burnburn-plugin-api`-crate).
- Nieuwe dependencies: geen (serde/serde_json/tempfile bestonden; `crc32fast` stond in het register).

## Aan Kern (io)
- C-7 gezien: `FileSystem::{append, list_dir}` staan in de poort, `StdFileSystem`/`MemoryFileSystem` gaven tijdens mijn
  ronde nog `Err(Other)`. Daarom: `burnburn_history::JournalFs` is nu een blanket-alias van `FileSystem`;
  `burnburn_runtime::StdJournalFs` levert een werkende `append` (OpenOptions::append) + `list_dir`; history-tests wikkelen
  `MemoryFileSystem` met een eigen `append` (+ gescheurde-append-injectie). Zodra jullie versies groen zijn, vervalt
  `StdJournalFs` en gebruiken de tests `MemoryFileSystem` direct — seintje graag.
- `io::write_file` (atomair) bestaat nog niet; history en cli hebben elk een kleine kopie van temp → sync → replace → dir-sync.
- C-5: graag `HashDomain::CompiledJob` ("compiled-job"); ik zet `compiled_job_id` dan om.

## Aan User-flow (presentation)
- Verzoek: `compile_options()` (0,01 mm, Absolute, MachineOrigin) publiek maken; de CLI dupliceert de constante nu en
  `cli_and_gui_compile_same_revision_to_same_normalized_plan` bewaakt de gelijkheid.
- `HistoryStore`-poort: `burnburn_history::History` + `burnburn_runtime::MachineDriver` zijn klaar om erachter te zetten
  (intent pas terug na sync; `IntentReceipt.fresh == false` = zelfde klik, niets sturen; `ActionId` per klik).

## Aan CAM
- Calibratie: scan-offset/kerf uit `CalibrationSet` worden nog nergens toegepast (compile/lower). `MachineProfile` zelf is
  ongewijzigd; als calibratie in het profiel hoort (contract §IM29), kan `CalibratedProfile` daarin opgaan.

## Aan Testing
- Mutaties op jullie advies gelogd: IM16 (tweede Start na reconnect, cancel stuurt verzoek, Idle = Completed, ACK =
  Running) en IM14 `file_errors` (4 mutaties). Open: 7 `decode_errors`, `unknown_f2_01_…`.
- Jullie meldingen "runtime-driver-tests falen / clippy-fout in record.rs" waren tussentoestanden van mijn rode fase; nu groen.
- C-11 `FakeRuidaController`: mijn fake staat in `crates/burnburn-runtime/tests/udp.rs` (thread op 127.0.0.1:0, checksum-
  controle, script per datagram); verplaatsen naar test-support mag.
- Fuzz-kandidaat: `History::open` over willekeurige segmentbytes; `scan_segment` is paniekvrij geschreven.

---

# Integratie-productie (18-10-2026) — van User-flow, ter review (wijzigingen in jullie crates)

Opdracht "koppel burnburn-history aan HistoryStore" (log `docs/dev-notes/integratie-productie.md`). TDD, alles groen,
clippy `-D warnings` schoon op machine/history/runtime (`cargo fmt` over die crates gedraaid: ook bestaande
afwijkingen zijn daardoor geformatteerd).
- **machine:** `ExecutionActor::resume_unknown(Execution)` (alleen `Unknown`, alleen op een vrije actor; daarna kan
  alleen een statusquery) en `settle_unknown(terminal)` (alleen vanuit `Unknown`, stuurt niets). Nieuwe varianten
  `UnknownReason::InterruptedByRestart`, `FailReason::{OperatorDidNotRun, OperatorPartial}`. Tests in
  `tests/execution.rs` (3).
- **history:** mapping van die varianten; `ExecutionStateDto::to_state()`; records `order-recorded` (`OrderRecord`) en
  `order-linked` + `record_order`/`link_order`/`orders`/`order_of`. Test `orders_and_their_executions_survive_a_restart`.
  Let op: `History::observe` gaat uit van evidence-index = historielengte; klopt niet voor een geadopteerde execution
  (adapter omzeilt dit). Graag beoordelen of `observe` een startpositie moet krijgen.
- **runtime:** `JournalHistoryStore` (implementeert `presentation::HistoryStore` over `History` + `JournalFs` +
  `WallClock`), dependency op `burnburn-presentation` (layers.toml staat het toe). Tests `tests/history_store.rs` (4,
  MemoryFileSystem + ScriptedPeer). `MachineDriver` ongewijzigd; de twee paden bestaan nu naast elkaar — graag
  beslissen of `MachineDriver` opgaat in `MachineService` + adapter.
