# Contracten fase C/D — vooruitschets

Status: **schets (Design), 18 oktober 2026; fase C aangescherpt** (IM17, IM18, IM21, IM22, IM25, IM26, IM27, IM31, IM32 —
zie design-review "Fase C-besluiten"). Niet bindend voor veldnamen; wel voor *eigenaar, crate-plaats en de
regels*. Elk werkpakket begint met de genoemde eerste test (plan, `02-werkpakketten.md`) en past deze schets aan via
[`design-review.md`](../coordination/design-review.md). Lagen: [ADR 0001](../adr/0001-workspace-en-lagen.md) +
[`layers.toml`](layers.toml). Fase-B-contracten: [`contracts.md`](contracts.md) §1–§15.

Leidende principes die overal terugkomen:

1. **Bevroren invoer, één identiteit.** Alles wat de machine raakt, verwijst naar een `JobPlanId` + `rd_hash`
   (domeingescheiden, ADR 0004). Niemand compileert stil opnieuw.
2. **Bewijs, geen aanname.** Machinetoestand komt uit antwoorden (`Evidence`), nooit uit een upload-ACK of een timeout.
   Start wordt nooit automatisch herhaald.
3. **Evaluators leveren patches** (contracts §11); referenties breken zichtbaar, ze worden nooit stil herbonden.
4. **Capability-matrix** per machineprofiel: `Supported | Unsupported | Unverified { reason }`. `Unverified` gedraagt
   zich als `Unsupported` voor export/upload tot er bewijs is (AGENTS.md).
5. Determinisme: geen wandklok, geen `HashMap`-iteratie, geen threads in algoritmekernen; budget in iteraties.

---

## IM16 — Live transport (`burnburn-runtime::ruida_udp`)

```rust
/// Production ControllerPort (contracts §9). Owns one std::net::UdpSocket (non-blocking), no Tokio (ADR 0005).
pub struct RuidaUdpPort { /* socket, peer addr, codec state, outstanding packet, epoch */ }
impl RuidaUdpPort {
    pub fn connect(cfg: UdpConfig, identity_probe: bool) -> Result<Self, PortError>;   // bind lokaal, geen broadcast-scan
}
pub struct UdpConfig { pub peer: SocketAddr, pub local_port: u16, pub packet_timeout: Duration, pub max_payload: usize }
impl ControllerPort for RuidaUdpPort { /* send / try_recv / reconnect */ }
```

Regels:
- **Framing in de codec, sockets in runtime.** `burnburn_ruida::udp` (bestaat) doet checksum, scrambling en
  pakket-ACK/NAK-herkenning als pure functies; `RuidaUdpPort` doet alleen IO + timers via de geïnjecteerde `Clock`.
- **Stop-and-wait per pakket**: één uitstaand pakket; een pakket-NAK of -timeout mag dat *zelfde pakket* begrensd
  opnieuw sturen (`Policy.max_packet_retries`), behalve als het pakket een `Start` bevat: dan gaat de execution naar
  `Unknown` (contract §9) en volgt alleen een statusquery.
- `Reply::Ack` naar de actor betekent "pakket ontvangen", nooit "opgeslagen" of "gestart".
- **Machine-identiteit** vóór elke upload: een identiteitsquery (model/firmware/serie waar de controller die levert)
  wordt vergeleken met het gekozen profiel; mismatch → `ActorError::IdentityMismatch`, geen upload. Antwoorden van een
  ander IP/poort dan de peer worden genegeerd en gelogd.
- **Laser-uit-framing:** `MachineAction::Frame { plan: JobPlanId }` gebruikt een apart `FramePlan`: alleen
  `Travel`-events langs de bounds, één settingsblok met vermogen 0. *Review fase C:* gebouwd in `burnburn_machine::frame`
  (Protocol) i.p.v. `cam::frame_plan` — overgenomen: het is een pure afleiding van een bevroren `JobPlan`, cam is niet nodig;
  `FramePlan.source()` noemt het productieplan. `ruida::lower_frame(&FramePlan, &RuidaProfile)`
  weigert elk event met vermogen > 0 en emitteert geen laser-aan-commando. Test (met de onafhankelijke Python-decoder als
  oracle): `frame_program_contains_no_laser_on_command`, en `frame_is_rejected_when_profile_lacks_verified_frame`.
- Live UDP-tests staan achter een expliciete omgevingsvariabele + machine-ID (`BURNBURN_HW_MACHINE=…`) en draaien
  nooit standaard; resultaten per model/firmware/interface in `docs/hardware/evidence.md` (nieuw, eigenaar Protocol).

## IM17 — CompiledJob, Execution, queue en historie (`burnburn-history`, ADR 0008)

*Aangescherpt fase C (18-10-2026), na lezen van `burnburn-machine` (`RdArtifact`, `Execution { plan, rd_hash }`,
`Evidence`, `ExecutionState` met `UnknownReason`). Bindend: journalformaat, recordsoorten, volgorde-eisen. Veldnamen
mogen wijken als de regels blijven.*

