# Release: installeren, bijwerken en herstellen (story X07)

Besluit B-9 (`docs/design/open-beslissingen.md`): **geen netwerk-updatecontrole in v1**. BurnBurn zoekt zelf nooit naar
updates en heeft geen HTTPS/TLS. Een update is altijd een door de gebruiker verkregen nieuw installatiebestand.

Legenda: **✔ getest** = door een geautomatiseerde test of een echte opdracht op deze Mac uitgevoerd (bewijs in
`docs/dev-notes/W4-release.md`); **handmatig** = niet uitgevoerd, moet een mens (of een runner die we hier niet hebben) doen;
**nooit gedraaid** = geschreven maar niet uitgevoerd.

## Wat de build maakt

| Platform | Opdracht | Resultaat | Status |
|---|---|---|---|
| macOS | `cargo xtask bundle --release --zip --dmg --sign-adhoc` | `target/release/BurnBurn.app`, `target/release/dist/BurnBurn-<versie>-macos-<arch>.{zip,dmg}` + `.sha256` | ✔ getest (echte run, zie dev-note) |
| Windows | `cargo xtask msix-layout --exe <burnburn-win.exe> --out <map>` | map met `AppxManifest.xml`, `Assets/*.png`, `burnburn-win.exe`; daarna `makeappx pack /d <map> /p BurnBurn.msix` | manifest en mapindeling ✔ getest (headless); `makeappx`, ondertekenen, installeren **handmatig** |
| Beide | `.github/workflows/release.yml` (handmatig of tag `v*`) | dezelfde opdrachten op runners | **nooit gedraaid** |

Versie: de werkruimteversie in `Cargo.toml` (`[workspace.package] version`). macOS gebruikt hem voor
`CFBundleShortVersionString`/`CFBundleVersion` (✔ getest); MSIX maakt er `major.minor.patch.0` van (✔ getest; een
pre-release-achtervoegsel valt weg). Minimale OS-versies: macOS 13 (`LSMinimumSystemVersion`, ✔ getest in de plist),
Windows 10 22H2 (build 19045, `TargetDeviceFamily MinVersion`, ✔ getest in het manifest; B-11).

Reproduceerbaarheid: de `.zip` is byte-voor-byte gelijk bij herhaald bundelen van dezelfde binary (bestandstijden uit
`SOURCE_DATE_EPOCH`, standaard 2024-01-01; ✔ getest, 3 echte runs gaven dezelfde SHA-256). De `.dmg` is dat niet (het
formaat bevat identifiers). Dat de *binary* zelf reproduceerbaar uit cargo komt (pad-onafhankelijk, andere machine) is
**niet onderzocht**.

## macOS

### Installeren
1. Download `BurnBurn-<versie>-macos-<arch>.dmg` (of `.zip`) en het bijbehorende `.sha256`.
2. Controleer: `cd <downloadmap> && shasum -a 256 -c BurnBurn-<versie>-macos-<arch>.dmg.sha256`. ✔ getest (`OK`).
3. Open de dmg en sleep `BurnBurn.app` naar Programma's (bij de zip: uitpakken en verplaatsen). Het image zelf is ✔ getest
   (`hdiutil verify`); het slepen en starten op een schone Mac is **handmatig**.
4. Eerste start: de app is alleen **ad hoc** ondertekend (`codesign -s -`; ✔ getest met
   `codesign --verify --deep --strict`), niet genotariseerd. Gatekeeper vraagt daarom bevestiging: rechtsklik › Open.
   Dat gedrag is **handmatig** te bevestigen.
5. `.burnburn`-bestanden openen met BurnBurn: documenttype en exported UTI staan in de Info.plist (✔ getest, `plutil -lint`
   schoon); dat Finder het bestand daadwerkelijk koppelt is **handmatig**.

### Notarisatie (maintainer, handmatig, geen identiteit in deze repo)
Met een Developer ID: `codesign --force --deep --options runtime --sign "Developer ID Application: …" BurnBurn.app`,
daarna `xcrun notarytool submit … --wait` en `xcrun stapler staple`. **Nooit uitgevoerd.**

### Bijwerken (zonder netwerk)
1. Haal de nieuwe dmg/zip op en controleer de checksum (✔ getest, zie boven).
2. Beslis eerst of bijwerken nu mag: de zuivere beslisfunctie `burnburn_runtime::update_gate::decide_update` weigert zolang
   er een upload, opgeslagen of lopende job, onzekere uitkomst of live machinebediening is (✔ getest). **De UI toont dit
   nog niet**: tot dan geldt de regel handmatig: sluit de app alleen als het Laservenster geen actieve job toont.
3. Sluit BurnBurn, vervang `BurnBurn.app` in Programma's (**handmatig**). Projecten, profielen en instellingen staan buiten
   de app (`~/Library/Application Support/BurnBurn`), dus blijven staan.
