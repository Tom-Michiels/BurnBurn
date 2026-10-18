# Poort A — rechthoek, maat, undo, bewaren, heropenen (IM01–IM08)

**Poortcriterium:** teken 120 × 80 mm, verander naar 150 × 80 mm, undo, save en heropen op Mac. De headless route
bereikt exact dezelfde documentinhoud. Windows-host herhaalt de maatwijziging met dezelfde kern (IM08).

Intentie- en effectnamen volgen `docs/design/contracts.md` §6–7 en de implementatie in
`crates/burnburn-presentation` (zie [presentation-api.md](presentation-api.md)).

Stories: **P01** (bewaren), **P02** (undo/redo), **E01** (deel: rechthoek, getypte maat, annuleren),
**E03** (deel: selectie/objectboom-sync), **X02** (command-ID's, sneltoetsen), **X03** (deel: komma/punt),
**X07** (deel: Windows-proef). Scenariotest: `headless_gate_a_rectangle_resize_undo_save_reopen`.

## A1 — Nieuw document openen

| # | Gebruiker (Mac / Win) | Intentie | Kern | Viewmodel |
|---|---|---|---|---|
| 1 | Archief ▸ Nieuw, Cmd+N / Ctrl+N | `UiIntent::NewDocument` | nieuw `Document`, revisie r0 | `HostEffect::OpenWindow{doc}`; titel "Naamloos", `dirty=false`, geen objecten, tool `Select`, Undo/Redo disabled |

Fout: geen. Twee keer Cmd+N geeft twee sessies, elk met eigen `DocumentId`, eigen selectie en eigen historie
(`two_windows_do_not_share_selection`).

## A2 — Rechthoek tekenen met slepen (E01, P02)

| # | Gebruiker | Intentie | Kern | Viewmodel |
|---|---|---|---|---|
| 1 | Toolbar ▸ Rechthoek of toets R | `Invoke(ToolRectangle)` | — | `tool=Rectangle`, `tool_state=Idle` |
| 2 | Muis neer op (0,0) | `Canvas(Pointer Down)` | — | `tool_state=Preview{(0,0)-(0,0)}` |
| 3 | Slepen naar (120,80) | `Canvas(Pointer Move)` × n | — (geen revisie) | preview volgt; maatlabel "120 × 80 mm" |
| 4 | Loslaten | `Canvas(Pointer Up)` | `DocumentCommand{CreateRectangle{rect,name}}` op verwachte revisie r0 → r1, één historie-entry | object in boom, geselecteerd; `dirty=true`; Undo-label "Ongedaan maken Rechthoek" |

- Shift beperkt tot vierkant, Option/Alt tekent vanuit centrum, Cmd/Ctrl onderdrukt snappen (E04, later IM09).
- Een sleep in negatieve richting levert dezelfde genormaliseerde rechthoek (min/max).

**Annuleren:** Esc tijdens slepen (`Canvas(Key Escape)`), venster verliest focus/capture (`Canvas(Pointer Cancel)`),
gereedschapwissel, of loslaten zonder oppervlak → `ToolState::Cancelled` → `Idle`; documentrevisie en historie
ongewijzigd, geen half object (`drag_cancel_leaves_document_and_history_unchanged`,
`cancelled_drag_creates_no_history_entry`, `zero_area_release_creates_nothing`).

## A3 — Rechthoek met getypte maat (E01, X03)

| # | Gebruiker | Intentie | Kern | Viewmodel |
|---|---|---|---|---|
| 1 | Rechthoek geselecteerd; inspecteur toont B 120, H 80 mm | — | — | `inspector{width:"120 mm",height:"80 mm"}` |
| 2 | Typt `150` in Breedte, Return (of Tab/focusverlies) | `CommitField{doc,Width,"150"}` | `SetDimension{target,Width,150 mm,Min}` r1 → r2 | breedte 150; boom en canvas tonen dezelfde selectie; Undo-label "Ongedaan maken Maat" |
| 3 | Alternatief: `5,9 in` of `5.9in` | idem | 149,86 mm | — |

**Fouten:** `abc`, leeg, `NaN`, `1e999`, negatief of nul → veldfout (`MessageKey::InvalidLength`) ("Ongeldige maat"), geen command,
revisie onveranderd, veld blijft bewerkbaar met oude waarde als placeholder. Een maat zonder eenheid gebruikt de
documentweergave-eenheid (v1: mm). **Annuleren:** Esc in het veld herstelt de weergave; geen intentie.
**Platform:** NSTextField `controlTextDidEndEditing` en WinUI `LostFocus`/Enter leveren beide precies één
`SetDimension` (native test `native_dimension_commit_dispatches_one_document_command`, IM06).

## A4 — Undo en redo via menu (P02, X02)

| # | Gebruiker | Intentie | Kern | Viewmodel |
|---|---|---|---|---|
| 1 | Wijzig ▸ Ongedaan maken Maat, Cmd+Z / Ctrl+Z | `Invoke(Undo)` | `DocumentSession::undo` r2 → r3 (inhoud = r1) | breedte 120; zelfde ObjectId; Redo enabled |
| 2 | Shift+Cmd+Z / Ctrl+Y | `Invoke(Redo)` | r3 → r4 (inhoud = r2) | breedte 150 |
| 3 | Undo terwijl een drag loopt | `Invoke(Undo)` | eerst lopende drag annuleren, dan pas undo | preview weg, daarna undo |

- Eén undo-stack: AppKit-menu en Windows-menu lezen `commands[Undo]` uit het viewmodel; geen eigen
  `NSUndoManager`-historie. Undo/redo/heropenen maken nooit een `MachineIntent` (P02).
- Undo zonder historie: `Undo` disabled in viewmodel; een toch binnenkomende intentie is een no-op zonder fout.
- Na undo van "rechthoek maken" verdwijnt het object ook uit de selectie; redo selecteert het weer.

## A5 — Bewaren via native save-sheet (P01)

| # | Gebruiker | Intentie | Kern/taak | Viewmodel |
|---|---|---|---|---|
| 1 | Cmd+S op naamloos document | `Invoke(Save)` | — | `HostEffect::ShowSavePanel{doc,"Naamloos.burnburn"}` |
| 2 | Kiest map/naam, Bewaar | `Save{doc, Some(path)}` | `TaskRequest{TaskInput::Save{project r_n, path}}` | `pending_tasks=1`; UI blijft bedienbaar |
| 3 | Taak klaar | `task_finished(TaskOutput::Saved)` | `mark_saved(r_n)` | titel = bestandsnaam, `dirty=false` |
| 4 | Cmd+S op bewaard document | `Invoke(Save)` | direct `TaskRequest::Save` naar bekend pad | — |

**Annuleren:** sheet ▸ Annuleer → `DialogCancelled{SavePanel}` → geen taak, document blijft dirty.
**Fouten:** schrijffout/volle schijf/geen rechten → `TaskError::Save` → `HostEffect::ShowError` met pad en oorzaak; vorige
versie op schijf blijft leesbaar (IM04 `failed_replace_keeps_last_readable_project`); document blijft dirty.
Wijziging tijdens het bewaren: `Saved` voor r_n terwijl huidig r_n+1 → alleen r_n bewaard, `dirty=true`.

## A6 — Heropenen (P01)

| # | Gebruiker | Intentie | Kern/taak | Viewmodel |
|---|---|---|---|---|
| 1 | Archief ▸ Open, Cmd+O; kiest bestand in `NSOpenPanel` | `UiIntent::Open{path}` → `TaskInput::Open` (worker) | `TaskInput::Open` → `burnburn_io::load_project` → `Document` met dezelfde ObjectId's en maten | nieuw venster; titel = bestandsnaam; `dirty=false`; Undo disabled (historie wordt niet bewaard) |
| 2 | Opent reeds geopend bestand | idem | — | bestaand venster naar voren, geen tweede sessie |

**Fouten:** onleesbaar/beschadigd bestand → `HostEffect::ShowError`, geen venster; nieuwere schemaversie → expliciete melding
"gemaakt met nieuwere BurnBurn; niet-ondersteunde onderdelen blijven bewaard" en alleen-lezen-openen (P01, IM04).
Openen stuurt nooit machinecommando's.

## A7 — Gewijzigd document sluiten (P01, IM20)

| # | Gebruiker | Intentie | Effect |
|---|---|---|---|
| 1 | Cmd+W / rode knop / Ctrl+W met `dirty=true` | `Invoke(Close)` | `HostEffect::ConfirmClose` (Mac: sheet; Win: TaskDialog) |
| 2a | Bewaren | `ConfirmClose{Save}` | als A5; venster sluit pas na `Saved` |
| 2b | Niet bewaren | `ConfirmClose{Discard}` | `HostEffect::CloseWindow`; sessie weg; open taken geannuleerd |
| 2c | Annuleer | `ConfirmClose{Cancel}` | niets; sessie open (`closing_dirty_native_document_then_cancel_keeps_session_open`) |

Na sluiten: elke late `TaskOutcome` voor die sessie → verworpen zonder effect of viewmodelwijziging
(`closed_session_receives_no_callback`). Schoon document sluit zonder vraag.

## A8 — Achtergrondtaak en verouderd resultaat (X04-basis)

Een lichte `Measure`-taak (`Invoke(Measure)`: telling, totale bounds) loopt op een snapshot. Wijzigt de gebruiker tussendoor het
document, dan wordt het resultaat met de oude bronrevisie verworpen in `task_finished` en blijft het viewmodel de
nieuwere revisie tonen (`stale_task_result_does_not_change_newer_revision`). Een edit annuleert zo'n taak (best effort);
de revisiecheck blijft beslissend, ook als de worker de annulering mist (`complete_despite_cancel`). Een lopende
**save** wordt door een edit niet geannuleerd en registreert alleen de revisie die hij schreef
(`save_finishing_after_further_edit_leaves_document_dirty`). Hetzelfde contract geldt later voor trace, boolean en compile.

## A9 — Windows-proef (IM08, X07)

Zelfde stappen A1–A6 met Windows-equivalenten. Het scenario `windows_host_replays_dimension_scenario_with_same_document_result`
speelt de intenties van A2–A4 af en vergelijkt de documentinhoud met de headless uitkomst. Cross-compilatie telt niet
als bewijs; status blijft **onbevestigd** tot een Windows-runner bestaat.

## Headless poortscenario

`headless_gate_a_rectangle_resize_undo_save_reopen`: A1 → A2 (drag 0,0→120,80) → A3 (`150`) → A4 (Undo) → A5
(SaveAs naar tijdelijke map, taak uitvoeren) → A6 (open) en controleer: één object, zelfde ObjectId, 120 × 80 mm,
`dirty=false`, geen machine-intenties gegenereerd.