```rust
// burnburn-history
pub struct CompiledJobId(ContentHash);   // ContentHash::of(CompiledJob, canon(plan_id ‖ rd_hash ‖ profile.content))
pub struct CompiledJob {
    pub id: CompiledJobId,
    pub plan: JobPlanId, pub rd_hash: ContentHash, pub profile: ProfileRevision,   // ProfileRevision incl. content-hash (DR-18)
    pub project_hash: ContentHash,               // io::project_hash van de gecompileerde snapshot (C-6: niet in het plan)
    pub project_path: Option<PathBuf>,           // alleen weergave/zoeken (IM28), geen identiteit
    pub preflight: PreflightSummary,             // aantallen per severity + codes; nooit een Error (anders geen CompiledJob)
    pub blobs: CompiledBlobs,                    // { plan: BlobKey, rd: BlobKey, project: BlobKey } — Asset-domein
    pub created: WallTime,                       // alleen weergave
}
pub struct ActionId(u128);                       // per klik (IM31); idempotentiesleutel, ook nu al in records
#[non_exhaustive]
pub enum HistoryRecord {
    JobCompiled(CompiledJob),                                           // blobs zijn vóór dit record durable
    Queued { item: QueueItemId, job: CompiledJobId, quantity: u32 },
    QueueChanged { item: QueueItemId, quantity: u32 },                 // nooit onder `completed`
    ExecutionCreated { execution: ExecutionId, job: CompiledJobId, item: Option<QueueItemId>, machine: MachineId },
    IntentRecorded { execution: ExecutionId, action: ActionId, intent: IntentKind },   // VÓÓR de driver iets krijgt
    Evidence { execution: ExecutionId, evidence: EvidenceDto },        // 1-op-1 uit machine::Evidence
    StateObserved { execution: ExecutionId, state: ExecutionStateDto },// na elke actor-overgang
    Reconciled { execution: ExecutionId, from: ExecutionStateDto, to: ExecutionStateDto, by: ReconcileBasis, action: ActionId },
    OperatorNote { execution: ExecutionId, text: String },
}
pub enum IntentKind { Upload, ConfirmStored, Start, Stop, QueryStatus, Frame, WriteSettings }
pub enum ReconcileBasis { StatusQuery { evidence_seq: RecordSeq }, OperatorConfirmed { role: Role, statement: ReconcileStatement } }
pub enum ReconcileStatement { RanToCompletion, DidNotRun, Partial }   // operator zegt wat hij zag; Partial telt niet mee
pub struct RecordSeq(u64);                                            // globaal, strikt stijgend over segmenten

pub struct History { /* in-memory index; geen open filehandles (alles via FileSystem) */ }
impl History {
    pub fn open(fs: &dyn FileSystem, dir: &Path, limits: &JournalLimits) -> Result<(History, RecoveryReport), HistoryError>;
    pub fn append(&mut self, fs: &dyn FileSystem, record: HistoryRecord, at: WallTime) -> Result<RecordSeq, HistoryError>;
    pub fn put_blob(&mut self, fs: &dyn FileSystem, bytes: &[u8]) -> Result<BlobKey, HistoryError>;   // atomair, idempotent
    pub fn read_blob(&self, fs: &dyn FileSystem, key: BlobKey) -> Result<Vec<u8>, HistoryError>;     // hash wordt herberekend
    pub fn next_execution_id(&self) -> ExecutionId;           // max(gezien)+1, ook over herstarts
    pub fn action_outcome(&self, action: ActionId) -> Option<&ActionOutcome>;   // IM31-idempotentie
    pub fn job(&self, id: CompiledJobId) -> Option<&CompiledJob>;
    pub fn executions(&self) -> impl Iterator<Item = &ExecutionView>;
    pub fn read_only(&self) -> bool;                          // true na corruptie midden in een segment
}
// burnburn-presentation (zelfde laag → eigen leesmodellen, géén history-types in de poort)
pub trait HistoryStore: Send + Sync {
    fn jobs(&self) -> Vec<JobSummary>; fn executions(&self, filter: ExecutionFilter) -> Vec<ExecutionSummary>;
    fn record_intent(&self, req: IntentRequest) -> Result<IntentReceipt, HistoryStoreError>;   // terug pas na sync
}
```

**Journalformaat (bindend, ADR 0008 amendement 1):**

| Onderdeel | Vorm |
|---|---|
| Map | `<app-data>/history/journal-<n:08>.bbj`, `<app-data>/history/blobs/sha256-<hex>` |
| Segmentkop | 8 bytes `b"BBJ\0" ‖ u16 LE formatversie (=1) ‖ u16 LE 0`; daarna records. Een segment zonder geldige kop = corrupt. |
| Record | `u32 LE magic 0xB0B0_4A31 ‖ u32 LE len ‖ u32 LE crc32(payload) ‖ payload[len]`; `len ≤ JournalLimits.max_record` (default 1 MiB) |
| Payload | UTF-8 JSON, één object, velden in vaste structvolgorde (geen `HashMap`), `f64` met `float_roundtrip`: `{"schema":1,"seq":…,"wall_time":"…","kind":"…","body":{…}}` |
| Schrijven | `FileSystem::append(segment, header ‖ payload)` → `sync_file(segment)`; pas daarna geeft `append` `Ok(seq)`. Nieuwe segmentfile: `create_temp_in` + kop + sync + `replace` + `sync_dir`, dán het eerste record. |
| Rotatie | na een record waardoor het segment ≥ 8 MiB is; geroteerde segmenten worden nooit meer beschreven. |
| Herstel | sequentieel lezen; korte kop/payload of foute CRC **in de staart van het laatste segment** → `RecoveryReport::TornTail { segment, offset, bytes }`, de volgende append begint in een **nieuw** segment (de gescheurde staart blijft staan als bewijs, nooit truncaten); dezelfde fout elders, een niet-stijgende `seq`, of onbekende `schema` → historie opent `read_only` + `RecoveryReport::Corrupt`. Onbekende `kind` binnen bekend schema → bewaard als `Unrecognized` (doorgegeven bij compactie), telt niet mee. |
| Blobs | `put_blob` schrijft via het atomaire io-pad (`io::write_file`, C-7/userflow-verzoek); sleutel = `ContentHash::of(Asset, bytes)`; `read_blob` herberekent en weigert bij mismatch. |

Regels:
- **Volgorde-eisen** (elk een test): blobs → `JobCompiled`; `ExecutionCreated` → `IntentRecorded` → *dan pas* de
  `MachineIntent` naar de driver (`start_intent_is_durable_before_driver_receives_it`, met een `MemoryFileSystem`
  die de sync laat falen: dan **geen** send). Evidence/`StateObserved` worden ná de waarneming geschreven; een crash
  ertussen geeft na herstart `Unknown`, nooit een aanname.
- `machine`-types blijven serde-vrij: history bezit `EvidenceDto`/`ExecutionStateDto` + mapping met een
  **uitputtende** `match` (een nieuwe `Evidence`-variant breekt history-compilatie, niet stil `Other`). `Evidence.at`
  (`MonotonicTime`) gaat niet naar schijf; het record draagt `wall_time` (weergave) en `seq` (ordening).
- `editing_project_does_not_modify_prepared_job_revision`: `CompiledJob` bevat blobs van plan, RD en project; het
  document kent hem niet; undo raakt hem niet.
- "Opnieuw uitvoeren" = nieuwe `Execution` op dezelfde `CompiledJobId` met de RD-blob (`RdArtifact::from_rd_file(plan,
  blob)`, hash moet gelijk zijn aan `rd_hash`), nooit hercompileren. Is het profiel intussen gewijzigd (`content` ≠
  huidig), dan blokkeert presentation met uitleg (IM29).
- Aantallen: `completed` stijgt alleen bij `StateObserved(Completed)` dat uit actor-bewijs volgt, of bij
  `Reconciled { to: Completed, by: OperatorConfirmed { RanToCompletion } }`. `Unknown`/`Partial` tellen niet mee.
- Na herstart: elke niet-terminale execution → `Unknown` (`restart_marks_running_execution_unknown_without_resending_start`);
  de driver doet hooguit één statusquery; geen `Start`, geen upload.
- Fuzzdoel: `History::open` over willekeurige segmentbytes (Testing), plus property-test "elke prefix van een geldig
  journal opent zonder verlies van volledige records".

