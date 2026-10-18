# Poort B — montageplaat, logo, ringtrace en offline RD (IM09–IM15)

**Poortcriterium:** de montageplaat blijft correct bij maatwijziging; een vrij logo krijgt nette curves; een ringtrace
bewaart het gat; een JobPlan voor dezelfde documentrevisie levert inspecteerbare line/fill-RD. Offline export is nog
geen fysieke Ruida-job.

Notatie per stap: **handeling** → `intentie` → `command/taak` → *viewmodel*. Alle zware stappen (boolean, offset,
trace, solve, compile) zijn `TaskRequest`s met bronrevisie en cancellation; resultaat ouder dan de huidige revisie
wordt verworpen (contract A8). Elke "Toepassen" is precies één undo-entry (P02).

Scenario's (`crates/burnburn-presentation/tests/gate_b.rs`, groen 18-10-2026 headless macOS):
`headless_gate_b_mounting_plate_width_change` (B8), `headless_gate_b_ring_trace_keeps_hole` (B9),
`headless_gate_b_same_revision_preview_and_rd` (B10–B12). Interactie B1–B5: `tests/im09_interaction.rs`.
Nog gepland: `headless_gate_b_logo_node_repair` (import → knoopreparatie → geometriecontrole; wacht op import-UI IM10).
Intentnamen in deze tekst zijn ontwerpnamen; de geïmplementeerde vorm staat in `presentation-api.md`
(o.a. `SetSketchParameter`, `OpenTraceSource`/`SetTraceRecipe` + `ApplyTrace`, `AddOperation`, `Compile`, `ExportRd`).

## B1 — Montageplaat tekenen met exacte invoer en snapping (E01, E04)

1. Rechthoek-tool, klik op hoek, typ tijdens slepen `120` Tab `80` Return → `Pointer{Press}`, `ToolNumericInput{field,"120"}`…
   → `CreateRectangle` → *rechthoek 120 × 80, geselecteerd*. Zelfde canonieke geometrie bij `4.724in` of `100+20`.
2. Cirkel-tool, beweeg naar hoekpunt → *snapdoel "Eindpunt" benoemd naast cursor* (editor `SnapCandidate{kind,point}`);
   Tab doorloopt overlappende kandidaten; Cmd/Ctrl ingedrukt → *geen snap*.
3. Klik middelpunt, typ `Ø 4` → `CreateCircle` → *cirkel Ø 4 mm*. Lijn, ellips, boog idem (E01).
4. Selecteer gaten + referentie, Rangschik ▸ Uitlijnen links → `Invoke(AlignLeft)` → `AlignObjects{reference}` →
   *referentieobject gemarkeerd; afmetingen ongewijzigd* (E04). Spiegelen/verdelen idem.

Annuleren: Esc tijdens tekenen → geen object (E01). Fout: ongeldige getypte waarde → veldfout, tool blijft in preview.

## B2 — Het juiste object selecteren (E03)

1. Klik op overlappende lijnen → *bovenste geselecteerd*; nogmaals klikken op dezelfde plek → `Pointer{Press}` met
   zelfde positie binnen tolerantie → *volgende kandidaat* (cyclus); statusregel toont "2 van 3".
2. Selectiefilter (toolbar) "Alleen paden" → `SetSelectionFilter(Paths)` → hit-test negeert andere typen.
3. Selectie in objectboom → `SetSelection(ids)` → *canvas toont dezelfde selectie*; canvasselectie → *boom scrollt mee*.
4. Groep ▸ Isoleren / Vergrendelen → `DocumentCommand::SetLocked{ids}` (vergrendeling is documentdata; isoleren is
   vensterstate) → *vergrendelde objecten niet treffend, ook in geneste groepen*.

Fout: klikken op vergrendeld object → geen selectie, statusmelding "vergrendeld". Selectie is per venster.

## B3 — Logo lokaal corrigeren op knoopniveau (E02)

1. Dubbelklik pad / Node-tool (A) → `SelectTool(Node)` → *knopen met type-symbool (hoek/glad/symmetrisch)*.
2. Sleep handvat → `Pointer` Press/Move/Release → preview; Release → `EditPathSpan{path,span_range,…}` (één entry).
3. Knoop ▸ Glad → `SetNodeKind{Smooth}`; Split/Join/Reverse → eigen commands, selectie blijft op dezelfde plek.
4. *Inspecteur toont geslotenheid, knoopaantal, fysieke maat.*

