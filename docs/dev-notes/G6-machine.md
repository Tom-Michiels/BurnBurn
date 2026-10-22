# G6-machine: profielbibliotheek + GRBL, M01, W01, W09, C06, C05

Lane van golf 6, basis `481966e`, branch `worktree-agent-a9d4263f8f1c9dff2`. Alle commando's met
`CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/g6-machine`, `CARGO_INCREMENTAL=0`, toolchain 1.99.1, `--locked`
(behalve de eerste run van `burnburn-io` na de nieuwe workspace-regel `burnburn-gcode` bij io: `--offline`, alleen
`Cargo.lock` kreeg een regel, geen nieuwe externe dependency).

Er is geen echte laser of controller gecontacteerd; alle tests gebruiken nagebootste transport/klok/bestandssysteem. Wat
hieronder "rood" heet is een echte assertiefout na een minimale stub (geen compilerfout), tenzij anders vermeld.

## Wat er is (kort)

1. **Profielbibliotheek + GRBL**: de bibliotheek (`profiles.json`) bewaart per machine ook de GRBL-dialectinstellingen
   (nieuwe sectie `settings`, schema 2; zonder instellingen blijft het bestand byte-identiek schema 1). Een bibliotheek-
   profiel kiezen herstelt de dialect (geen Ruida-terugval meer). Een host opent een Ruida- of GRBL-profielbestand
   (`UiIntent::OpenMachineProfile`, menu Archief ▸ Machineprofiel openen…, macOS en Windows); de parser staat nu in
   `burnburn_presentation::profile_file` en wordt door de CLI hergebruikt.
2. **M01**: model en firmware worden door de gebruiker opgegeven (Device Settings, twee rijen), staan in de bibliotheek, in
   het hubvenster (als "door jou opgegeven") en zijn wat de identiteitscontrole voor een upload vergelijkt
   (`ExecutionActor::declare_identity`, `MachinePort::declare_identity`). Een controller-antwoord schrijft ze nooit.
3. **W01**: de goedkeuring is een journaalrecord (`JobApproved`) op de bevroren job; overleeft herstart; dezelfde job opnieuw
   voorbereid is weer goedgekeurd; een gewijzigd plan (ander `JobPlanId`) niet. De operatorlijst toont vrijgegeven jobs uit de
   historie zonder venster (`OperatorJobRow.document: Option<_>`).
4. **W09**: de bewijzen (kerf, scanoffset, pointeroffset) staan in `calibration.json` (eigen bibliotheekbestand, met
   `next_id` zodat een id nooit hergebruikt wordt). Het actieve `MachineProfile` draagt de metingen (`Calibration`: kerf per
   materiaal, scanoffset per snelheid) en dat is deel van de profielrevisie: een nieuwe meting maakt plannen verouderd, een
   bevroren job houdt zijn snapshot. De compiler gebruikt ze en schrijft per gebruikte waarde een `AppliedCalibration` met
   de bewijzen in het `JobPlan` (deel van het plan-id; ook in de plan-blob van de historie).
5. **C06**: per vorm startpunt en/of richting vergrendelen (`PathLock`, `ProcessCommand::SetPathLock`, menu Vormen ▸ Startpunt
   vergrendelen / Richting vergrendelen); de planner gebruikt het.
6. **C05**: grayscale-golden uit een onafhankelijke Python-referentie; preflight-waarschuwing voor pass-through op een
   ongelijk raster.

## Cycli (rood -> groen)

