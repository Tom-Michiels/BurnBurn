# G8-cam-werk (golf 8): common-line volgens IM32, nest per onderdeel, Tab Cut Power in de historie, W06, W07, W01/W02

Basis: commit 16f0f7d (main na golf 7). Alle commando's met `export PATH="$HOME/.cargo/bin:$PATH"`,
`CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/g8-camwerk`, `CARGO_INCREMENTAL=0`, toolchain 1.99.1, `--locked`
(één keer `--offline` om de dev-dependency-regel van runtime op geometry in `Cargo.lock` te zetten).
Alleen werkelijk uitgevoerde commando's en hun uitkomst staan hieronder. "Rood" is steeds een gedragsrood (een falende
bewering), geen compilerfout; een test zonder rode fase is zo gemarkeerd. Geen hardware, geen netwerk, geen gebruikersprojecten.

## Wat er is

| Onderdeel | Gebouwd | Waar |
|---|---|---|
| Common-line (IM32) | het gedeelde stuk wordt gesneden bij de *laatste* vorm, ná het binnenwerk van beide; de documentbrede "Remove Overlapping Lines" houdt de eerste (ongewijzigd) | `cam/src/overlap.rs` (`Keep::{First, Last}`), `cam/src/compile.rs` |
| Nest: nerf per onderdeel | per onderdeel: volgt de plaat (X langs de nerf) / dwars (Y langs de nerf) / vrij draaien; alleen zichtbaar met een nerf op de plaat | `presentation/src/schikken.rs` (`NestPartGrain`), `nest_task.rs`, `macos/src/panels/schikken.rs` |
| Nest: alle onderdelen | de lijst staat in pagina's van 8 met Vorige/Volgende en "Onderdelen 9-16 van 30" (was: de eerste 12) | `macos/src/panels/schikken.rs` (`PART_PAGE_ROWS`), `NestInput::part_page` |
| Nest: namen van kopieën | kopie n van "Pad 1" heet "Pad 1 (n+1)"; een naam die al bestaat wordt overgeslagen | `presentation/src/controller/nest.rs` (`named_copy`) |
| `Packing::Tight` gemeten | zie "Metingen" | `cam/tests/nest_tight_measure.rs` |
| Tab Cut Power in de historie | `PlanEvent::CutPower` in de plan-blob (`cut-power`, vermogen + herkomst), gecontroleerd tegen de plan-id | `history/src/plan_blob.rs` |
| W06 herkenning | rotatie (hele cirkel, stap 10°) en schaal (0,8 / 1 / 1,25 + verfijning), eigen Rust | `image/src/locate_pose.rs` |
| W06 sjablonen bewaren | `recognition-templates.json` (schema 1) naast de andere bibliotheken; opnieuw leren onder dezelfde naam vervangt, id's worden niet hergebruikt | `io/src/recognition_library.rs`, `presentation/src/controller/{library,recognition}.rs` |
| W06 vak wijzen | de twee hoeken (klikken of slepen) in het camerabeeld dat op het blad ligt; Escape stopt | `presentation/src/controller/recognition.rs` (`recognition_pointer`, `recognition_overlays`) |
| W07 aantallen naar de nest | de nest-paneel kent de orders met "nog te maken" (aantal min opgenomen voltooiingen); één knop zet het aantal exemplaren van de onderdelen met die naam | `SchikkenIntent::TakeOrderQuantities`, `controller/nest.rs` |
| W01/W02 job op onderdeelnaam | een order vindt een vrijgegeven job ook op de naam van een getekend onderdeel in het bevroren project (zonder hoofdletters/spaties); meerdere jobs: nog steeds expliciete keuze | `history::load_project`, `HistoryStore::job_parts`, `runtime/src/history_store.rs`, `controller/machine.rs` |

Beide hosts: één headless paneelmodel (macOS-panelen via `#[path]` ook in de Windows-crate); nieuwe `SchikkenControl`- en
`G5Control`-varianten, teksten in `l10n_schikken.rs` / `l10n_g7_werk.rs` (NL/EN), geen nieuw menu-item.

## TDD-log

