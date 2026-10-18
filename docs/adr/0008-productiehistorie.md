# ADR 0008 — Productiehistorie: eigen append-only journal (geen SQLite)

- Status: **Aanvaard (Design)**, 18 oktober 2026 — bindend vanaf de eerste IM17-test; amendement 1 (fase C) onderaan.
- Geldt voor: `burnburn-history` (nieuw, laag 5), `burnburn-runtime`, `burnburn-presentation` (`HistoryStore`-poort)
- Contract: [`contracts-phase-c.md`](../design/contracts-phase-c.md) §IM17

## Context

IM17 vraagt een "geschiedenisdatabase": bevroren `CompiledJob`s, `Execution`s met bewijs (upload, stored,
startverzoek, uitvoering, voltooiing), queue en aantallen, `Unknown`-reconciliatie na herstart. Later: projectzoeken
(IM28), orders/operator (IM31). Volumes zijn klein (duizenden jobs per jaar, tienduizenden evidence-records); de
belangrijkste eisen zijn **crashveiligheid, nooit overschrijven, auditbaarheid** en "intentie vastleggen vóór actie".

## Opties

| | SQLite (`rusqlite`, bundled) | `redb` 4.4.0 | Eigen append-only journal |
|---|---|---|---|
| Rust-native | Nee: C-bibliotheek, `cc` in de build (`c_toolchain`) | Ja (MIT OR Apache-2.0, alleen `libc`) | Ja, alleen `std` + `crc32fast`/`sha2` |
| Past bij evidence-model | Muteerbare rijen; append-only moet je afdwingen | KV met transacties; muteerbaar | Het model *is* een event-log |
| Crashgedrag testbaar met `MemoryFileSystem` | Nee (eigen VFS) | Nee (eigen file-IO) | Ja: via `FileSystem`-poort, foutinjectie per stap |
| Query's | SQL | Handmatige indexen | In-memory index bij openen (klein volume) |
| Formaat stabiel/leesbaar | Ja | Intern formaat, versies | Eigen, gedocumenteerd, JSON-payloads |

## Besluit

1. **Eigen append-only journal** in `burnburn-history`, uitsluitend via `burnburn_io::fs::FileSystem` (zodat
   `MemoryFileSystem`-foutinjectie de crashpaden test). Geen SQLite: C-kern en `c_toolchain` in de runtimegraaf,
   en zijn muteerbare rijen passen slechter bij bewijs dat nooit herschreven mag worden.
2. **Layout** onder de app-datamap: `history/journal-<seq:08>.bbj` (segmenten, rotatie bij ±8 MiB) en
   `history/blobs/<sha256-…>` (content-addressed: `JobPlan`-encoding, RD-bytes, projectsnapshot), blobs via het
   atomaire schrijfpad van io (temp → sync → replace → dir-sync).
3. **Record** = `magic u32 · len u32 · crc32 u32 · payload` (CRC-32/IEEE via `crc32fast`) met payload = canonieke JSON van een getagd
   `HistoryRecord` (`schema`, `seq`, `wall_time` (alleen weergave), `kind`). `append` = write + `sync_file`; pas daarna
   geldt het record als vastgelegd.
4. **Herstel**: bij openen wordt elk segment sequentieel gelezen; een *staart* met korte lengte of foute CRC wordt
   genegeerd en gemeld (`RecoveryReport::TornTail`), niet herschreven; corruptie *midden* in een segment is een fout die
   de historie alleen-lezen opent. Nooit stil truncaten van geldige records.
5. **Intentie vóór actie**: `ExecutionCreated`, `UploadRequested`, `StartRequested` worden gesynct **vóór** de
   bijbehorende `MachineIntent` naar de driver gaat. Na herstart is elke niet-terminale `Execution` `Unknown` tot een
   statusquery het bewijs levert; er wordt nooit automatisch herstart (AGENTS.md).
6. **`ExecutionId`** wordt toegekend door de historie: `max(seen) + 1`, vastgelegd in `ExecutionCreated` — persistent
   over herstarts (contract §1).
7. Compactie/archivering: later; segmenten zijn onveranderlijk zodra geroteerd.

## Gevolgen

- `burnburn-history` (laag 5) hangt af van types, job, machine, io; `presentation` kent alleen de poort
  `HistoryStore` met eigen leesmodellen; `runtime` verbindt ze.
- Testnamen (IM17): `editing_project_does_not_modify_prepared_job_revision`,
  `torn_tail_record_is_ignored_and_reported`, `start_intent_is_durable_before_driver_receives_it`,
  `restart_marks_running_execution_unknown_without_resending_start`.
- Herzien naar `redb` als query's (IM28 zoeken, IM31 orders) in-memory te traag worden; meetcriterium: openen +
  indexeren van 100 000 records > 500 ms op de referentie-Mac.

## Amendement 1 — fase C (18 oktober 2026)

- **Byteformaat vastgelegd** in [`contracts-phase-c.md`](../design/contracts-phase-c.md) §IM17 (segmentkop `BBJ\0` +
  formatversie, record `magic · len · crc32 · payload`, LE, `max_record` 1 MiB, JSON-payload met `schema`/`seq`/`kind`).
- **Gescheurde staart wordt niet getrunceerd én niet verder beschreven:** de volgende append opent een nieuw segment.
  Zo blijft de staart als bewijs en hoeft `FileSystem` geen truncate te kennen.
- **Nieuw portlid `FileSystem::append`** (io, Kern; design-review C-7). `write_all` blijft "volledige inhoud".
  `MemoryFileSystem` krijgt een injectie "append schrijft k van n bytes" voor `torn_tail_record_is_ignored_and_reported`.
- **Recordsoorten:** `IntentRecorded` vervangt de losse `UploadRequested`/`StartRequested` en draagt meteen de
  `ActionId` (IM31), zodat er later geen migratie nodig is. `StateObserved` legt elke actor-overgang vast;
  `Reconciled` noemt zijn basis (statusquery-record of operatorverklaring).
- **`CompiledJobId`** = `ContentHash::of(HashDomain::CompiledJob, …)` (ADR 0004 amendement C); blobs op `Asset`.
- `machine` blijft serde-vrij; history bezit de DTO's met uitputtende mapping.