Invariant: niet-geselecteerde curvebereiken blijven bitgelijk (`editing_one_bezier_span_preserves_other_spans`).
Annuleren: Esc tijdens handvatdrag → oorspronkelijke curve.

## B4 — Boolean, offset, trim, fillet met live preview (E05)

1. Selecteer twee vormen, Vorm ▸ Verenigen → `Invoke(BooleanUnion)` → `TaskRequest::Boolean{snapshot}` →
   *previewoverlay met resultaat; status "berekenen…" zonder UI-blokkade*.
2. Offset-paneel: afstand `2 mm`, richting, hoekstijl → elke wijziging nieuwe taak; oude resultaten verworpen.
3. Toepassen → `ApplyDerivedGeometry{source_rev, result}` → één undo-entry; bron behouden of verwijderd volgens keuze.

Fouten: onoplosbare offset (te grote inwaartse afstand) → *gerichte melding met plaats*, Toepassen disabled, document
onaangetast. Annuleren: paneel sluiten/Esc → preview weg, taak geannuleerd.

## B5 — Geometriefouten gericht repareren (E09)

1. Bewerken ▸ Geometrie controleren → `TaskRequest::GeometryCheck{tolerances}` → *lijst: open eindpunt 0,08 mm,
   duplicaat 12 mm, zelfdoorsnijding; klik zoomt naar locatie en selecteert betrokken objecten*.
2. Kies probleem ▸ Herstel met tolerantie `0,05 mm` → *voor/na-overlay* → Toepassen → `RepairGeometry{issue,tol}`.
3. Bedoelde sleuf 0,15 mm blijft open; alleen gekozen naad 0,05 mm sluit. Undo herstelt alle segmenten.

## B6 — Importeren met juiste afmetingen (P03)

1. Archief ▸ Importeer (Cmd+I / Ctrl+I) → native open-panel → `UiIntent::Import{document, path}` →
   `TaskRequest::ParseImport` → *importvoorbeeld met fysieke maat en eenheid*.
2. Eenheidloze DXF → *keuze mm/inch/cm met maatpreview "120 × 80 mm" vs "3048 × 2032 mm"*; Importeer disabled tot keuze
   (`unitless_dxf_needs_explicit_scale_before_import_commit`).
3. Importeer → `ImportObjects{report}` (één entry) → *importverslag: niet-ondersteunde entiteiten/effecten met
   bronverwijzing*.
4. PNG/JPEG → bronbeeld-asset met DPI-voorstel; ontbrekende DPI vraagt breedte (sluit aan op T01).

Annuleren: elk moment vóór Importeer → document ongewijzigd. Fouten: parserlimiet/beschadigd → melding, niets geïmporteerd.

## B7 — Geometrie exporteren (P05)

Archief ▸ Exporteer ▸ SVG/DXF → *vooraf verliesoverzicht: constraints, modifiers, tekst-als-tekst of contouren,
niet-ondersteunde DXF-curven* → save-sheet → `TaskRequest::Export{snapshot,format,options}`. Annuleren = niets
geschreven. Export wijzigt het document niet (geen revisie, niet dirty).

## B8 — Parametrische schets (S01–S04)

1. Schets ▸ Nieuwe schets → `CreateSketch` → *schetsmodus; vrije contouren blijven vrij*.
2. Tekenen: *voorgestelde relatie (horizontaal, coincident) als gestippeld symbool*; Shift+Cmd onderdrukt voorstel;
   alleen bij loslaten zonder onderdrukking wordt de relatie `AddConstraint` (S01). Geïmporteerde contour krijgt nooit
   automatisch constraints.
3. Parameterpaneel: `materiaaldikte = 3.1 mm`, `speling = 0.12 mm`, `sleufbreedte = materiaaldikte + speling` →
   `SetParameter{expr}` → `TaskRequest::Solve` → *effectieve waarden met eenheid*; cyclus/eenheidsfout → gerichte melding,
   geen wijziging. Eén parameterwijziging met afgeleiden = één undo-entry (S02).
4. Maat aanklikken ▸ "Meten" ↔ "Sturen" → `SetDimensionRole{Driven|Driving}` → *labels verschillen (bijv. "(120)" vs
   "120")*; omzetting die overbepaalt → conflict, laatste geldige geometrie zichtbaar (S03). Hulplijnen en maten nooit in
   laseroutput.
