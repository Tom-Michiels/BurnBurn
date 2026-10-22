# G9-cam-werk (golf 9): nerf als vrije hoek, Tight sneller, W06 gespiegeld/perspectief/deels zichtbaar, onderdeelnaam, compile bij 100k

Basis: commit d751627 (main na golf 8). Alle commando's met `export PATH="$HOME/.cargo/bin:$PATH"`,
`CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/g9-camwerk`, `CARGO_INCREMENTAL=0`, toolchain 1.99.1, `--locked`.
Alleen werkelijk uitgevoerde commando's en hun uitkomst staan hieronder. "Rood" is steeds een gedragsrood (een falende
bewering) na een minimale stub, geen compilerfout; een test zonder rode fase is zo gemarkeerd. Geen hardware, geen netwerk,
geen gebruikersprojecten.

## Wat er is

| Onderdeel | Gebouwd | Waar |
|---|---|---|
| Nest: nerf als vrije hoek | per onderdeel een vierde keuze "Nerf onder een hoek" met een veld (graden, decimaalkomma ok, standaard 45); het onderdeel draait tot die richting langs de nerf van de plaat ligt | `presentation/src/schikken.rs` (`NestPartGrain::Angle(tienden)`), `nest_task.rs` (`angled_rotations`), `host-models/src/panels/schikken.rs` (`NestPartGrainAngle`) |
| `Packing::Tight` sneller | diepe-cellenmasker (zeker overlap zonder polygon-overlay) + geen overlay voor de plaatrand bij een rechthoekige plaat; zelfde plaatsingen | `cam/src/nest.rs` (`DeepMask`, `exact_valid`, `NestWork`, `nest_packed_counted`) |
| W06 gespiegeld | het product mag het spiegelbeeld van het sjabloon zijn (gespiegeld in de eigen y-as, daarna gedraaid); de kopie die erop gezet wordt wordt ook gespiegeld | `image/src/locate_pose.rs` (`PoseParams::mirror`, `PoseMatch::mirrored`), `presentation/src/controller/recognition.rs` (`placed`) |
| W06 perspectief | homografie met twee kantelparameters (`max_tilt`, 0 tot 0,6): de offset van een sjabloonpixel wordt gedeeld door `w = 1 + (tx*mx + ty*my)/R` | `image/src/locate_pose.rs` (`Pose::point`) |
| W06 deels zichtbaar | een product dat deels buiten het beeld ligt wordt gescoord op de zichtbare pixels (eigen gemiddelde); de treffer zegt hoeveel (`visible`) en het paneel meldt "DEELS ZICHTBAAR, n %"; de operator beslist nog steeds over elke treffer | `locate_pose.rs` (`partial_similarity`, `min_visible`), `host-models/src/panels/g7_werk.rs`, `l10n_g7_werk.rs` |
| W01/W02 onderdeelnaam | een expliciete naam in het project (`part-name.json`, los van de objectnamen, niet in de projecthash), in te vullen in het notitiepaneel; een order vindt de job op die naam; dezelfde naam in meer jobs: keuze van de operator | `io/src/{project,container}.rs` (`Project::part_name`, `order_part_names`), `presentation/src/controller/{notes,machine,tasks,document}.rs`, `runtime/src/history_store.rs` |
| Compile bij 100k contouren | lineaire afbeelding van fill-buckets op delen (was kwadratisch); meetbank voor compile-schaal | `cam/src/compile.rs` (`local_buckets`, `ChunkStats`), `cam/tests/compile_scale_measure.rs` |

Beide hosts: gedeelde paneelmodellen in `burnburn-host-models` (nieuwe `SchikkenControl::NestPartGrainAngle`,
`G5Control::{RecogMirror, RecogPerspective, RecogPartial, NotesPartName, NotesPartNameSave}`), teksten NL/EN, geen nieuw
menu-item. Nieuwe intents/velden zijn additief: `UiIntent::SetPartName`, `RecognitionIntent::SetSearchExtras`,
`SearchChoice.{mirror, perspective, partial}`, `CandidateView.{mirrored, visible_percent}`, `NotesViewModel.part_name`.

## TDD-log

