# Coördinatie Implementatie (sketch) → andere rollen

Stand 18-10-2026, IM11. Log: `docs/dev-notes/IM11.md`.

## Besluit solver (beslisproef IM11) — aan Design

**Keuze: `sketch-solver` 0.1.0 (MIT OR Apache-2.0, enige dep `libm`) achter `burnburn_sketch::solver::Solver`.**
Geen eigen solverwerkpakket nodig op dit moment.

- Contracttests (`crates/burnburn-sketch/tests/solver_contract.rs`, generiek over `Solver`, zodat een eventuele eigen
  solver dezelfde suite draait): volledig bepaald (dof 0, exact), onderbepaald (dof 1 + vrije coördinaten),
  redundant (gemeld, toch opgelost), conflict (met de constraints waarmee hij botst), niet-convergent (onhaalbare
  driehoek: status ≠ Solved, residu > tolerantie; "conflict" wordt niet geëist), branch-flip (grote maatwijziging
  houdt de getekende tak, `flipped` leeg), punt-op-lijn, coïncident, loodrecht, diameter/radius, gelijk,
  concentrisch (= coïncidente middelpunten), symmetrie, tangentie lijn–boog, determinisme (twee runs identiek),
  annulering, getypeerde invoerfouten. Alle 13 groen.
- Montageplaat volledig bepaald; breedte 120 → 150 houdt gaten exact (≤ 1e-9 mm) symmetrisch; proptest over
  willekeurige haalbare platen: residu ≤ tolerantie en symmetrie binnen 1e-9·maat.
- **Onbevestigd:** bit-identiek op Windows x86_64 (geen runner). De crate claimt het (geen FMA, `libm`-trig);
  graag de determinismetest op de Windows-runner (Testing).
- Adapter-API = de in contracts §12 overgenomen vorm. `SolveError::Rejected { reason: String }` komt alleen nog voor
  bij onverwachte solverfouten (DR-12-opmerking): bekende oorzaken zijn getypeerd (`InvalidReference`,
  `InvalidGeometry`, `InvalidValue`, `Cancelled`).
- Synchrone evaluator: `CancellationToken::never()` + iteratiegrens van de adapter (100 per LM-run, 500 voor
  continuatie). Geen klok.
- Register: `sketch-solver` = gekozen (was kandidaat); `libm` 0.2.16 nu directe dep van sketch (DR-13). Graag
  `docs/design/dependency-register.md` bijwerken.

**Antwoord Design (fase C, 18-10-2026):** register bijgewerkt — `sketch-solver` =0.1.0 *in gebruik*, `libm` direct in
sketch *in gebruik*. Eén solver achter de trait blijft; de generieke contractsuite is precies wat een eventuele eigen
solver later moet halen.

## DR-12 uitgevoerd — aan Kern (graag review; kleine gerichte edits in jullie crates)

Sketch-**data** staat nu in `burnburn-document::sketch` (module-eigenaar Sketch), parser/solver/evaluator in
`burnburn-sketch`. Toegevoegd in Kern-bestanden (met eigen tests):

| Bestand | Wijziging | Test |
|---|---|---|
| `document/src/sketch/` (nieuw) | `SketchId`, `EntityId`, `ConstraintId`, `Entity`, `Relation`, `DimensionKind`, `Dimension { text, role }`, `ParameterTable` (naam → tekst), `SketchSolution` (laatst geldig), `SketchCommand`, `SketchLayer`, canonieke vorm | `document/tests/sketch_patch.rs` |
| `document/src/patch.rs` (nieuw, §11) | `DerivedOwner`, `DerivedKey`, `DerivedOutput`, `DocumentPatch { owner, inputs, outputs, status, solution }`, `EvalStatus::{Valid, Pending, Failed}`, `EvalDiagnosis`, `DerivedRecord`, `Evaluator` | idem |
| `command.rs` | `CommandPayload::Sketch`, `UndoLabel::Sketch`, `ChangeSet.sketches`, `CommandError::{Sketch, UnknownOwner, InvalidPatch}` | idem |
| `object.rs`/`canonical.rs` | `GeometrySource::Derived(DerivedKey)` (+ canonieke tag `derived`) | idem |
| `snapshot.rs` | veld `sketches: Arc<SketchLayer>`, `with_sketches`, `sketch`, `derived_record`, **`production_blocked(GeometryId)`** | idem |
| `session.rs` | `apply_with(cmd, &[&dyn Evaluator])`; `apply` = `apply_with(cmd, &[])`; evaluatie van gewijzigde sketches in dezelfde transactie; `DerivedKey → GeometryId` hergebruik; verdwenen keys verwijderd (incl. `process.remove_geometry`); dirty/undo dragen de sketchlaag mee | idem |
| `io/src/sketch_dto.rs`, `sketch_map.rs` (nieuw), `dto.rs`, `map.rs`, `hash.rs` | `Document.sketches` (additief, weggelaten als leeg → bestaande bestanden én hashes ongewijzigd), `Source::DerivedSketch`, projecthash krijgt `tag("sketches")` alleen als er sketches zijn | `io/tests/sketch_roundtrip.rs` |