| Test | Rood (echt gezien) | Red-commando | Groen |
|---|---|---|---|
| `cam g8_common_line_order` (4) | 3 FAILED: het gedeelde stuk (x = 20) stond na de omtrek van de eerste plaat, vóór het gat van de tweede (`cut 5` tegen `last hole 11`); de lengte-test was al groen (bewakingstest). Eerste versie eiste "na alle gaten" en faalde ten onrechte op het gat van de derde plaat; de oracle is gecorrigeerd naar "na de gaten van beide delende platen" en daarna opnieuw rood gezien (stash van `cam/src`) | `cargo test -p burnburn-cam --locked --test g8_common_line_order` | 4 ok; hele `burnburn-cam`-suite ok (incl. `g7_common_line` 6 en proptest "raster van n x m") |
| `presentation g8_nest_ui` (5) | stub liet `part_grain` en kopienamen onbenut: 4 FAILED (rij toonde `Along` i.p.v. `Across`; 0 i.p.v. 1 geplaatst; namen "Pad 1","Pad 2","Pad 1","Pad 1"...), 1 ok (bewaking: volgt de plaat) | `cargo test -p burnburn-presentation --locked --test g8_nest_ui` | 5 ok; `nest`-filter en volledige presentation-suite ok |
| `macos g8_cam_werk_panels` (4) | paginering en nerfkeuze ontbraken: 4 FAILED (12 rijen i.p.v. 8; `NestPartsNext` stuurt geen intent) | `cargo test -p burnburn-macos --locked --test g8_cam_werk_panels` | 4 ok; `g7_cam_panels`, `schikken_panel`, `g6_werk_nest` ok |
| `windows g8_cam_werk_forms` (2) | geen rode run: gedeeld paneelmodel | `cargo test -p burnburn-windows --locked --test g8_cam_werk_forms` | 2 ok; `menus` (8) blijft ok |
| `cam nest_tight_measure` (1 + 1 `#[ignore]`) | meting, geen gedrag: geen rode fase | `cargo test -p burnburn-cam --locked --test nest_tight_measure`; release: `... --release ... -- --ignored --nocapture` | 1 ok; cijfers onder "Metingen" |
| `history g8_plan_cut_power` (4) | plan-blob weigerde de gebeurtenis (`PlanBlob("plan event not representable")`): 4 FAILED | `cargo test -p burnburn-history --locked --test g8_plan_cut_power` | 4 ok; rest van `burnburn-history` ok |
| `image g8_locate_pose` (13 + 1 `#[ignore]`) | De implementatie is eerst geschreven, de tests daarna. Rode fase daarom tegen een stub die direct `Ok(vec![])` teruggeeft: 9 FAILED, 4 ok (validatie, flat/te groot, annuleren, bepaaldheid: bewakingstests). Daarna groen; één test-aanname was fout (een gedraaid product scoort zonder rotatiezoek 0,88, niet <0,7) en is herschreven naar "duidelijk lagere score en een plek die pixels ernaast ligt" | `cargo test -p burnburn-image --locked --test g8_locate_pose` | 13 ok (release 1,3 s; debug ~23 s); prestatiemeting onder "Metingen" |
| `io g8_recognition_library` (9) | stub gaf lege bibliotheek/`Ok(())`: 9 FAILED | `cargo test -p burnburn-io --locked --test g8_recognition_library` | 9 ok; `g7_remnant_library` (6), `g7_camera_calibrations` (8) ok |
| `presentation g8_recognition` (16) | stub: 12 FAILED, 4 ok (bewaking: poort-standaard, 0/1 voor translatie, naamloos niet bewaard, ongedraaide plaatsing) | `cargo test -p burnburn-presentation --locked --test g8_recognition` | 16 ok; volledige presentation-suite ok |
| `macos g8_werk_recognition` (5) | 5 FAILED (geen keuzes/knoppen/regels). Twee eigen test-fouten hersteld: "turned" zat ook in het keuzevakje, en "Kaartje: 29" stond ook in de sjabloonregel | `cargo test -p burnburn-macos --locked --test g8_werk_recognition` | 5 ok; `g7_werk_recognition` (5) ok |
| `windows g8_recognition_form` (2) | geen rode run: gedeeld paneelmodel | `cargo test -p burnburn-windows --locked --test g8_recognition_form` | 2 ok |
| `history g8_load_project` (2) | stub gaf `UnknownJob`: 2 FAILED | `cargo test -p burnburn-history --locked --test g8_load_project` | 2 ok |
| `runtime g8_job_parts` (2) | trait-standaard gaf geen namen: 1 FAILED, 1 ok (bewaking: onbekende job) | `cargo test -p burnburn-runtime --locked --test g8_job_parts` | 2 ok; `g6_approval` (6) ok |
| `presentation g8_order_parts` (5) | 4 FAILED (job niet gevonden op onderdeelnaam; twee jobs: geen keuze), 1 ok (bewaking: bestandsnaam werkt nog) | `cargo test -p burnburn-presentation --locked --test g8_order_parts` | 5 ok; `g7_order_released` (3), `im31_operator` (6), `g6_approval` (6) ok |
| `presentation g8_nest_orders` (4) | stub: 4 FAILED. Eén test-fout hersteld (de afwerking van een stuk vraagt eerst een `Running`-status; nest-paneel opent niet voor de rol Operator, de test gebruikt de ontwerper) | `cargo test -p burnburn-presentation --locked --test g8_nest_orders` | 4 ok |
| `macos g8_nest_orders_panel` (1) | 1 FAILED (geen ordersectie) | `cargo test -p burnburn-macos --locked --test g8_nest_orders_panel` | 1 ok |
| `windows g8_nest_orders_form` (1) | geen rode run: gedeeld paneelmodel | `cargo test -p burnburn-windows --locked --test g8_nest_orders_form` | 1 ok |