| Test | Rood (echt gezien) | Red-commando | Groen |
|---|---|---|---|
| `cam nest_tight_golden::tight_placements_of_three_scenes_match_the_recorded_implementation` (karakteriseringstest, 42 regels in `tests/data/nest_tight_golden.txt`: gewone plaat, plaat met keep-out en spiegelen, gaten op twee platen) | geen rode fase: vastgelegd op de oude implementatie en daarna ongewijzigd groen na de versnelling | `BB_PRINT_GOLDEN=1 cargo test -p burnburn-cam --locked --release --test nest_tight_golden -- --nocapture` (opname); `cargo test -p burnburn-cam --locked --test nest_tight_golden` | ok vóór en na |
| `cam nest_tight_golden::tight_needs_far_fewer_polygon_overlays_for_the_same_placements` | `15006 overlays against a baseline of 15006` (teller `NestWork::overlays`, alleen een telstub op de oude code) | `cargo test -p burnburn-cam --locked --test nest_tight_golden` | 4995 overlays (3,0x minder), plaatsingen ongewijzigd; hele `burnburn-cam`-suite ok |
| `presentation g9_nest_grain_angle` (5) | `NestPartGrain::Angle` viel in de stub op "volgt de plaat": 2 FAILED (0 geplaatst i.p.v. 1), 3 ok (bewaking: volgt de plaat past niet; verkeerde hoek past niet; zonder nerf op de plaat verandert de hoek niets) | `cargo test -p burnburn-presentation --locked --test g9_nest_grain_angle` | 5 ok |
| `macos g9_cam_werk_panels nest_panel` (2) | 3 keuzes i.p.v. 4 en het veld stuurde geen intent: 2 FAILED | `cargo test -p burnburn-macos --locked --test g9_cam_werk_panels` | ok; de bestaande `g8_cam_werk_panels` verwachtte 3 keuzes en verwacht nu 4 (bewuste wijziging) |
| `windows g9_cam_werk_forms::the_angle_choice_adds_a_field_…` (1) | geen rode run: gedeeld paneelmodel | `cargo test -p burnburn-windows --locked --test g9_cam_werk_forms` | ok; `menus` (8) blijft ok |
| `image g9_locate_pose` (14: gespiegeld 4, perspectief 3, deels zichtbaar 5, parameters, determinisme) | velden bestonden alleen in de stub: 11 FAILED, 3 ok (bewaking: origineel wordt niet als gespiegeld gemeld; deels zichtbaar wordt niet gevonden zonder vraag; heel zichtbaar = 1,0). Na de implementatie 3 testfouten van mijn eigen oracle hersteld: (a) het G8-product met twee kenmerken is bijna zijn eigen spiegelbeeld op een draai na (score 0,99 ongespiegeld), dus de G9-tests gebruiken een product met drie kenmerken van verschillende sterkte en afstand; (b) `min_visible 0,9` mag een naar binnen geschoven, heel zichtbare pose opleveren, de test eist nu `visible >= 0,9`; (c) bij `min_score 0,7` kwamen extreme spookposes (kanteling 0,4, schaal 0,72, score 0,79 tot 0,84) als tweede treffer, de tests eisen 0,85 | `cargo test -p burnburn-image --locked --test g9_locate_pose` | 14 ok (release 7 s; debug 9 s dankzij `[profile.dev.package.burnburn-image] opt-level = 2`, voorheen ruim 80 s) |
| `presentation g9_recognition` (8) | zoeken negeerde de keuzes: 4 FAILED, 4 ok (standaardwaarden). De test van de gespiegelde kopie is los rood gezien door de spiegeling in `placed` tijdelijk uit te zetten: FAILED (kopie stond op de verplaatste plek, niet gespiegeld), daarna hersteld | `cargo test -p burnburn-presentation --locked --test g9_recognition` | 8 ok |
| `macos g9_cam_werk_panels recognition` (3) | controls bestonden alleen als stub zonder paneelregel: 3 FAILED | idem | 3 ok |
| `windows g8_recognition_form::the_form_has_the_three_new_search_choices_…` | geen rode run: gedeeld model | `cargo test -p burnburn-windows --locked --test g8_recognition_form` | ok |
| `io g9_part_name` (6) | veld zonder opslag: 4 FAILED van 5 (`a_file_from_before_the_field_loads_without_a_name` ok, bewaking); `the_names_an_order_can_use_…` met een stub die de objectnamen gaf: FAILED | `cargo test -p burnburn-io --locked --test g9_part_name` | 6 ok; hele `burnburn-io`-suite ok. Eén testfout van mij hersteld: een dubbele `part-name.json` kan met de zip-crate niet gemaakt worden; die bewering is weggelaten (de algemene dubbele-ingang-regel bestond al) |
| `presentation g9_part_name` (7) | stub-intent deed niets: 6 FAILED, 1 ok (zonder expliciete naam vinden de objectnamen de job nog) | `cargo test -p burnburn-presentation --locked --test g9_part_name` | 7 ok |
| `runtime g8_job_parts::an_explicit_part_name_replaces_the_object_names_…` | met `job_parts` tijdelijk terug op de objectnamen: FAILED (`["Beugel", "Plaatje"]`), daarna hersteld | `cargo test -p burnburn-runtime --locked --test g8_job_parts` | 3 ok |
| `macos g9_cam_werk_panels part_name` (2) | controls zonder paneelregel: 2 FAILED | `cargo test -p burnburn-macos --locked --test g9_cam_werk_panels` | 7 ok in het hele bestand |
| `windows g9_cam_werk_forms part_name::…` | geen rode run: gedeeld model | `cargo test -p burnburn-windows --locked --test g9_cam_werk_forms` | 2 ok |
| `cam lib compile::tests::{mapping_the_buckets_of_a_big_unit_…, many_parts_cost_no_more_than_one_pass_…}` | oude algoritme in `local_buckets` met teller: `looked at 8002000 contour indices for 4000 contours` en `14859142 … in 400 parts` | `cargo test -p burnburn-cam --locked --lib compile::tests` | beide ok na de lineaire afbeelding (<= 4n stappen); `the_mapping_equals_the_original_one` (proptest tegen het oorspronkelijke algoritme, ook buckets die delen overspannen) was vanaf het begin groen en bleef groen; hele `burnburn-cam`-suite ok |