Open punten / vragen aan Kern:
1. **Schemaversie.** Een bestand mét sketches wordt door een oudere build geweigerd door `deny_unknown_fields`
   (generieke parsefout, geen `NewerSchema`). Liever schema 2 zodra sketches erin staan? Jullie beslissen.
2. `map.rs` schrijft een toekomstige onbekende `DerivedOwner` nog als `Drawn` (wildcard door `#[non_exhaustive]`).
   Nu onbereikbaar (alleen `Sketch`); bij Trace/Text een fout i.p.v. stil terugvallen.
3. Niet gedaan uit §11: `CommandPayload::ApplyPatch` (asynchroon, Trace), `CommandError::StalePatch`,
   `CommandWarning::ReferenceBroken`, `CommandError::DerivedObject` (SetDimension/Transform op sketchuitvoer wordt nu
   nog uitgevoerd en bij de volgende evaluatie overschreven), `Sketch.placement` (frame = document-mm).
4. Een plain `apply` op een sketch zet de owner op `EvalStatus::Pending` (productie geblokkeerd tot een evaluator
   draait). Presentation moet dus voor sketchcommands `apply_with(&[&SketchEvaluator])` gebruiken.
5. Jullie lopende `CommandPayload::Edit`-werk in `session.rs` geeft nu een clippy-fout (`unnecessary_to_owned`,
   r. 294); dat blokkeert `clippy -D warnings` voor alles wat document gebruikt.

## Aan CAM

`DocumentSnapshot::production_blocked(id)` is waar voor afgeleide geometrie waarvan de owner niet `Valid` is
(`Failed`, `Pending`, nooit geëvalueerd). Graag in preflight als `Diagnostic::SketchNotSolved` (fout, blokkeert
export; preview mag). De geometrie zelf is dan de laatst geldige.

## Aan User-flow

- Commands: `CommandPayload::Sketch(SketchCommand::{Create, Delete, AddEntity, AddConstraint, RemoveConstraint,
  SetParameter, RemoveParameter, SetDimensionValue, SetDimensionRole})`, toepassen met
  `session.apply_with(cmd, &[&SketchEvaluator::new(SketchSolverAdapter)])` → één undo-entry voor definitie + geometrie.
  Tekst wordt bewaard zoals getypt, ook ongeldig; de fout staat in `derived_record(owner).status`.
- Voor het constraintpaneel: `burnburn_sketch::evaluate_sketch(snapshot.sketch(id), …)` → `Evaluation { health,
  dof, free, diagnostics: per ConstraintId (Satisfied/Redundant/Conflicting/Unmet/Measured/Invalid met ExprError-span),
  conflict_set() }`. Driven-maten: `Diagnosis::Measured(Value)` → label "(120)".
- `Drag { entity, to }` bestaat nog niet.

## Aan Testing

- Determinismetests op Windows draaien: `solver_contract::two_runs_are_bit_identical`,
  `sketch_evaluation::evaluation_is_deterministic` (vergelijk liefst ook een digest tussen Mac en Windows).
- Een end-to-end-test "sketch → save → reopen → opnieuw evalueren" kan niet in `burnburn-sketch` (io zit hoger,
  layers R3) en niet in io (sketch zit niet in de allow-lijst). Nu gedekt door twee helften:
  `io/tests/sketch_roundtrip.rs` (fake evaluator, bit-identieke uitvoer na heropenen) en
  `sketch_evaluation::reevaluating_a_stored_sketch_reproduces_bit_identical_outputs`. Voorstel: één scenario in
  `test-support`/`tests/scenarios` dat beide koppelt.
- Fuzzkandidaat: `burnburn_sketch::param::Expression::parse` (en `Parameters::from_definitions`).

## Antwoord Kern (18-10-2026)
- Review DR-12: akkoord met de edits; ik heb in dezelfde gedeelde bestanden alleen kleine aanvullingen gedaan
  (`CommandPayload::Edit`/`UndoLabel::Edit`/`CommandError::Edit`, één match-arm in `session.rs`, `ChangeSet.groups`,
  `GeometryObject::groups`, `DocumentSnapshot::group_members`, `ObjectKind`-varianten). Niets van jullie aangepast.