## IM18 — Tekst en rasterproductie

*Fase C: data verhuist naar `document::text` (zelfde reden als DR-12: het moet in snapshot, undo en bestand). Eigenaar
Generators/tekst (crate `burnburn-text` + module `document::text`).*

```rust
// burnburn-document::text (data; eigenaar Generators/tekst)
pub struct TextId(u64);                                  // DerivedOwner::Text(TextId)
pub struct TextObject { pub content: String, pub font: FontRef, pub fallback: Vec<FontRef>, pub size: Length,
                        pub layout: TextLayout /* Line | Box{w} | OnPath{path: GeometryRef, offset, side} */,
                        pub style: TextStyle /* Outline | SingleLine */, pub placement: Affine }
pub struct FontRef { pub family: String, pub style: FontStyle, pub asset: Option<ContentHash> } // asset = ingebed font
// CommandPayload::Text(TextCommand::{Create, SetContent, SetFont, SetLayout, SetPlacement, Delete, EmbedFont})

// burnburn-text (laag 3; harfrust + skrifa achter `text::shape`)
pub trait FontSource { fn load(&self, r: &FontRef) -> Result<Arc<FontData>, FontError>; }   // project-assets eerst
pub struct TextEvaluator<F: FontSource> { /* implements document::Evaluator; owner_kind = Text */ }
pub fn shape(obj: &TextObject, fonts: &dyn FontSource) -> Result<ShapedText, TextError>;
pub struct ShapedText { pub runs: Vec<GlyphRun>, pub outlines: Vec<(DerivedKey, Path)>, pub used_fonts: Vec<ContentHash>,
                        pub missing: Vec<char> }
// burnburn-presentation: FontCatalogPort (opsommen). burnburn-runtime: SystemFontSource (std-fontmappen, pure Rust).
```

Regels:
- Tekst is een evaluator (§11): `DerivedKey.local` = index van de glyph-run (regel × run), niet de glyph-index, zodat
  een tekstwijziging verderop operations op eerdere regels niet breekt. Inputs-hash: `EvalText` over tekst, layout,
  font-bytes-hashes en engineversie.
- Ontbrekend font → `EvalStatus::Failed { FontMissing }`, last-valid outlines blijven, preflight `StaleDerived`; nooit
  een stille OS-fallback; de fallbackketen staat in het document.
- Heropenen op een andere machine geeft dezelfde outlines als de fonts als asset zijn ingebed (licentiewaarschuwing in
  de UI bij inbedden; juridische beslissing: open-beslissingen B-6).
- `dither_runs_are_regenerated_from_source_after_physical_resize`: dither is nooit documentdata; `cam` resamplet bij
  compile vanaf de originele pixels op de fysieke maat en pitch. Cache op inputs-hash. Dither = integer-foutdiffusie
  (i32); Mac en Windows geven bit-gelijke `BitImage`s (property-test op beide).
- Geen grayscale-modulatie vóór de semantiekproef (`Feature::GrayscaleModulation` blijft `Unverified`, §IM25).

```rust
// burnburn-image::raster (pure, deterministic integer arithmetic)
pub struct ToneCurve { pub gamma: f64, pub brightness: f64, pub contrast: f64 }   // eindig, begrensd; LUT via libm::pow
#[non_exhaustive] pub enum DitherAlgorithm { Threshold, FloydSteinberg, Jarvis, Stucki, Atkinson, Ordered { size: u8 } }
pub fn resample_area(src: &RasterImage, crop: PixelRect, out_w: u32, out_h: u32) -> GrayImage;  // vaste-punt
pub fn dither(img: &GrayImage, tone: &ToneCurve, algo: DitherAlgorithm, serpentine: bool) -> BitImage;
```

## IM19 — Lokale tracecorrectie (`burnburn-trace::correct`)

```rust
pub struct RegionMask { pub polygon: Vec<[f64; 2]> /* pixelcoördinaten */ }
pub struct CorrectionRequest { pub previous: Arc<TraceResult>, pub region: RegionMask,
                               pub recipe: TraceRecipe /* lokale override */, pub anchors: Vec<Anchor>,
                               pub protected: BTreeSet<ContourId> }
pub fn retrace_region(req: &CorrectionRequest, image: &RasterImage, cancel: &CancellationToken)
    -> Result<TraceResult, TraceError>;
pub struct DeviationMap { /* per contoursegment: max afwijking px; raster voor overlay */ }
```

Regels: contouren buiten de regio behouden `ContourId` *en* segmenten bit-gelijk
(`retrace_region_preserves_protected_hole_and_untouched_segments`); beschermde contouren veranderen nooit, ook niet
binnen de regio; een contour die de regiogrens kruist wordt alleen binnen de regio opnieuw gefit met vaste
aansluitpunten op de grens. Nieuwe contouren krijgen nieuwe ID's (hoger dan alle bestaande, nooit hergebruik).
`TraceResult.inputs` neemt de correctieketen op, zodat een oude regiocorrectie niet op een nieuw component landt (IM24).

## IM20 — Native afwerking en release

- **Toegankelijkheid:** `AppViewModel` krijgt per element `AccessibleRole` + `MessageKey`-label; het canvas levert een
  `AccessibilityTree` (objecten als elementen met bounds in punten) uit `presentation`. Hosts vertalen naar
  `NSAccessibility` resp. UI Automation — getest per host.
- **Lokalisatie:** `MessageKey` + catalogi `resources/locales/{nl,en}.ftl` (Fluent, `fluent-bundle` kandidaat) geladen in
  presentation; hosts tonen alleen strings. Ontbrekende sleutel = testfout (`all_message_keys_have_nl_and_en`).
- **IME:** compositie (marked text) leeft in de host/het veld; pas `insertText` (commit) wordt een `UiIntent`.
- **Recovery:** autosave schrijft `recovery/<uuid>.burnburn` via io, nooit over het projectbestand; openen na crash biedt
  herstel aan, verandert de Save-betekenis niet.
- **Bundle/installatie:** `cargo xtask bundle-macos` (Info.plist met `LSMinimumSystemVersion 13.0`, ADR 0002) en later
  `bundle-windows`. Update-contract: alleen een *controle* op een releasemanifest (opt-in), geen zelfinstallatie in v1.

## IM21 — Modifiers, componenten en associatieve referenties

*Aangescherpt fase C (18-10-2026). Eigenaar Kern. Data én modifier-evaluatie in `burnburn-document::structure`
(geen crate, C-1): `modifier`, `part`, `plate`, `reference`, `relation`. Bindend: het referentiemodel hieronder.*

### Referentiemodel (topologische naamgeving via herkomst)