| Onderdeel / tests | Rood (commando, uitkomst) | Groen |
|---|---|---|
| io `g6_profile_library_grbl::*` (9) | `cargo test -p burnburn-io --test g6_profile_library_grbl`: eerst op de stub (alleen `grbl: None`) 5 failed / 1 passed; na de M01-uitbreiding (model/firmware, sectie `settings`) 5 failed / 4 passed | 9 passed; `profile_library` (4) blijft groen na aanpassing van één regel (de schema-bump test gebruikt nu letterlijk schema 1) |
| presentation `g6_profile_grbl::*` (6) | `cargo test -p burnburn-presentation --test g6_profile_grbl`: 6 failed | eerst 4 passed / 2 failed (de bibliotheek werd pas gelezen bij het eerste vervangen; opgelost door `ensure_libraries` in `replace_active_machine`), daarna 6 passed; hele presentation-suite groen |
| machine `g6_declared_identity::*` (5) | `cargo test -p burnburn-machine --test g6_declared_identity`: 5 failed (stub `declare_identity` deed niets) | 5 passed |
| presentation `g6_machine_identity::*` (9) | 6 failed / 3 passed (3 zijn bewakingstests) | 9 passed |
| macOS `g6_machine_identity::*` (4) | 3 failed / 1 passed | 4 passed; hele macOS-suite incl. native tests groen |
| presentation `g6_profile_file::*` (3) | `cargo test -p burnburn-presentation --test g6_profile_file`: 2 failed / 1 passed (stub gaf altijd `Err`) | 3 passed; `burnburn-cli` `gcode`, `cli`, `x05` groen |
| presentation `g6_profile_open::*` (4) | 3 failed / 1 passed (het commando-effect was al aangesloten in de stub) | 4 passed |
| macOS `g6_machine_open_profile::*` (3), Windows `g6_machine_open_profile::*` (2) | geen aparte rode fase (pure aansluiting); wel eerst rood: `macos tests/l10n::all_message_keys_have_nl_and_en` ("missing translations: Command(OpenMachineProfile)") | groen na de teksten, menu-item en dialoog |
| history `g6_approval::*` (6) | `cargo test -p burnburn-history --test g6_approval`: 4 failed / 2 passed | 6 passed |
| runtime `g6_approval::*` (6) | `cargo test -p burnburn-runtime --test g6_approval`: 4 failed / 2 passed | 6 passed (ook: een uit de historie herbouwde job (zonder `inputs`) kan weer geüpload worden; dat faalde in productie met "job has no frozen inputs") |
| presentation `g6_approval::*` (6) | 4 failed / 2 passed | 6 passed |
| job `g6_calibration::*` (9) | `cargo test -p burnburn-job --test g6_calibration`: 7 failed / 2 passed | 9 passed |
| io `g6_profile_calibration::*` (5) | 5 failed (eerste run: 4 passed / 1 failed door een fout in mijn test (dubbele machine-id); test gecorrigeerd) | 5 passed |
| cam `g6_calibration::*` (11) | `cargo test -p burnburn-cam --test g6_calibration`: 6 failed / 5 passed | 10 passed / 1 failed (mijn test gebruikte een profiel dat kerf al ondersteunt), test gecorrigeerd, 11 passed |
| history `g6_plan_calibration::*` (3) | 3 failed (`BlobMismatch`: een bevroren plan met metingen kon niet terug uit de historie) | 3 passed |
| io `g6_calibration_library::*` (7) | 5 failed / 1 passed op de stub; de `next_id`-eis is samen met de implementatie toegevoegd (dus geen aparte rode fase) | 7 passed |
| presentation `g6_calibration::*` (10) | 7 failed / 3 passed | 10 passed |
| document `g6_path_lock::*` (6) | `cargo test -p burnburn-document --test g6_path_lock`: 6 failed | 6 passed |
| io `g6_path_lock_roundtrip::*` (4) | 3 failed / 1 passed | 4 passed |
| cam `g6_path_lock::*` (7) | `cargo test -p burnburn-cam --test g6_path_lock`: 3 failed / 4 passed | 7 passed |
| presentation `g6_path_lock::*` (4), macOS `g6_path_lock::*` (3), Windows `g6_path_lock::*` (1) | presentation: 3 failed / 2 passed (één trivieel slagende test verwijderd); macOS/Windows: aansluiting + l10n | groen |
| scenarios `g6_image_grayscale::*` (2) | bestaand gedrag: geen rode fase. Gevoeligheidscontrole: laagvermogen 80 -> 79 % in de test gaf 2 failed | 2 passed tegen `image-grayscale-golden.txt` |
| cam `g6_image_preflight::*` (6) | `cargo test -p burnburn-cam --test g6_image_preflight`: 3 failed / 3 passed | 6 passed |
| presentation `g6_image_preflight::*` (2), macOS `g6_image_preflight::*` (1) | 1 failed / 1 passed | groen |

Eindcontroles (zie het eindrapport voor de uitkomst van `cargo xtask verify`): `cargo fmt --all`; `cargo clippy --workspace
--all-targets --locked -- -D warnings` schoon; `cargo clippy -p burnburn-windows --all-targets --locked --target
x86_64-pc-windows-msvc -- -D warnings` schoon.

## Ontwerpkeuzes en afwijkingen

- **Bibliotheekformaat**: sectie `settings` per machine-id (`grbl`, `model`, `firmware`), alleen aanwezig als er iets is;
  schema 2 alleen dan (of als een profiel metingen heeft). Een oudere app meldt "nieuwer schema" in plaats van stil de
  instellingen te laten vallen. Lege tekst voor model/firmware is "niet opgegeven".