## Metingen (release, één machine, load average onbekend; tijd is wandklok, geen CPU-tijd tenzij vermeld)

**Nest `Packing::Tight`** (`cargo test -p burnburn-cam --locked --release --test nest_tight_measure -- --ignored --nocapture`, synthetische plaat van
600 x 400 mm, 3 mm tussenruimte, 5 mm rand, kwartslagen). Geplaatst, benutting en bereik zijn identiek voor en na.

| Onderdelen, stap | Tight voor | Tight na | Grid |
|---|---|---|---|
| 39, 2 mm | 0,85 s | 0,20 s | 0,01 s |
| 39, 1 mm | 0,64 s | 0,28 s | 0,07 s |
| 78, 2 mm | 3,27 s | 0,41 s | 0,03 s |
| 117, 2 mm (85/117 geplaatst) | 4,75 s | 0,50 s | 0,04 s |
| 156, 2 mm (73/156 geplaatst) | 2,44 s | 0,46 s | 0,03 s |

Dus 2,3x tot 9,5x sneller (4,3x, 2,3x, 8,0x, 9,5x, 5,3x); ten opzichte van Grid blijft Tight 4x tot 20x trager (G8: 40x tot 200x). De profilering
(tijdelijke tellers, daarna verwijderd) liet vóór de wijziging zien dat bij 78 onderdelen van 177 473 exacte controles er 173 178 op een naburig onderdeel faalden
(98 %) en dat de overlay voor de plaatrand ongeveer 16 % van de overlaytijd kostte.

**Compile bij 100k contouren** (`BB_COMPILE_ALL=1 cargo test -p burnburn-scenarios --locked --release --test benchmarks_100k -- --ignored --nocapture --test-threads=1`;
wandklok van `compile` alleen; `burnburn-cam` en `burnburn-geometry` gebruiken geen threads (`thread::`, `rayon`, `spawn` komen er niet voor), dus de wandklok is daar de CPU-tijd
van één kern plus eventuele planner-wachttijd; een aparte CPU-teller is niet toegevoegd):