## Metingen

`Packing::Tight` tegen `Grid` op een realistische plaat: 600 x 400 mm, tussenruimte 3 mm, rand 5 mm, kwartslagen, gemengde onderdelen
(L-beugel 70x50, schijf Ø40, sleuf 60x20, driehoek 50x45, plaatje 90x35, ster 56 mm; 39 stuks per "set"). Release,
`cargo test --release -p burnburn-cam --locked --test nest_tight_measure -- --ignored --nocapture`, één machine (arm64), één run:

| Stuks | Stap | Grid | Tight |
|---|---|---|---|
| 39 | 2 mm | 39/39 geplaatst, 27,0 %, bereik 592 x 200 mm, 0,01 s | 39/39, 27,0 %, bereik 593 x 190 mm, 0,80 s |
| 39 | 1 mm | 39/39, 27,0 %, 594 x 193 mm, 0,08 s | 39/39, 27,0 %, 595 x 194 mm, 0,62 s |
| 78 | 2 mm | 78/78, 54,1 %, 594 x 364 mm, 0,03 s | 78/78, 54,1 %, 595 x 344 mm, 6,72 s |
| 117 | 2 mm | 76/117, 62,0 %, 0,07 s | 85/117, 66,2 %, 5,35 s |
| 156 | 2 mm | 64/156, 63,1 %, 0,04 s | 73/156, 68,1 %, 3,39 s |

Lezing: zolang alles past, geeft Tight geen hoger percentage maar wel een kortere bereik (10-20 mm bij 78 stuks); als de plaat vol
raakt, past Tight 9 stuks meer (+4 tot +5 procentpunt benutting). De prijs is 40x tot 200x de rekentijd van Grid (tot 7 s op 78 stuks).
De tijd is dus voor de bediening acceptabel als achtergrondtaak met annuleren, niet voor elk beweging in het paneel; het blijft een
bewuste keuze (standaard uit). Eén machine, één fixture: geen statistiek, geen echte plaatdata van de werkplaats.

W06 op cameraformaat (1280 x 720, sjabloon 87 x 87, release, `cargo test --release -p burnburn-image --locked --test g8_locate_pose --
--ignored --nocapture`): alleen translatie 0,03 s; rotatie met stap 15° 0,38 s; rotatie stap 10° met 3 schalen 1,76 s. In de panelen is
de stap 10° met 3 schalen (ongeveer 1,8 s op een 1280x720-beeld op deze machine); de zoekactie is een annuleerbare taak.

## Ontwerpkeuzes en aannames

- **Common-line, IM32.** In de volgorde die `line::plan` maakt (binnenwerk eerst, daarna omtrek) houdt het *laatst* gesneden deel de gedeelde
  lijn. Dat voldoet aan "na het binnenwerk van beide" zolang `inner_first` aan staat (standaard); met `inner_first` uit is de volgorde
  van de gebruiker en geldt alleen "de laatste vorm". `Feature::CommonLine` blijft `Unverified`: RD/GRBL weigeren zo'n plan nog. De
  documentbrede Remove Overlapping Lines houdt de eerste, ongewijzigd (C12-gedrag).
- **Nerf per onderdeel** is een keuze uit drie, geen vrije hoek: "volgt" (X-as langs de nerf), "dwars" (Y-as langs de nerf, deel-grain
  90°) en "vrij". Zonder nerf op de plaat doen de keuzes niets en worden ze niet getoond.
- **Pagina's, geen scrollbalk.** Het paneel is een vaste vorm van generieke regels; paginering is hetzelfde op macOS en Windows en
  headless te testen. De rij houdt het onderdeelnummer van de héle lijst (`NestPartCopies(i)`), niet van de pagina.