*Review DR-20 (18-10, tijdens fase C): Kern bouwde `document::structure::reference` — `EdgeOrigin { source, edge:
SegmentRef, copy }`, `EdgeName { origin, index, of }`, `SubRef::{Edge, Vertex { edge, end }}`, `ProvenanceEdge`-runs
(opeenvolgende segmenten met dezelfde origin = één edge; `origin: None` = gegenereerd, bv. een join-boog),
`RefStatus::{Bound(Resolved), Ambiguous, Broken}`, `RefHolder`. Dat is eenvoudiger dan Design's schets (geen geneste
stap-origins, vertex = eind van een benoemde edge) en wordt **leidend**. Eén aanvulling blijft bindend: zie `EdgeKey`.*

```rust
// burnburn-document::structure::reference (zoals gebouwd; ingekort)
pub struct GeometryRef { pub object: GeometryId, pub sub: Option<SubRef> }
pub enum SubRef { Edge(EdgeName), Vertex { edge: EdgeName, end: VertexEnd } }
pub struct EdgeOrigin { pub source: GeometryId, pub edge: SegmentRef /* → stabiele EdgeKey, DR-20 */, pub copy: u32 }
pub struct EdgeName { pub origin: EdgeOrigin, pub index: u32, pub of: u32 }   // `of` ≠ huidig aantal → Ambiguous
pub enum RefStatus { Bound(Resolved), Ambiguous { candidates: Vec<EdgeName> }, Broken { reason: BreakReason } }
```

**`EdgeKey` (DR-20, bindend):** `EdgeOrigin.edge` mag geen *positie* in het bronobject zijn zolang dat bronobject
bewerkbaar blijft (E06: de bron van een modifier blijft gewone geometrie). Een `InsertNode` in segment 1 verschuift
anders de index van segment 3 en de referentie wijst stil naar de buur. Daarom krijgt elk getekend object per
segment een stabiele `EdgeKey` (per-object teller), onderhouden door de edit-commands volgens de tabel hieronder;
`identity_origins` gebruikt die sleutel i.p.v. de index. Test: `inserting_node_in_source_does_not_rebind_reference_to_later_edge`.

| Bewerking op getekende geometrie | `EdgeKey`-gevolg |
|---|---|
| `MoveNode`, `MoveHandle`, `Transform`, `ReverseSubpath`, `ConvertToCubic` | sleutels blijven (convert: alle deelsegmenten dezelfde sleutel; ze vormen één run) |
| `InsertNode` (splitsen) | beide helften erven de sleutel → één edge-run met twee segmenten blijft `Bound` (de run is de edge); pas als een boolean/offset de run in stukken knipt wordt `of` > 1 → `Ambiguous` |
| `DeleteNode` (twee segmenten → één) | nieuwe sleutel; refs naar de verdwenen sleutels → `Broken(OriginVanished)` |
| `Boolean`/`Offset` als *destructieve* edit (§3) | nieuw object, nieuwe sleutels; refs naar de bronnen → `Broken(ObjectDeleted)` |

Regels:
- **Herkomst wordt bij evaluatie berekend, niet geraden.** Elke evaluator levert per uitvoersegment een
  `Option<EdgeOrigin>` (opgeslagen als `ProvenanceEdge`-runs bij de afgeleide uitvoer). Offset/mirror/array kennen de herkomst per constructie.
  Boolean (i_overlay levert geen herkomst): een uitvoersegment krijgt de origin van het invoersegment waar het
  **op ligt** (collineair/co-circulair binnen `ProductionTolerance`, overlap > 0); ligt het op twee invoersegmenten
  (samenvallende randen), dan krijgt het beide als kandidaten → een referentie erop is `Ambiguous`. Dit is
  deterministische containment, geen "dichtstbijzijnde buur".
- Een referentie wordt na elke evaluatie in dezelfde transactie opnieuw opgelost. `Broken` → `CommandWarning::ReferenceBroken
  { holder, reference }` + preflight-`Error ReferenceBroken` voor operations; `Ambiguous` → `CommandWarning::ReferenceAmbiguous`
  en de gebruiker kiest (`RebindReference { holder, to }` = eigen command, één undo). **Nooit automatisch herbinden**
  (`broken_reference_after_boolean_is_reported_without_rebinding_neighbour`).
- Opslag: alleen `GeometryRef` (met `EdgeName`) wordt bewaard; `RefStatus` is afgeleid en wordt niet bewaard.
  `EdgeKey`s van getekende objecten zijn een optioneel staartveld `edge-keys` (C-4); ontbreken ze in een bestaand
  bestand, dan krijgen segmenten bij laden sleutels 0..n (deterministisch).
- `OperationRef` → `GeometryRef` (DR-15 voorbereid: io-DTO `[{ "geometry": 7 }]` krijgt optioneel `"sub"`); een operation
  op een *edge* (bv. alleen de bovenrand graveren) is pas in IM25 nodig; tot dan accepteert CAM alleen `sub: None`
  (preflight-`Error UnsupportedReference` anders).

### Modifiers

*Zoals gebouwd (`structure::modifier`, Kern) — leidend:* `ModifierChain { name, source: GeometryId, steps:
Vec<ModifierStep { modifier, enabled }>, tolerance: ProductionTolerance, mode: EvalMode::{Inline, Background} }`,
`Modifier::{Offset, Boolean { op, with: Vec<GeometryId> }, LinearArray { count, step }}` (`#[non_exhaustive]`; Mirror,
PolarArray, Fillet later), inputs-hash `EvalModifier` over versie + keten + bronpaden. `EvalMode::Background` (zware
booleans als taak + `ApplyPatch`/`StalePatch`) is een goede aanvulling op Design's schets en overgenomen.

- Uitvoer = afgeleide objecten (`DerivedOwner::Modifier(id)`); de bron blijft een gewoon, bewerkbaar object. Een modifier op de uitvoer van een andere modifier mag; cycli → `CommandError::ReferenceCycle`.
- Volgorde van evaluatie in `apply_with`: topologisch op afhankelijkheid (sketch → modifier → generator), deterministisch
  bij gelijke rang (op owner-id).

### Componenten en platen

*Zoals gebouwd (`structure::{part, plate}`, Kern) — leidend:* `PartDefinition { name, parameters: BTreeMap<String,
Length>, shapes: BTreeMap<u32, NamedShape>, variants }`, `PartInstance { definition, variant, overrides, transform,
plate: Option<PlateId> }` (instance wijst naar plaat, niet omgekeerd), `Plate { name, width, height, material,
thickness }` (rechthoekige plaat; restplaat-outline volgt met IM26 `Stock.outline`), `DerivedOwner::Part(PartInstanceId)`
(niet `Instance`). `structure::relation` (`ObjectRelation`, `RelationState`) voor relaties tussen objecten.