| Fixture | Voor G9 (opnieuw gemeten) | Na G9 |
|---|---|---|
| F100K-A (1 000 x 100-hoek) | 25,1 ms (G5-cam) | 24,2 ms |
| F100K-B (10 000 x 10-hoek) | 55,1 ms (G5-cam) | 48,0 ms |
| F100K-C (100 000 losse lijnen) | 1 492,5 ms (G5-cam: 1 450,8 ms; origineel ~92 000 ms) | 194,6 ms |

Schaalproef `cargo test -p burnburn-cam --locked --release --test compile_scale_measure -- --ignored --nocapture` (n losse lijnen, verdubbelen):

| n | voor | na |
|---|---|---|
| 12 500 | 38,1 ms | 25,0 ms |
| 25 000 | 118,2 ms (x3,1) | 43,0 ms (x1,7) |
| 50 000 | 408,0 ms (x3,5) | 97,0 ms (x2,3) |
| 100 000 | 1 478,4 ms (x3,6) | 267,3 ms (x2,8) |
| 200 000 | 5 722,1 ms (x3,9) | 438,4 ms (x1,6) |

De factor ~4 per verdubbeling voor de fix is het kwadratische signaal. Het zat in `order_chunks`: per fill-bucket een lineaire
zoektocht (`members.iter().position`) naar de plek in het deel. Na de fix is de groei ongeveer lineair (de spreiding 1,6 tot 2,8 is machineruis).
`sample` (macOS) op de 200k-run wees `order_chunks` aan. Opmerking, buiten mijn gebied: in dezelfde profielen zat `Operation::new`
(`burnburn-document/src/process/operation.rs`) met een kwadratische dubbele-doelen-controle (`targets[..i].contains(g)`), ~2,8 s bij 200k doelen; dat
is geen compile maar het toevoegen van een bewerking aan 100k objecten. Staat in de wachtrij.

## Ontwerpkeuzes en aannames

- **Tight: diepe cellen.** Een raster (cel = stap/8, minstens vier productietoleranties) markeert cellen die minstens één celbreedte diep in de
  opgevulde omtrek (`grown`) van een geplaatst onderdeel liggen (celmiddelpunt binnen, geen randsegment in de cel of in een van de acht buren). Valt een
  bemonsterd randpunt van het te plaatsen onderdeel (elke ≤ één celbreedte) in zo'n cel, dan overlapt het zeker en wordt de overlay overgeslagen; alle andere
  gevallen beslist de overlay als voorheen. Het onderdeel heeft naast een randpunt altijd binnenkant, dus de overlap is breder dan de tolerantie en nooit het
  "verwaarloosbare" randje (< 1e-9 mm²) dat de exacte controle toestaat. Een rechthoekige bruikbare plaat (één contour die zijn eigen bounding box vult) slaat de
  overlay voor de plaatrand over zodra de bounds van het onderdeel binnen de plaat liggen. Grote platen krijgen grovere cellen (maximaal 4 miljoen). De
  karakteriseringstest bewijst gelijke plaatsingen op drie scènes; een dwangmatig bewijs voor alle invoer is er niet (een heuristische afwijzing met een marge,
  geen wiskundig bewijs bij ontaarde onderdelen met kenmerken kleiner dan een celbreedte).
- **Nerf als hoek.** `NestPartGrain::Angle(tienden)`, 0 tot 1800 (richting, geen zin: 210° is 30°). Het onderdeel krijgt `grain_deg = hoek` en als draaiingen alleen
  de draai die de hoek langs de nerf van de plaat brengt (en die plus 180° als de gekozen draaiingen een halve slag kennen; met spiegelen ook die van het spiegelbeeld).
  Daardoor past een strook die 30° getekend is op een bed dat alleen een plat liggende strook toelaat, ook zonder kwartslagen. Zonder nerf op de plaat doet de regel niets.
