# Poort D — doos, klantlogo, handschrift (IM21–IM30) en werkplaats (IM31–IM32)

**Poortcriterium:** montageplaat, doos, klantlogo en handschrift halen de productdoelen uit het applicatieplan op een
benoemd corpus. Platformhergebruik loopt via commands, domein en renderer; Mac-schermen worden niet naar generieke
gedeelde widgets gekopieerd.

Algemene regels blijven: één undo per toepassen, revisiegebonden taken, machine-acties buiten de historie, eerlijke
capabilitystatus. Scenario's (domeinketen → RD → decode, groen in `crates/burnburn-scenarios/tests/gate_d.rs`):
`headless_gate_d_mounting_plate_variants_nested_on_two_plates_give_one_rd_per_plate`,
`headless_gate_d_box_thickness_change_no_double_kerf`, `headless_gate_d_customer_logo_repaired_union_with_fill_and_line`,
`headless_gate_d_handwriting_centerline_open_paths`. De presentatieroute (panelen D1–D12) en
`headless_gate_d_customer_logo_local_retrace` zijn gepland.

## D1 — Niet-destructieve modifierketen (E06)

Selectie ▸ Modifier toevoegen ▸ Offset/Boolean/Patroon → `AddModifier{source,kind,params}` → *modifierstapel in
inspecteur met per stap status*. Oogje uit → `SetModifierEnabled(false)`; bron blijft. Bronwijziging die een
resultaatreferentie breekt → *"gebroken referentie" op de stap, niet herkoppeld aan buursegment*
(`broken_reference_after_boolean_is_reported_without_rebinding_neighbour`). Slider slepen = één undo bij loslaten.

## D2 — Tangentie, symmetrie, gekoppelde referenties (S05, S06)

- S05: constraint Tangent/Symmetrisch/gekoppelde fillet; negatieve of niet-passende radius → veldfout; solver die naar
  een andere tak zou springen → *melding "oplossing wisselt" met keuze*, geen stille flip.
- S06: in schets ▸ Referentie gebruiken ▸ klik rand in ander onderdeel → `AddReference{source_part, entity}` →
  *label "uit Deksel ▸ rand 3"*; bron verwijderd/gesplitst → *gebroken koppeling zichtbaar*.

## D3 — Componenten, varianten en meerdere platen (S07, P06)

1. Selectie ▸ Maak onderdeel → `CreateComponent` → *definitie in bibliotheek, plaatsing op canvas*.
2. Plaatsing slepen/roteren → eigen transformatie; lokale override → *badge "override"*; bron blijft.
3. Varianten-tabel: rijen met parameterwaarden → `CreateVariant{values}`; één falende variant → *alleen die rij rood*.
4. Platen-paneel: Nieuwe plaat (maat, materiaal) → `CreateSheet`; onderdeel naar plaat slepen → `PlaceOnSheet`;
   compile gebruikt alleen gekozen plaat; ontbrekende bron → melding (P06).

## D4 — Generators, padtekst en personalisatie (E10, E08, E11)

- E10: Genereer ▸ Doos/Finger joint/Tab-slot/Scharnier → paneel met parameters (dikte, speling apart van kerf) →
  `CreateGenerator{kind,params}` → *parameters én resultaatgeometrie inspecteerbaar*; ongeldig aantal/te klein detail →
  concrete fout. Diktewijziging past sleuf aan, kerf niet dubbel (`material_thickness_changes_slot_but_does_not_apply_kerf_twice`).
- E08: tekst op pad → `BindTextToPath{offset, side}`; single-line font → *open paden, geen fill*; preview = plannergeometrie.
- E11: Gegevens ▸ Tabel koppelen (CSV) → *rijpreview met opgeloste velden, ontbrekende velden rood*; QR/barcode met
  modulegrootte-controle en leesproefstatus; bevroren job bewaart gekozen rijen en tijdwaarden.

## D5 — Centerline en technische reconstructie (T04, T08)

- T04: tracer ▸ doel "Centerline" → *graaf met eindpunten, kruisingen, ambigue kruisingen gemarkeerd*; open paden blijven
  open; kruising splitsen/verbinden → `ResolveJunction{choice}` (`t_junction_retains_three_branches_and_marks_ambiguous_connections`).
- T08: "Technische vormen" → *kandidaten (lijn/boog/cirkel) met bronoverlay en residu*; Accepteer per kandidaat →
  `ReplaceWithPrimitive`; maattekst/symmetrie alleen na expliciete keuze sturend.

## D6 — Foto opschonen en recepten herhalen (T03, T10)

- T03: globale vs lokale drempel apart; vierpuntsperspectief → *resulterende vlaktransformatie zichtbaar*; maskerkwast
  behoud/verwijder overschrijft bron niet.