- **Kopienamen** hebben het volgnummer van de kopie (kopie 1 = origineel houdt zijn naam, kopie 2 heet "Naam (2)"); een bestaande naam
  wordt overgeslagen zodat geen twee objecten één naam hebben.
- **Rotatie/schaal-zoeker.** Elke pixel van het sjabloon wordt opgezocht in het beeld op de plek waar hij zou liggen als het product
  gedraaid en geschaald was (bilineair): het sjabloon wordt nooit geresampled. Grof zoeken op een verkleind beeld per (hoek, schaal),
  verfijnen met een patroonzoek over positie (tot ¼ px), hoek (tot ½°) en schaal. Hoek in (-180°, 180°], positief = met de klok mee op
  het scherm (y omlaag); schaal = grootte van het product in het beeld ten opzichte van het sjabloon. Een pose waarvan het gedraaide
  sjabloon buiten het beeld valt wordt niet bekeken. Score = dezelfde genormaliseerde kruiscorrelatie; nog steeds een gelijkenis, geen
  nauwkeurigheid in mm.
- **Plaatsen met draaiing.** De hoek in het beeld wordt via de kalibratie naar de bedhoek omgerekend (twee richtingsvectoren door
  `pixel_to_bed_at_height`), dus een gespiegelde y-as keert het teken om; de kopie van de selectie wordt om het midden van de selectie
  gedraaid, met dezelfde factor geschaald en op de plek gezet. Pure translatie blijft het oude pad (`Paste` met offset). Een geschaalde
  kopie gaat ervan uit dat het product werkelijk groter is (zelfde hoogte), niet dat de camera dichterbij stond.
- **Sjablonen bewaren.** Een sjabloon met een naam wordt vastgelegd als 8-bit grijs (hex per pixel) met de exacte cameraconfiguratie;
  het leert zelf al naar 8-bit, zodat een teruggelezen sjabloon identieke treffers geeft (test). Een onleesbaar of nieuwer bestand
  blijft ongemoeid en er wordt niets in geschreven; leren werkt dan nog wel. Schema 1 met `deny_unknown_fields`, zoals de andere
  bibliotheken; een nieuw schema is een getypeerde fout, geen lege bibliotheek.
- **Wijzen** gebruikt de canvasaanwijzer (zoals T03): het beeld ligt op het blad met 0,1 mm per pixel, het zicht zoomt erop. Een klik is
  een hoek, een sleep is twee hoeken; een sleep korter dan 4 px telt als klik. Het vak wordt op hele pixels afgerond.
- **Orders en nest.** "Nog te maken" = aantal min voltooide uitvoeringen met bewijs (niet uit uploads). Een onderdeel heet zoals de order
  het noemt als één van zijn objecten die naam heeft (hoofdletters en spaties genegeerd). Een aantal gelijk aan dat van het paneel wordt
  geen eigen aantal.
- **Order vindt job.** De namen van de onderdelen komen uit het bevroren project van de job (de historie kan het sinds `load_project`
  teruggeven, gecontroleerd tegen de project-hash); de bestandsnaam werkt nog. Een naam die in meer dan één vrijgegeven job voorkomt
  vraagt de keuze van de operator.

## Open / onbevestigd

- **Handmatig**: uiterlijk van de nieuwe paneelregels (pagina's, keuzes, ordersectie, herkenningspaneel met keuzes en lijst), het
  daadwerkelijk wijzen van een vak met de muis op het canvas (alleen via de headless aanwijzer geoefend) en hoe het beeld op het blad
  er voor een operator uitziet.
- **Native**: geen nieuwe native AppKit-run; de nieuwe regels zijn generieke paneelregels (zelfde widgets als de bestaande). Windows:
  alleen headless formulieren, typecheck en clippy voor `x86_64-pc-windows-msvc`.
- **W06**: geen herkenning van gespiegelde producten; geen perspectief; schaal is uniform; een product dat deels buiten het beeld ligt
  wordt niet gevonden; de snelheid is op één synthetische fixture en één machine gemeten.
- **Common-line**: nog steeds niet bewezen op een machine (RD en GRBL weigeren het plan).
- **Tab Cut Power**: de plan-blob bewaart het nu, maar RD/GRBL-uitvoer is niet bewezen (ongewijzigd).
- **Tight**: gemeten op één synthetische plaat, geen echte werkplaatsdata.
- **Hardware**: geen laser of camera gecontacteerd.