5. Breedte `150` → gaten blijven symmetrisch (`changing_plate_width_preserves_symmetric_hole_offsets`).
6. Conflict: *status "conflict" met symbool en tekst*; klik melding → `SetSelection(conflict_set)`; "Herstel laatste
   geldige" → undo-achtige terugkeer (S04).

## B9 — Bronbeeld op schaal en silhouettrace (T01, T02)

1. Afbeelding geselecteerd ▸ Traceer (Cmd+T / Ctrl+T) → *tracer naast canvas: bron, masker, vector, afwijking*.
2. Schaal: breedte `100 mm` of twee punten + bekende afstand → `SetImageScale` (bron-pixels onaangetast); *"0,1 mm/px
   bronresolutie" apart van "fittingtolerantie 0,02 mm"*.
3. Drempel/minimaal detail wijzigen → `TaskRequest::Trace{recipe, source_rev}`; laatste geldige preview blijft zichtbaar
   met "berekenen…"; verouderde resultaten verworpen.
4. Toepassen → `ApplyTrace{recipe, engine_version, result}` (één undo, bron + recept bewaard) → *contouren met gaten*.

Annuleren: tracer sluiten → document intact. Fout: geen voorgrond gevonden → melding, Toepassen disabled.
Scenario `traced_ring_preserves_hole_at_declared_physical_scale`.

## B10 — Proceslagen, meerdere bewerkingen, recept en volgorde (C01, C02, C06, C08)

1. Proceslagenpaneel: kleurknop klikken met selectie → `AssignOperation{layer, ids}` → *laag toont aantal objecten*.
2. Laag dubbelklikken → instellingen: modus Line/Fill, snelheid `mm/s` (expliciete eenheid), power → `SetOperationSettings`;
   zichtbaar en output-enabled zijn twee aparte toggles (C01). Beschikbare modi volgen het machineprofiel.
3. Zelfde cirkel tweede bewerking (markeren) geven → `AddOperationRef` → *analysepaneel: meervoudig gekoppeld (2)*;
   diameter wijzigen werkt in beide (C02, `two_operations_share_one_geometry_without_duplicate_source`).
4. Recept kiezen ▸ "Multiplex 3 mm – machine X rev 4" → `ApplyMaterialRecipe{snapshot}` → *herkomst per waarde*;
   bibliotheekupdate later wijzigt deze job niet (C08). Testkaart → aparte tijdelijke compilatie, bronproject schoon.
5. Volgorde: binnen vóór buiten automatisch; vergrendel startpunt/richting/groep → `LockOrder{…}`; cyclus → fout met
   betrokken operations (C06).

## B11 — Machineprofiel en capabilities (M01, M02)

1. Voorkeuren ▸ Machines (Cmd+, op Mac / Instellingen op Win) → *profiel: model, firmware (indien bekend), revisie,
   bedmaat, origin, verbinding — afzonderlijke secties*.
2. Origin wijzigen → `UpdateMachineProfile` (profieldata, niet het document) → *bestaand compile-resultaat "verouderd"*.
3. Capabilitymatrix → *per functie supported/unsupported/unknown met voorwaarde*; UI en compiler lezen dezelfde matrix.

## B12 — Preflight, kerf, planpreview en offline RD (C03, C07, C09, M03)

1. Voorbereiden-werkruimte ▸ Preview → `TaskRequest::Compile{source_rev, profile_rev}` → bevroren `JobPlan{hash}` →
   *preview: sneden, travels, scans, volgnummers, tijdschatting (gelabeld "schatting")*.
2. Klik beweging → `SelectPlanMove{move}` → *bronobject én operation geselecteerd* (C07).
3. Kerf `0,15 mm` → *binnen/buiten-zijde visueel, nominale maat blijft 120 mm, offset 0,075 mm per rand* (C03).
4. Preflight → *fouten (rood + symbool) gescheiden van waarschuwingen; klik zoomt naar oorzaak*: buiten bed, ongeldige
   operation, onbekende eenheid, dubbele snede, open fillcontour (C09, `outside_bed_operation_links_error_to_source_geometry`).
5. Exporteer RD → save-sheet → `TaskRequest::EncodeRd{plan_hash}` → *bestand geschreven; "Inspecteer RD" toont
   gedecodeerde geometrie/modes/power/bounds naast plan; onbekende records zichtbaar* (M03).

Invariant: preview en RD gebruiken dezelfde `plan_hash`. Editwijziging na compile → *plan "verouderd"*, export van het
oude plan blijft mogelijk maar toont revisieverschil. Geen machinecontact in deze poort.