- T10: A/B-varianten met gedeelde zoom en pad-/knooptelling; recept bewaren; batch → rapport per beeld; zelfde
  bron/recept/engine → zelfde geometriehash. Oude regiopatch wordt niet op nieuwe component toegepast
  (`old_region_patch_is_not_applied_to_new_unrelated_component`).

## D7 — Tabs/leads/passes en geavanceerde procesopties (C04, C11)

Operation-inspecteur ▸ Tabs: klik op contour plaatst tab → `AddTab{pos}` → *zichtbaar in plan*; bronwijziging maakt tab
ongeldig → melding. Multipass/Z, perforatie, overcut, scanhoek, air/delay: *alleen activeerbaar bij bewezen capability;
anders zichtbaar met reden*. Importeerde onbekende optie → gerichte melding (`unsupported_modulation_mode_is_rejected_instead_of_replaced`).

## D8 — Arrays en nesting (C10)

Array (grid/cirkel) en Nesting zijn aparte panelen/modellen. Nesting: plaat, marges, nerfrichting, rotaties, aantallen →
`TaskRequest::Nest` (annuleerbaar, X04) → *benutting "x % van plaatoppervlak" (noemer expliciet), niet-geplaatste
aantallen zichtbaar* → Toepassen = één undo (`nesting_keeps_order_counts_and_respects_grain_rotation`).

## D9 — Camera en markregistratie (W04, W05)

- W04: Camera-paneel → *bedfoto met tijdstip en calibratiestatus*; resolutiewijziging → *calibratie ongeldig*
  (`calibration_is_invalid_after_camera_resolution_changes`); handmatig plaatsen gelabeld "visuele plaatsing".
- W05: Registratie ▸ transformtype kiezen → *benodigde punten*; meet markpunten → `MachineIntent::MeasureMark` per punt →
  *residu in mm op controlepunten*; collineair/te weinig → geen geldig resultaat.

## D10 — Migratie, pakketten en zoeken (P04, P07, P08)

- P04: PDF-import met pagina/schaal/vector-raster-keuze; LBRN2: *lijst ondersteunde subset; rapport onbekende opties*
  (`import_reports_unknown_lbrn2_options_without_silent_drop`); projectimport en machineprofielimport aparte menu-items.
- P07: Archief ▸ Exporteer pakket → *overzicht ontbrekende/gelicentieerde assets*; importeren van pakket → *diff van
  ingebedde machine-instellingen vóór vervangen lokaal profiel*.
- P08: Zoeken (Cmd+F in projectbrowser) op naam/materiaal/onderdeel/job → *preview + revisiedatum*; oude productierevisie
  openen maakt geen nieuwe productie-intentie.

## D11 — Controllerbeheer en calibratie (M09, W09)

- M09: Machine ▸ Bestanden → *lijst; verwijderen vereist bevestiging en bewezen capability*; instellingen: huidige
  waarde / voorstel / backup; normale jobinstellingen raken vendorparameters niet.
- W09: Calibratie ▸ scanoffset/pointeroffset/kerf-proef met machine, materiaal, datum, methode; nieuwe calibratie →
  *afhankelijke plannen "verouderd" of expliciet oude snapshot* (`scan_calibration_update_invalidates_dependent_prepared_plan`).

## D12 — Grote ontwerpen, CLI en uitbreidingen (X04, X05, X06)

- X04: selectie op 100k-segmentfixture p95 ≤ 100 ms (benchmark, vaste machine); trace/nest/compile tonen voortgang en
  Annuleer → `CancellationToken`; document intact.
- X05: `burnburn compile project.burnburn --variant A` geeft dezelfde genormaliseerde planhash als GUI
  (`cli_and_gui_compile_same_revision_to_same_normalized_plan`); upload/start vereisen expliciete machine + intentie.
- X06: Uitbreidingen-paneel: *capabilities, API-versie, toegangsrechten*; pluginfout vervangt nooit een bevroren job.

## Na poort D — werkplaats (IM31, IM32)

- **M08** speciale machines/backends: profielwissel rotary ↔ vlakbed herstelt bedoeld profiel; eigen capabilitymatrix.
- **W06** herkenning: meerdere plausibele matches → beoordeling; score niet als mm-nauwkeurigheid getoond.
- **W07** restplaten en orders: orderaantal verandert alleen door geregistreerde uitvoeringsresultaten.
- **W08** compacte operatorweergave: machine, revisie, materiaal, bounds, aantallen samen; tweede scherm via één lokale
  machineservice; reconnect herhaalt geen opdracht (`reconnected_operator_view_does_not_repeat_last_machine_action`).
- **S08** AutoConstrain-voorstel op selectie: relaties + max. verplaatsing in mm; toepassen vereist gekozen voorstel.
- **T07** kleurregio's: palet en mapping expliciet; dubbele grenzen gemeld (`shared_colour_boundary_is_not_scheduled_twice_without_explicit_policy`).
- **C12** productienesting: complete sets, nerf, expliciete common-line-modus; score schendt geen verboden spiegelingen.
