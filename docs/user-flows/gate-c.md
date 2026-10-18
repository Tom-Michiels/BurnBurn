# Poort C — dagelijkse productie op één Ruida-configuratie (IM16–IM20)

**Poortcriterium:** de gekozen Ruida-configuratie doorstaat offline protocol- en fouttests en daarna hardwarevalidatie.
Productie gaat via expliciete preview → upload → framing → start. Upload telt geen gemaakt werkstuk; een verloren
startantwoord blijft `Unknown`. Geen webview als hoofdinterface.

Machinehandelingen zijn `MachineIntent{execution_id, machine_id, action, required_state}` naar de machine-actor; ze
komen nooit in de documenthistorie en worden nooit door undo, heropenen of reconnect herhaald. Statusweergave volgt
uitsluitend bewijs (ACK ≠ Stored ≠ Completed). Tests draaien met fake clock en fake controller; hardwarebewijs per
model/firmware/interface wordt apart en als zodanig gerapporteerd.

Scenario's (headless, `crates/burnburn-presentation/tests/gate_c.rs`, fake controller + fake clock): groen
`headless_gate_c_prepare_upload_start_completes`, `upload_ack_is_not_shown_as_stored`,
`headless_gate_c_lost_start_reply_stays_unknown`, `lost_start_reply_does_not_cause_second_start_after_reconnect`,
`cancel_after_start_is_unknown_with_explanation`, `preview_export_and_upload_name_the_same_job_plan`,
`editing_project_does_not_modify_prepared_job_revision`, `undo_redo_never_emits_machine_intent`,
`start_intent_is_recorded_before_the_controller_receives_it`, `identity_mismatch_uploads_nothing`, `restart_marks_unfinished_execution_unknown_without_resending_start` (ook over het
echte journal in `burnburn-runtime/tests/history_store.rs`),
`restarted_unknown_execution_is_reconciled_by_the_operator_after_a_status_query` ("Registreer uitkomst" pas na een
statusantwoord ≠ Running; duurzaam, nooit automatisch). Framing (C3) en hardwarebewijs: gepland.

Route zoals gebouwd: `CommandKey::Compile` → `CommandKey::PrepareJob` (taak: exact dit plan → RD-bytes) → jobkaart
(`DocumentViewModel.job`) met uploadknop → `UiIntent::Machine { action: ActionId, seen }` → `MachineIntent` →
`MachinePort::dispatch` (één `MachineService` per machine, intent eerst via `HistoryStore`) → `ExecutionActor`.
Statuspaneel: `AppController::machine_view()` (status uit bewijs; knoppen met idempotentiesleutel).

## C1 — Job klaarzetten (W01)

1. Produceren-werkruimte ▸ "Job klaarzetten" (Cmd+Return / Ctrl+Return) → `UiIntent::Invoke(PrepareJob)` →
   `TaskRequest::Compile` → `DocumentCommand::FreezeJob{plan_hash, source_rev, machine_rev, material_rev, tool_versions}`
   (job is projectdata, één entry) → *jobkaart: naam, planhash (kort), revisies, "bevroren"*.
2. Gebruiker bewerkt daarna het ontwerp → *jobkaart blijft ongewijzigd; banner "ontwerp gewijzigd sinds job – nieuwe
   conceptversie"* (`editing_project_does_not_modify_prepared_job_revision`).
3. "Opnieuw produceren" → expliciet nieuwe `Execution` van dezelfde job; geen impliciete kopie.

Fout: preflightfout aanwezig → Klaarzetten disabled met link naar fouten (C09).

## C2 — Uploaden met eerlijke voortgang (M04)

1. Jobkaart ▸ Upload → bevestiging toont machine, jobrevisie, bestandsnaam op controller → `MachineIntent::Upload` →
   *voortgang in drie stappen: "Verzonden x %" → "Protocol-ACK" → "Opslag bevestigd"*.
2. Naamconflict → *keuze: overschrijven / andere naam / annuleer*; volle opslag → *gerichte melding*, toestand `Failed`
   met oorzaak; timeout → `Unknown` met "controleer controller" (geen automatische tweede upload).
3. Verbinding valt weg tijdens upload → `Unknown`; reconnect → `MachineIntent::QueryStoredFiles` (alleen lezen) →
   reconciliatie; nooit automatisch start.

Annuleren: tijdens verzenden "Stop upload" → `MachineIntent::AbortTransfer`; een al opgeslagen controllerbestand wordt
niet impliciet verwijderd.

## C3 — Framen en bewegen (M05)

1. "Frame" (laser uit) → *coördinatenpaneel met document-, bed- en jobcoördinaten, elk gelabeld* → `MachineIntent::Frame{bounds}`;
   bounds komen uit het beoordeelde plan.