- Een instance-uitvoer is afgeleid (niet direct bewerkbaar, C-8); bewerken = definitie of "Losmaken".
- Elke plaat compileert naar een eigen `JobPlan`: `CompileRequest.scope: CompileScope::{Document, Plate(PlateId)}`; de
  scope zit in `PlanSource` (en dus in `JobPlanId`).
- Een variant wijzigt alleen parameters; een andere *topologie* per variant (gat erbij) is toegestaan, refs naar
  verdwenen edges worden zoals hierboven `Broken`.

## IM22 — Lasergenerators (`burnburn-generators`, data in `document::generator`)

*Nieuw fase C (C-1). Eigenaar Generators/tekst.*

```rust
// burnburn-document::generator (data)
pub struct GeneratorId(u64);                              // DerivedOwner::Generator(GeneratorId)
pub struct GeneratorInstance { pub id: GeneratorId, pub spec: GeneratorSpec, pub placement: Affine,
                               pub material: Option<MaterialKey> }          // dikte uit het materiaal of expliciet
#[non_exhaustive]
pub enum GeneratorSpec {
    FingerJoint { edge: GeometryRef /* IM21 */, thickness: Length, finger: Length, play: Length },
    TabSlot { .. }, BoxMaker { inner: [Length; 3], thickness: Length, joint: JointSpec, lid: LidKind },
    LivingHinge { region: GeometryRef, pattern: HingePattern, pitch: Length },
    StencilBridges { targets: Vec<GeometryRef>, width: Length, min_island: Length },
    PathText { text: TextId, path: GeometryRef },            // gebruikt text::shape
    DataRows { source: AssetRef /* CSV, bevroren */, template: TemplateRef, rows: RowRange },
    Barcode { symbology: Symbology /* Code128 | Qr{ecc} | DataMatrix */, data: String, module: Length },
}
// burnburn-generators
pub struct GeneratorEvaluator<F: FontSource> { /* implements document::Evaluator; owner_kind = Generator */ }
pub fn generate(spec: &GeneratorSpec, ctx: &GenContext, cancel: &CancellationToken) -> Result<GenOutput, GenError>;
pub struct GenOutput { pub outputs: Vec<DerivedOutput /* §11, met edge-origins §IM21 */>, pub notes: Vec<GenNote> }
```

Regels:
- **Nominale geometrie, geen kerf.** Generators tekenen exacte maat; `play` (speling) is een expliciete parameter.
  Kerf wordt alleen in `cam` toegepast (`material_thickness_changes_slot_but_does_not_apply_kerf_twice`): de test
  wijzigt de dikte, compileert, en toont dat de slotbreedte = dikte + play − kerf-correctie precies één keer.
- **Bevroren data:** `DataRows` leest een CSV-asset (bytes in het project), nooit een live bestand; elke rij → een eigen
  `DerivedKey` (rijnummer), zodat een operation op rij 3 blijft wijzen als rij 7 verandert.
- Barcodes: zelf geïmplementeerd of pure-Rust crate (register: kandidaat); testoracle = onafhankelijke decoder of
  gepubliceerde testvectoren, niet de eigen encoder.
- Generators produceren gesloten, niet-zelfdoorsnijdende contouren of melden `GenNote::Invalid` (geen stille reparatie).
- Single-line tekst (`TextStyle::SingleLine`) levert open paden; CAM behandelt ze als `Line`, `Fill` weigert ze
  (`OpenPathInFill`).

## IM25 — Procesuitbreidingen en capability-matrix (`document::process`, `job::capability`, `cam`, `ruida::lower`)

*Aangescherpt fase C na lezen van `burnburn-job/src/capability.rs` (CAM): `Feature` (12, `#[non_exhaustive]`),
`Support::{Supported, Unsupported, Unverified { reason }}`, `MachineCapabilities::conservative()` + `with`. Leidend; de
regels hieronder vullen aan.*

- `OperationKind` groeit: `OffsetFill { step, direction }`, `Fill { angle }`, `Line { passes, z_step, lead_in, lead_out,
  tabs: TabSpec, overcut, perforation: Option<Perforation { on, off }> }`, subbewerkingen als `Operation.children`.
- **Eén afleiding per instelling:** `fn required_features(&Operation) -> BTreeSet<Feature>` in `cam` (puur, getest per
  veld: een instelling op default vraagt geen feature). Preflight: elke vereiste feature die niet `Supported` is →
  `Error UnsupportedFeature { feature, support }` met de operation als `SourceRef`; `Unverified` toont de `reason`.
- **Twee betekenissen gescheiden:** de matrix zegt of *de compiler+backend* een feature mag uitdrukken. Of een concreet
  model/firmware/interface überhaupt bewezen werkt, staat los daarvan in `docs/hardware/evidence.md` en in het
  viewmodel als `HardwareStatus::Unverified` (ook voor gewone Line). Geen van beide wordt uit een upload-ACK afgeleid.
- **Wie zet `Supported`:** alleen een profielbewerking met een bewijsverwijzing (`SupportEvidence { doc_anchor, firmware }`)
  voor features die niet "gewone events" zijn (`ScanAngle`, `GrayscaleModulation`, `CommonLine`, `ZStep`). Voorstel aan CAM:
  `Support::Supported` blijft zonder veld (gewone events); verhogen van een *conservatief niet-supported* feature vereist
  `with_evidence(feature, SupportEvidence)` — de evidence gaat mee in de canonieke profielhash (DR-18).
- **Defaults zijn deel van de hash:** de canonieke vorm schrijft de support van *alle* `Feature::ALL` (ook defaults), zodat
  een nieuwe compilerdefault een nieuwe `ProfileRevision.content` geeft en oude `CompiledJob`s verouderd worden (zo gebouwd; behouden).
- **Tweede verdediging:** `ruida::lower` controleert opnieuw per `PlanEvent` tegen `RuidaProfile` en faalt met
  `LowerError::Unsupported { event, feature }` (`unsupported_modulation_mode_is_rejected_instead_of_replaced`). Nooit vervangen.
- Offsets (OffsetFill, kerf, leads) via `geometry::offset`; één tolerantietype (`ProductionTolerance`).

## IM26 — Arrays en mixed-part nesting (`burnburn-cam::nest`)

*Aangescherpt fase C: invoer/uitvoer bindend. Module in cam (geen crate) tot de contracttests een eigen grens
rechtvaardigen. Eigenaar CAM.*