4. Open een project met de nieuwe versie. Is het van een ouder schema, dan wordt het in het geheugen gemigreerd; bij het
   eerstvolgende Opslaan is er een **herstelbare kopie** (zie Herstel).

### Herstel
- *Project van een nieuwere versie*: opent niet; de melding noemt beide schemaversies en zegt BurnBurn bij te werken.
  Het bestand wordt niet aangeraakt (✔ getest: `update_backup::opening_a_newer_project_names_the_problem_and_the_way_out`,
  `io::load_validation::newer_schema_is_reported_and_file_left_untouched`).
- *Opslaan over een bestand van een oud schema (migratie)*: eerst een byte-identieke kopie `<naam>.burnburn.schema<N>.bak`
  naast het origineel, nooit overschreven (volgende krijgt `.2`, `.3`…); lukt de kopie niet dan blijft het origineel staan
  en wordt er niet opgeslagen. Onleesbaar bestand → `<naam>.burnburn.unreadable.bak`. Een nieuwer bestand op schijf wordt
  door een oudere build niet overschreven. ✔ getest met het fake bestandssysteem (inclusief stroomuitval na de kopie).
  Terugzetten: hernoem de `.bak` (zonder `.schemaN.bak`) en open met de bijbehorende oudere BurnBurn.
  **De opslaan-route in de app gebruikt dit nog niet** (`save_project_keeping_previous` bestaat; aansluiting staat in
  `docs/plan/integratie-wachtrij.md`).
- *Mislukte update*: oude app terugzetten uit de bewaarde oude dmg/zip. Projecten blijven intact (zie boven).
- *Crash tijdens werk*: bestaande recovery-bestanden (`~/Library/Application Support/BurnBurn/Recovery`, IM04) – niet in dit
  pakket gewijzigd.

## Windows (10 22H2 of nieuwer)

### Bouwen van het pakket
1. Op een Windows-machine met Rust en de Windows SDK: `cargo build --release --locked -p burnburn-windows --bin burnburn-win`.
   **Nooit gedraaid** (alleen typecheck/headless tests op de Mac; CI-job `windows-host` nooit gedraaid).
2. `cargo xtask msix-layout --exe target\release\burnburn-win.exe --out target\msix-layout --publisher "CN=<onderwerp van je certificaat>"`.
   Manifest en mapindeling ✔ getest (headless); de opdracht zelf op Windows **nooit gedraaid**.
3. `makeappx pack /d target\msix-layout /p BurnBurn.msix`, daarna `signtool sign` met een certificaat dat bij `Publisher` past.
   **Handmatig, nooit gedraaid.** Het standaard-`Publisher` (`CN=BurnBurn Development`) is een plaatshouder.

### Installeren
Dubbelklik op het ondertekende `.msix` (of `Add-AppxPackage`), of installeer via de App Installer. **Handmatig.** Voor een
zelfondertekend pakket moet het certificaat eerst vertrouwd worden (beheerder). Het manifest registreert `.burnburn`
(bestandstype, pictogram) en vraagt `runFullTrust`, `privateNetworkClientServer` (UDP naar de lasercontroller) en `webcam`
(cameraplaatsing). Of de Windows-host een geopend bestand als opdrachtargument verwerkt, is nog **niet gebouwd** (zie wachtrij).

### Bijwerken
Zonder netwerkcontrole: installeer een `.msix` met een hoger versienummer over de bestaande (zelfde `Identity Name` en
`Publisher`); Windows bewaart de gebruikersdata van het pakket. Zelfde regel als op macOS: niet bijwerken tijdens een actieve
job (`update_gate`, ✔ getest; UI-koppeling open). Dat een upgrade-installatie werkt is **handmatig**.

### Herstel
Projectherstel is platformonafhankelijk (zie macOS: `.bak`-kopieën en nieuwer-schema-weigering, ✔ getest met fake fs).
Terug naar een vorige versie: `Remove-AppxPackage`, installeer de oude `.msix`. Recovery-bestanden: `%LOCALAPPDATA%\BurnBurn\Recovery`.
Alles op Windows is **handmatig**.

## Open / niet gedaan
- Notarisatie, Developer-ID-ondertekening, MSIX-ondertekening: handmatig, geen identiteit aanwezig.
- Installatie, upgrade en verwijderen op een schone Mac/Windows-machine; Gatekeeper/SmartScreen-gedrag.
- Opslaan-route en updatemelding in de apps aansluiten op `save_project_keeping_previous` en `decide_update`.
- Migratie met een kopie voor andere opslag dan projecten (materiaalbibliotheek, kunstbibliotheek, bundels) – die hebben
  eigen schemaweigering maar nog geen `.bak`-route.
- Linux: geen release-route (geen doelplatform).