1. Schema: CAM heeft `SCHEMA_VERSION` al op 2 gezet; sketches (en mijn `kinds`/`groups`) horen bij dat nog niet
   uitgebrachte schema 2, dus een oudere build meldt `NewerSchema`. Een v2-fixture met sketch volgt best bij jullie.
2. Eens: bij een tweede `DerivedOwner` wordt de wildcard in `map.rs` een fout. Wie hem toevoegt, past dat aan.
3. `CommandError::DerivedObject`: voor mijn `Edit`-commands bestaat nu `EditError::DerivedGeometry(id)`.
   `SetDimension`/`Transform` op sketchuitvoer blokkeer ik niet zonder jullie akkoord (de sketch kan juist
   verplaatst moeten worden) — zeg het als het moet.
5. Opgelost (`session.rs` gebruikt geen `to_vec` meer); `clippy -D warnings` op document is groen.

**Antwoord Design (fase C, 18-10-2026)** op open punten 1–5 en Kern-punt 3:
1. Schema: zie C-3 — sketches horen bij het nog niet uitgebrachte schema 2 (Kern heeft dat al zo).
2. Bevestigd: wildcard in `map.rs` wordt een fout bij de tweede owner; Trace/Text/Generator voegen hun arm toe.
3. `CommandError::DerivedObject { owner }` voor `SetDimension`/`EditNodes`/`Transform` op afgeleide geometrie (C-8);
   verplaatsen van een sketch = `SketchCommand::SetPlacement { sketch, placement: Affine }` (veld `Sketch.placement`,
   frame = document-mm). `ApplyPatch`/`StalePatch`/`ReferenceBroken` levert Kern (C-9, IM21).
4. Bevestigd: presentation gebruikt altijd `apply_with` (User-flow doet dat al).
Scenario "sketch → save → reopen → evaluate" past nu in `burnburn-scenarios` (C-1).

## Van Kern (fase C, IM21, 18-10-2026)
- Tangentie/symmetrie **tussen objecten** bestaan nu als data + controle in `document::structure::relation`
  (`ObjectRelation::{Tangent, Symmetric}` over `ReferenceId`s → `GeometryRef`, `snapshot.relation_state(id)` met residu).
  Ze worden **niet afgedwongen**: het sketchcontract heeft geen externe entiteit. Voorstel: `Entity::External {
  reference: ReferenceId }` (alleen-lezen punt/lijn/cirkel uit `snapshot.resolve`) zodat jullie solver een relatie met
  andere objecten kan oplossen; de sketch-inputshash moet dan de opgeloste externe geometrie bevatten (anders geen
  herevaluatie bij bronwijziging). Zeg of jullie dit willen; Kern levert dan `Resolved` → `Entity`-hulpfunctie.
- C-8: `Transform`/`SetDimension`/`EditNodes` op sketchuitvoer geven nu `CommandError::DerivedObject { owner }`;
  `SketchCommand::SetPlacement` + `Sketch.placement` zijn aan jullie (DR-25). Eén regel in `sketch/command.rs`
  aangepast: `let DerivedOwner::Sketch(id) = owner else { return Err(SketchError::InvalidSolution) }` (nieuwe owners).

## Van Integratie-datamodel (18-10-2026): externe entiteit — nog niet ondersteund
`SketchCommand::AddExternal { sketch, target: GeometryRef }` bestaat en wordt **altijd** geweigerd met
`SketchError::ExternalNotSupported` (onbekende sketch eerst `UnknownSketch`); niets wordt vastgelegd (test
`document/tests/sketch_external.rs`). Tangentie/symmetrie tussen objecten blijft dus gecontroleerd (`relation_state`),
niet afgedwongen. Waarom niet in één stap — wat een echte implementatie vraagt:
1. Model: `Sketch.externals: BTreeMap<EntityId, GeometryRef>` (alleen-lezen punt/lijn/cirkel), canoniek + io-DTO.
2. Evaluator: `snapshot.resolve(target)` → `Resolved` → vaste solver-entiteiten (punten met `Fix`, straal vast) in
   sketch-frame (nu = document-mm; met `Sketch.placement` de inverse toepassen).
3. Inputs-hash moet de **opgeloste** externe geometrie bevatten, anders geen herevaluatie bij bronwijziging.
4. Sessie: een sketch met externals moet ook herevalueren als een gerefereerd object wijzigt — en *na*
   `evaluate_structure` (bron kan modifier-/part-uitvoer zijn): tweede evaluatieronde + cyclusdetectie
   (sketch → modifier → sketch). `Broken`/`Ambiguous` → `EvalStatus::Failed` + `ReferenceBroken`, nooit herbinden.
Voorstel: Sketch bouwt 1–3, Kern 4; de API-naam `AddExternal` kan blijven.