2. Joggen met pijltoetsen in machinepaneel (niet canvas): focus vereist, toetsherhaling begrensd.
3. Onbekende aspositie → *"positie onbekend" (geen 0,0)*; Frame disabled tot positie bekend of gehomed.

## C4 — Start, pauze, hervatten, stop (M06)

1. Start → *bevestigingspaneel: machine, jobrevisie, plaatsing/origin, materiaal* → expliciete klik → `MachineIntent::Start{execution_id}`.
2. Verloren antwoord → *"Start onzeker (Unknown)"*; knop Start blijft disabled voor deze execution
   (`lost_start_reply_does_not_cause_second_start_after_reconnect`).
3. Pauze/Hervatten/Stop → eigen intents; "Stop" heet "Software-stop" en toont dat hardware-noodstop leidend blijft.

## C5 — Wachtrij en historie (W02, W03)

- *Wachtrijregels met toestand Draft → Compiled → Uploading → Stored → StartRequested → Running → Completed, plus Failed,
  Cancelled, Unknown*; succesvolle upload verhoogt geen voltooid-aantal (W02).
- `Unknown` → *actie "Registreer uitkomst" (voltooid/mislukt/onbekend) met operatornaam* → `RecordExecutionOutcome`.
- Historie (W03): materiaal, aantallen, schatting naast gemeten duur, mislukte/afgebroken runs vindbaar; "Herhaal" opent de
  snapshot en toont verschillen met actuele recepten vóór nieuwe execution.

## C6 — Verbindingsproblemen onderzoeken (M07)

Help ▸ Diagnose exporteren → *overzicht van wat wordt meegestuurd (profielrevisie, jobhash, protocolgebeurtenissen),
privé-assets en verbindingsdata uitvinkbaar* → save-sheet. Replay in CLI/headless reproduceert toestand zonder machine.

## C7 — Tekst voorbereiden (E07)

1. Tekst-tool (T), klik, typ via native tekstinvoer (IME-compositie is concept, geen geometrie) → Return/klik buiten →
   `CreateText{string, font, size, spacing}` (één entry).
2. Inspecteur: regelafstand, letterafstand → `SetTextStyle`; tekst blijft tekst na save/heropen.
3. Ontbrekend font bij openen → *opgeslagen contourweergave + melding "font X ontbreekt" met keuze vervangen*.
4. Omzetten naar contouren / stencilbruggen → `ConvertTextToPaths{keep_source}`.

## C8 — Gravurebeeld (T09) en bewerkingsmodi (C05)

1. Afbeelding aan Image-laag → inspecteur: fysieke maat, rasterinterval, tooncorrectie, resampling, dither →
   `SetImageProcessing` → *drie weergaven: bron, verwerkt, geplande scanruns*.
2. Vergroten 50 → 75 mm → *dither opnieuw vanuit bron* (`dither_runs_are_regenerated_from_source_after_physical_resize`).
3. Vooraf geditherd beeld → modus "vaste pixelschaal" met waarschuwing bij resample.
4. Modus Line/Fill/Image kiezen → *effect zichtbaar in plan*; niet-gevalideerde modus (bijv. grayscale) → *keuze zichtbaar
   maar geblokkeerd met "ontbrekende capability: …"* (C05, M02).

## C9 — Lokaal traceherstel en controle (T05, T06)

1. In tracer: regio-penseel/kader → `SetTraceRegion{mask, threshold}` → `TaskRequest::RetraceRegion{anchors}` → *alleen
   regio verandert; rest bitgelijk*; correcties met verloren koppeling → *lijst "opnieuw koppelen"* (T05).
2. Fittingtolerantie `0,05 mm`; gat/hoek beschermen → `ProtectFeature` → *afwijkingskaart, verwijderde componenten
   gelokaliseerd, knoop-/padtelling* (T06). Toepassen = één undo.

## C10 — Offline, toegankelijk, installeren (X01, X03, X07)

- X01: alle bovenstaande flows zonder netwerk; ontbrekende optionele dienst → *niet-blokkerende melding*. Update-aanbod
  wordt uitgesteld zolang `AppViewModel.machine_busy`.
- X03: proceslagen en solverstatus hebben naam + symbool; toetsenbordfocus op alle panelen; NL/EN en komma/punt
  veranderen geen fysieke waarde (`locale_does_not_change_physical_value`). 100–200 % schaal: **native/handmatig**.
- X07: installeren/updaten/herstellen per OS-release: native en **handmatig** beproefd; projectmigratie bewaart vorige versie.
- Sluiten met dirty document en Annuleer (A7) blijft geldig (`closing_dirty_native_document_then_cancel_keeps_session_open`).