```rust
// invoer — door cam uit de snapshot gebouwd (presentation geeft alleen ids + parameters)
pub fn nest_input(snapshot: &DocumentSnapshot, plate: PlateId, req: &NestRequest, tol: &ProductionTolerance)
    -> Result<NestInput, NestError>;
pub struct NestRequest { pub parts: Vec<(PartInstanceRef /* of PartId + aantal */, u32)>, pub stock: Vec<StockRef>, pub params: NestParams }
pub struct NestInput { pub parts: Vec<NestPart>, pub stock: Vec<Stock>, pub params: NestParams, pub inputs: ContentHash }
pub struct NestPart { pub part: PartRef, pub outline: Vec<Polygon> /* geflattend, mm, buiten CCW, gaten CW */,
                      pub count: u32, pub rotations: RotationSet, pub mirror_allowed: bool, pub priority: u32 }
pub enum RotationSet { Fixed, Steps { step_deg: u16 /* deelt 360 */ }, List(Vec<Angle>) }   // relatief t.o.v. stock-nerf
pub struct Stock { pub id: StockId, pub outline: Vec<Polygon> /* restplaat met gaten */, pub grain: Option<Angle>,
                   pub keep_out: Vec<Polygon> /* klemmen, eerdere snedes */ }
pub struct NestParams { pub spacing: Length, pub edge_margin: Length, pub fill_holes: bool, pub seed: u64,
                        pub max_iterations: u32 }
// uitvoer
pub fn nest(input: &NestInput, cancel: &CancellationToken) -> Result<NestResult, NestError>;
pub struct NestResult { pub inputs: ContentHash, pub placements: Vec<Placement>, pub unplaced: Vec<Unplaced>,
                        pub utilisation: Vec<Utilisation> /* per stock */, pub status: NestStatus }
pub struct Placement { pub part: PartRef, pub copy: u32, pub stock: StockId, pub rotation: Angle, pub mirrored: bool,
                       pub transform: Affine /* part-frame → plate-frame */ }
pub struct Unplaced { pub part: PartRef, pub count: u32, pub reason: UnplacedReason /* TooLarge | NoSpace | NoAllowedRotation */ }
pub enum NestStatus { Complete, BudgetExhausted }                    // best-so-far; deterministisch (iteraties, geen klok)
pub struct Utilisation { pub stock: StockId, pub used_area: f64, pub basis: UtilisationBasis, pub value: f64 }
pub enum UtilisationBasis { StockArea, UsedBoundingBox, ConvexHull }
pub enum NestError { Cancelled, InvalidInput { .. }, EmptyStock }
```

Regels:
- **Aantallen exact:** Σ geplaatst + Σ `unplaced` = gevraagd, per part (`nesting_keeps_order_counts_and_respects_grain_rotation`,
  property-test over willekeurige invoer).
- **Rotaties** alleen uit `RotationSet`, relatief t.o.v. `Stock.grain` (zonder nerf: t.o.v. de plaat-x-as); spiegelen
  alleen met `mirror_allowed`.
- **Afstand ≠ kerf.** `spacing` is de minimale afstand tussen *nominale* outlines; kerf blijft in `cam` (één keer).
  Preflight-`Warning SpacingBelowKerf` als `spacing < kerf` van de toegewezen operation (behalve bij common-line, §IM32).
- **Geometrie-invariant:** na plaatsing overlapt geen (met `spacing/2` geoffset) part een ander of `keep_out`, en ligt
  alles binnen `stock` − `edge_margin`; gecontroleerd met `geometry::boolean` in de test, niet met de nester zelf.
- **Determinisme:** zelfde `NestInput` (incl. seed) → bit-gelijke `NestResult` op Mac en Windows; geen `HashMap`-iteratie,
  geen threads in de kern.
- **Toepassen:** taak met `inputs`-hash (§11). Resultaat → `ComponentCommand::ApplyNest { plate, result }` (één undo);
  is de `inputs`-hash van de plaat intussen anders → `Delivery::Stale`. Annuleren levert nooit een gedeeltelijke patch.
- **Benuttingsnoemer** staat in het resultaat en de UI noemt hem altijd ("62 % van de plaatoppervlakte").

## IM27 — Camera en registratie

*Aangescherpt fase C: cameraport per OS. Poort en logica headless; alleen device-IO in de hosts.*

```rust
// burnburn-presentation (poort; pull-model zoals taken)
pub trait CameraPort: Send + Sync {
    fn devices(&self) -> Result<Vec<CameraDevice>, CameraError>;
    fn authorization(&self) -> CameraAuthorization;           // NotDetermined | Denied | Restricted | Authorized
    fn request_authorization(&self);                           // async; uitkomst via host-wake-up + authorization()
    fn start(&self, d: &CameraDeviceId, mode: &CameraMode) -> Result<(), CameraError>;
    fn stop(&self, d: &CameraDeviceId);
    fn latest_frame(&self, d: &CameraDeviceId) -> Option<Arc<CameraFrame>>;   // nooit blokkerend
}
pub struct CameraDevice { pub id: CameraDeviceId /* stabiele OS-string */, pub name: String, pub modes: Vec<CameraMode> }
pub struct CameraMode { pub width: u32, pub height: u32, pub fps_milli: u32, pub pixel_format: CameraPixelFormat }
pub struct CameraFrame { pub device: CameraDeviceId, pub mode: CameraMode, pub seq: u64, pub image: RasterImage, pub time: MonotonicTime }
#[non_exhaustive] pub enum CameraError { PermissionDenied, DeviceGone, ModeUnsupported, Busy, Platform { code: i64 } }
// burnburn-image::convert (puur; headless getest met bekende vectoren)
pub fn to_raster(format: CameraPixelFormat /* Nv12 | Yuy2 | Bgra8 | Rgb24 */, w: u32, h: u32, stride: usize, bytes: &[u8])
    -> Result<RasterImage, ConvertError>;
// burnburn-image::calib (puur)
pub struct CameraConfigKey { pub device: CameraDeviceId, pub mode: CameraMode, pub lens_profile: Option<ContentHash> }
pub struct CameraCalibration { pub valid_for: CameraConfigKey, pub intrinsics: Intrinsics, pub bed_homography: [[f64; 3]; 3],
                               pub residuals_mm: ResidualStats, pub check_points: Vec<CheckPoint> }
pub fn calibrate(observations: &[MarkObservation], cfg: CameraConfigKey) -> Result<CameraCalibration, CalibError>;
```