- **Declaratie is geen profielinhoud**: model/firmware zitten niet in de profielhash; een wijziging alleen daarvan is geen
  nieuwe revisie en maakt geen plan verouderd. De declaratie gaat vlak vóór elke machine-actie naar de poort (alleen als er
  iets is opgegeven, of eerder iets is opgegeven); wat de host zelf configureerde blijft anders gelden. Tijdens een
  actieve uitvoering blijft de oude identiteit gelden.
- **Metingen in het profiel**: `Calibration` staat in `burnburn-job` (laag 2) en `MachineProfile::with_calibration`; lege
  kalibratie laat de hash ongemoeid. `replace_active_machine` past de metingen van de machine toe, dus elk pad (dialoog,
  profielbestand, bundel, kiezen) levert dezelfde uitkomst. `burnburn_machine::CalibratedProfile` (IM29) is niet gebruikt
  en blijft ongewijzigd.
- **Kerf**: alleen voor een Line-bewerking zonder getypte kerf waarvan het recept een materiaal heeft met een meting; een
  getypte kerf wint altijd; dezelfde capability (`KerfCompensation`) als een getypte kerf, anders `UnsupportedFeature`.
- **Scanoffset**: retourrijen van bidirectionele X/Y-scans (en grayscale, X) schuiven `offset` terug langs de machine-as
  (na de oorsprongscorner), bij de gemeten snelheid of lineair geïnterpoleerd tussen twee metingen; buiten het gemeten bereik
  is er geen waarde (geen extrapolatie). Hoekscans krijgen niets. De betekenis van het teken ("de retourrijen liggen
  `offset` verschoven in +as-richting") volgt de bestaande scanproef in `burnburn-machine::calibration`; of dat voor een
  echte machine klopt is niet gemeten.
- **Pointeroffset**: bewaard en getoond (hub), `AppController::pointer_offset_mm()`; geen compiler-gebruik (het raakt waar de
  kop staat, niet wat gebrand wordt) en niet in de profielhash.
- **Plan-blob** (historie) kreeg een optioneel veld `calibration`; zonder dat veld kon een bevroren plan met metingen niet
  meer geladen worden.
- **C06**: richting vergrendelen heeft voor gesloten vormen geen effect (de planner keert gesloten vormen nooit om); een
  vergrendeld startpunt van een open vorm houdt ook de richting vast. Vergrendelen beperkt alleen; het zet de optimalisatie
  nooit aan. Een druk is één stap ongedaan maken tenzij de selectie vormen met verschillende vergrendelingen bevat (dan één
  commando per resulterende vergrendeling).
- **C05 pass-through**: waarschuwing als het aantal doelpunten geen geheel veelvoud van de bronpixels van het (bijgesneden)
  venster is, per as; vraagt de bronpixels (`preflight_with`), zonder bron zegt de preflight niets.
- `docs/design/layers.toml`: `burnburn-io` mag `burnburn-gcode` gebruiken (GRBL-instellingen in de bibliotheek).
  `burnburn-presentation` kreeg `serde` en `serde_json` als gewone dependency (al in de workspace; voor het profielbestand).

## Eerlijk open

- **Niet handmatig bekeken**: de nieuwe Device Settings-rijen, het open-paneel voor profielbestanden, het hubvenster met
  opgegeven model/firmware en de twee Vormen-menu-items zijn op een echt scherm niet gezien (macOS native tests dekken de
  menu-structuur; Windows alleen headless).
- **Hardware**: tab cut power in RD, feeder naar RD/GRBL en controllerinstellingen lezen/schrijven blijven getypeerd
  geweigerd (ongewijzigd, geen bewijs voor een draadvorm). Geen scanoffset- of kerfmeting is op een echte machine gedaan.
- **Plan-paneel**: `PlanViewModel.calibration` bevat de gebruikte waarden met bewijs, maar het plan-paneel (macOS/Windows)
  toont ze nog niet; de bron is dus alleen in het model, de plan-id en de historie te zien.
- **Orders**: een vrijgegeven job uit de historie zonder venster komt wel in de operatorlijst en kan worden geüpload, maar
  telt niet mee als kandidaat voor een order (alleen jobs in een venster doen dat). De naam van zo'n job is de bestandsnaam
  van het project, of "job" voor een project zonder pad.
- **Actief profiel bewaren**: een host bewaart het actieve apparaat zelf niet tussen starts (er is alleen de bibliotheek van
  vervangen profielen); dat is niet veranderd.
- **Projectbestanden met bevroren profiel + metingen** kunnen door een oudere build niet geopend worden (onbekend veld, net als
  bij eerdere additieve velden).
- Een pointer-offsetmeting zelf (rood stipje) wordt nergens toegepast.
- Windows is alleen met typecheck, clippy en headless tests gecontroleerd; de Win32-laag is niet gedraaid.