- **W06 model.** `x = c + scale * Rot(angle) * M(offset) / w`, `M` = spiegeling `ox -> -ox`, `w = 1 + (tx*mx + ty*my)/R`, `R` = halve diagonaal van het sjabloon;
  het sjabloon wordt nooit geresampled (elke sjabloonpixel wordt in het beeld opgezocht, bilineair). Grof zoeken over (spiegel, kanteling in vijf standen,
  schaal, hoek); verfijnen met een patroonzoek die ook de kanteling meeneemt (stap tot 0,005). Deels zichtbaar: correlatie over de pixels binnen het
  beeld met hun eigen gemiddelde, minstens 16 pixels en `min_visible` van het sjabloon (standaard 1,0 = uit; het paneel gebruikt 0,5 en kantelmax 0,4); de
  middelpunten van het grove zoeken lopen daarvoor tot een halve diagonaal buiten het beeld. Een snelle afwijzing via de bounding box van het afgebeelde sjabloon
  (`inside < 0,5 * min_visible * area`) slaat posen over die duidelijk te weinig zichtbaar zijn.
- **Plaatsen van een gespiegelde treffer.** De kopie van de selectie wordt eerst in de verticale as door het midden van de selectie gespiegeld (x -> -x) en dan
  gedraaid, geschaald en gezet. Een kanteling verandert de plaatsing niet: alleen de plek en de hoek tellen, de kalibratie rekent de rest. Een deels
  zichtbare treffer wordt nooit anders behandeld dan een andere: de operator moet hem beslissen (`Undecided` bij plaatsen) en het paneel zegt in een
  probleemregel dat hij op een deel rust.
- **Onderdeelnaam.** `Project.part_name: Option<String>`, `part-name.json` (`{"name": …}`, `deny_unknown_fields`, 1 tot 200 tekens, niet met spaties omrand,
  anders `InvalidValue`), alleen geschreven als er een naam is (een project zonder naam houdt zijn bestandsvorm en hash). Niet in `project_hash`
  (productieboekhouding, zoals `production` en `layer_slots`; anders zou een naamwijziging de plan-id van een vrijgegeven job laten wijzigen). Geen undo-stap; de naam
  maakt het document wel "niet bewaard" (`Session::unsaved`) tot de volgende save. `Project::order_part_names()` is de ene regel voor alle drie de plekken (presentatie,
  geschiedenis, test-fake): de expliciete naam als die er is, anders de objectnamen. Met een expliciete naam vindt een order de job dus *niet* meer op een objectnaam
  (de bestandsnaam blijft werken). Templates nemen de naam niet mee. Een project van vóór het veld laadt zonder naam; oudere builds weigeren een bestand met de nieuwe ingang
  als onbekende ingang (zoals bij elke eerdere additieve ingang).
- **Compile.** De fill-buckets van een unit worden in één pass over alle bucketleden aan de delen toegekend (per deel een positietabel; per bucket een kleine
  tabel van deel naar stuk), dus lineair in de som van de bucketgroottes ook bij veel delen.
- **Workspace.** `[profile.dev.package.burnburn-image] opt-level = 2` in de root-`Cargo.toml` (additief naast `"*"`): de herkenningstests duurden anders minuten.

## Open / onbevestigd

- **Handmatig**: uiterlijk van de nieuwe panelregels (nerfhoekveld, drie herkenningskeuzes, "DEELS ZICHTBAAR"-regel, onderdeelnaamveld in het notitiepaneel); geen native
  AppKit-run voor de nieuwe regels (generieke paneelregels). Windows: alleen headless formulieren, typecheck en clippy voor `x86_64-pc-windows-msvc`.
- **W06**: snelheid en kwaliteit op echte camerabeelden niet gemeten (analytische frames, één machine); spookposes met een lage drempel (zie hierboven) vragen
  een drempel van ongeveer 85 % of de beoordeling van de operator; de kanteling is een benadering voor het zoeken en geen gemeten cameraperspectief; deels zichtbare
  treffers rusten op minder pixels en zijn minder betrouwbaar (daarom de aparte markering); geen automatische keuze tussen een deels zichtbare en een hele treffer.
- **Nest**: `Tight` is gemeten op één synthetische plaat, niet op echte werkplaatsdata; nog geen automatisch verdelen over platen (ongewijzigd).
- **Onderdeelnaam**: geen aparte lijst of zoekfunctie van namen over projecten; dezelfde naam in meer jobs blijft de keuze van de operator.
- **Hardware (ongewijzigd, geweigerd)**: RD/GRBL-uitvoer van Tab Cut Power en common-line, pointeroffset-teken op een machine.