| | macOS (`burnburn-macos`) | Windows (`burnburn-windows`) |
|---|---|---|
| API | AVFoundation via `objc2-av-foundation` (register: kandidaat; zelfde objc2-familie als ADR 0002) | Media Foundation via `windows` (`Win32_Media_MediaFoundation`): `IMFSourceReader` |
| Toestemming | `NSCameraUsageDescription` in Info.plist (`xtask bundle`); `AVCaptureDevice::authorizationStatus` | privacy-instelling; `E_ACCESSDENIED` → `PermissionDenied` |
| Thread | `AVCaptureVideoDataOutput`-delegate op een eigen serial dispatch-queue | eigen capture-thread met synchrone `ReadSample` |
| Frame-overdracht | delegate kopieert de pixelbuffer (locked base address, stride) naar een `Vec<u8>`, roept `to_raster`, zet `Arc<CameraFrame>` in één `Mutex<Option<…>>`-slot (laatste wint), wekt de host; nooit blokkeren, nooit unwinden (catch_unwind niet nodig: geen panics door `unwrap`-verbod; FFI-callback retourneert altijd) | idem vanaf de capture-thread |
| Device-ID | `AVCaptureDevice.uniqueID` | symbolic link van de MF-bron |
| Tests | native integratietest met `BURNBURN_REQUIRE_CAMERA=1` (anders `SKIPPED`, geen pass); toestemmingsdialoog = **handmatige** acceptatie | idem |

Regels:
- Kalibratie alleen geldig voor exact dezelfde `CameraConfigKey` (`calibration_is_invalid_after_camera_resolution_changes`);
  residuen op **onafhankelijke** controlepunten die niet in de fit zaten.
- Camerapixels zijn nooit CAM-invoer zonder expliciete gebruikersplaatsing; overlay is hulp. "Vastzetten als
  achtergrond" maakt een asset (bytes + `CameraConfigKey` + kalibratiehash) met één undo-command.
- Presentation-tests gebruiken een `FakeCameraPort` (presentation-harness) met scriptbare frames en toestemming;
  `to_raster` heeft vectortests per formaat (incl. oneven breedte, stride > breedte).
- Lineaire algebra: eigen kleine oplosser (8×8 DLT + Gauss-Newton) heeft voorkeur boven `nalgebra` (Apache-2.0-only,
  grote graaf); beslis bij de eerste `calibrate`-test (register).

## IM28 — Migratie: LBRN2, PDF, werkplaatspakket (`burnburn-io::import`)

- Zelfde tweestapsvorm als §14 (`analyze` → `ImportDecision` → `import`), `ImportFormat::{Lbrn2, Pdf}`.
- **LBRN2** (XML, `roxmltree`): expliciete subset (shapes, paden, groepen, cut settings → `Operation`-voorstellen). Elk
  onbekend element, attribuut en elke niet-vertaalbare instelling → `ImportNote::Dropped { code, entity, count }`
  (`import_reports_unknown_lbrn2_options_without_silent_drop`). Cut settings worden *voorstellen* in het rapport, geen
  stille 1-op-1-kopie (vermogensschalen verschillen per machine).
- **PDF**: alleen vectorpaden uit content streams (`m l c v y h re`, `cm`, `q/Q`, fill/stroke-onderscheid) met limieten
  op objecten, streamgrootte en nesting; tekst en beelden in PDF → `Dropped` (later: tekst als outlines). Parser:
  `lopdf` of eigen objectlezer (register; beslissing in IM28 op fuzz-robuustheid en graafgrootte).
- **Werkplaatspakket** `.burnburnpack`: zip met project(en), fonts, assets, profielsnapshot (`ProfileRevision`) en
  optioneel `CompiledJob`-blobs; openen toont welke fonts/profielen ontbreken of afwijken, nooit stil vervangen.
- Projectzoekfunctie: index in `burnburn-history` (projecthash → pad, naam, laatste executions); geen OS-zoekindex.

## IM29 — Machinebeheer en calibratie

```rust
pub struct MachineProfile { pub id: MachineId, pub frame: MachineFrame, pub capabilities: MachineCapabilities,
                            pub calibration: CalibrationSet, pub revision: ProfileRevision /* hash over alles */ }
pub struct CalibrationSet { pub kerf: BTreeMap<MaterialKey, Length>, pub scan_offset: ScanOffsetTable /* speed → offset */,
                            pub pointer_offset: Option<[Length; 2]> }
pub struct ControllerSettingsSnapshot { pub machine: MachineId, pub read_at: WallTime, pub values: BTreeMap<SettingKey, SettingValue> }
pub fn diff(a: &ControllerSettingsSnapshot, b: &ControllerSettingsSnapshot) -> Vec<SettingChange>;
```

Regels: calibratiepatronen zijn gewone `JobPlan`s uit `cam::calibration`; metingen voert de gebruiker in; elke wijziging
geeft een nieuwe `ProfileRevision`, waardoor elke `PreparedJob`/`CompiledJob` met de oude revisie *verouderd* is
(`scan_calibration_update_invalidates_dependent_prepared_plan`). Controllerinstellingen lezen = idempotente queries;
**schrijven** is een aparte `MachineAction::WriteSettings` met backup-snapshot in de historie vóór verzending, expliciete
bevestiging en bewijs per instelling. USB volgt het driver-thread-model (ADR 0005) met een eigen bewijsmatrix.

## IM30 — CLI en plugincontract

- **CLI** (`burnburn-cli`) gebruikt `AppController` headless + dezelfde `run_task`; subcommando's `inspect`, `compile`
  (→ plan-JSON + RD + preflight), `export`, `upload`, `start`, `status`. `upload` en `start` zijn **gescheiden**
  intenties; `start` vereist een expliciete `--execution <id>` uit een eerdere upload met `Stored`-bewijs; geen
  `upload --and-start`. `cli_and_gui_compile_same_revision_to_same_normalized_plan`: zelfde `JobPlanId`.
- **Plugins: out-of-process**, versioned JSON-Lines over stdio (`burnburn-plugin/1`), gestart door runtime met timeout
  en `cancel`-bericht. Geen dynamisch laden van native code (geen ABI, geen `unsafe`), geen script-VM in-process.
  Pluginsoorten: generator (levert een `DocumentPatch`-voorstel met eigen `DerivedOwner::Plugin`), importer (levert
  `ImportedGeometry` + rapport), exporter (leest een `JobPlan`/snapshot, schrijft bestand). Plugins krijgen **nooit**
  `MachineIntent`-toegang. Alles wat terugkomt gaat door dezelfde validatie (eindig, limieten) en wordt één
  undo-bare command. Schema in een kleine crate `burnburn-plugin-api` (laag 2, alleen serde-DTO's) wanneer de eerste
  plugintest er is.
- **Prestaties:** 100k-segmentbench (`cargo xtask bench`) voor scene-opbouw, tessellatie, compile en lower; budgetten
  vastleggen ná de eerste meting, niet vooraf verzinnen.

## IM31 — Orders, herkenning, operator en machineservice

*Aangescherpt fase C: rollen en machineservice bindend. Eigenaar User-flow (presentation/editor) en Protocol (runtime-service).*

```rust
pub struct Order { pub id: OrderId, pub lines: Vec<OrderLine>, pub due: Option<WallDate> }
pub struct OrderLine { pub part: PartRef, pub variant: Option<VariantKey>, pub quantity: u32, pub material: MaterialKey }
pub struct ActionId(u128);            // door AppPorts.new_action_id() per klik; presentation genereert zelf geen random
pub enum Role { Designer, Operator }  // app-modus per venster/sessie, géén beveiligingsgrens (zie onder)

// burnburn-runtime
pub struct MachineService { /* per MachineId één driver-thread met één ExecutionActor; History via HistoryStore */ }
impl MachineService {
    pub fn submit(&self, req: MachineRequest) -> Result<SubmitReceipt, SubmitError>;   // niet-blokkerend op de UI-thread
    pub fn events(&self) -> Vec<MachineEvent>;                                          // pull, zoals taken
}
pub struct MachineRequest { pub action: ActionId, pub role: Role, pub intent: MachineIntent /* §9 */ }
pub enum SubmitReceipt { Accepted { action: ActionId }, AlreadyHandled { action: ActionId, outcome: ActionOutcome } }
#[non_exhaustive] pub enum SubmitError { NotPermittedForRole, RequirementNotProven { required, actual },
                                         MachineLockedByOtherProcess, HistoryNotDurable, ProfileOutdated }
```

**Rechtenmatrix (presentation filtert `CommandKey`s; `MachineService` controleert opnieuw):**

| Actie | Designer | Operator |
|---|---|---|
| Document bewerken, compileren, `CompiledJob` maken | ja | nee |
| Upload / Start / Stop / Frame van een bestaande `CompiledJob` | ja | ja |
| Queue-aantallen wijzigen, `OperatorNote` | ja | ja |
| `Reconciled { OperatorConfirmed }` (Unknown oplossen) | ja | ja (record bevat rol + ActionId) |
| Profiel/kalibratie/controllerinstellingen schrijven (`WriteSettings`) | ja | nee |

Regels:
- **Eén machineservice per proces** bezit elke verbinding; vensters en de CLI praten ermee, nooit rechtstreeks met een
  `ControllerPort`. **Tussen processen:** exclusieve lock-file `<app-data>/machines/<machine-id>.lock` (exclusive create,
  pid + start-tijd erin); een tweede proces krijgt `MachineLockedByOtherProcess` en mag alleen de historie lezen. Een
  verweesde lock (pid bestaat niet) wordt pas na expliciete gebruikersbevestiging vrijgegeven. IPC tussen processen: later.
- **Idempotentie:** `submit` zoekt eerst `History::action_outcome(action)`; bestaat die, dan `AlreadyHandled` zonder
  send (`reconnected_operator_view_does_not_repeat_last_machine_action`). Anders: `IntentRecorded { action }` durable →
  pas dan naar de driver. Dubbelklik = twee klikken = twee `ActionId`s → de tweede faalt op `requires` (bewezen toestand),
  niet op de ID.
- **Rol is geen beveiliging** (v1 heeft geen accounts): de matrix voorkomt vergissingen; wie het OS-account heeft, kan
  alles. Echte authenticatie = eigenaarsbeslissing (open-beslissingen B-7).
- Herkenning (camera → onderdeel/restplaat) levert kandidaten met score; bij ambiguïteit kiest de mens. Extra
  backends/assen alleen met eigen capability-bewijs.

## IM32 — Kleurtracing, gedeelde randen en common-line

*Aangescherpt fase C: twee begrippen die tot nu toe door elkaar liepen, worden gescheiden.*

```rust
// burnburn-trace::colour (Trace)
pub fn quantize(img: &RasterImage, k: u8, seed: u64) -> Palette;           // deterministisch (vaste seed, integer)
pub struct RegionGraph { pub regions: Vec<Region>, pub boundaries: Vec<BoundaryEdge> }
pub struct Region { pub id: RegionId, pub colour: PaletteIndex, pub boundary: Vec<(BoundaryId, Direction)> }
pub struct BoundaryEdge { pub id: BoundaryId, pub left: RegionId, pub right: Option<RegionId>, pub path: Path }
// document: regio-uitvoer = afgeleide objecten (DerivedOwner::Trace); elke BoundaryEdge één EdgeOrigin (§IM21),
// zodat twee regio-objecten die dezelfde grens delen, segmenten met dezelfde EdgeName dragen.

// burnburn-document::process (CAM) — per operation
pub enum SharedEdgePolicy { CutOnce, CutPerRegion }                       // default CutOnce
pub struct CommonLine { pub enabled: bool, pub tolerance: Length, pub min_overlap: Length }   // default uit
```

1. **Gedeelde bron-rand (altijd, geen feature):** twee targets van dezelfde Line-operation die een segment **exact**
   delen (bit-gelijk, in een van beide richtingen — zo levert de `RegionGraph` ze; `process::SharedEdgePolicy`, CAM), plant
   `cam` één keer bij `CutOnce`; het tweede voorkomen wordt laser-uit travel en preflight waarschuwt
   (`shared_colour_boundary_is_not_scheduled_twice_without_explicit_policy`). `CutPerRegion` is een expliciete keuze.
   Kerf op zo'n rand is niet aan twee kanten te compenseren → middellijn snijden + preflight-`Warning KerfOnSharedBoundary`.
2. **Common-line cutting (feature, standaard uit):** *verschillende* bron-randen die na kerf-offset samenvallen
   (parts op afstand = kerf, of rakend bij kerf 0), binnen `tolerance` en over ≥ `min_overlap`. Vereist
   `Feature::CommonLine` = `Supported` (default `Unverified` → preflight-`Error`). Werkwijze: cam berekent eerst de
   kerf-gecompenseerde toolpaths per part, zoekt samenvallende spans (deterministisch, op toolpaths, niet op
   nominale outlines), plant elke span één keer als deel van het *laatst* gesneden part, en pas nadat de
   binnencontouren van beide parts gesneden zijn (ordeningsregel; test `common_line_span_is_cut_after_inner_features_of_both_parts`).
3. **Herkomst in het plan:** een eenmaal geplande gedeelde span verwijst naar beide bronnen:
   `PlanEvent::Cut { to, origin: Option<SourceRef>, shared: Option<SourceRef> }` (additief; CAM-keuze voor de vorm),
   zodat preview/preflight beide parts kan markeren.
4. AutoConstrain levert *voorstellen* met residu die de gebruiker accepteert (één command per acceptatie), nooit
   automatisch toegepast.
