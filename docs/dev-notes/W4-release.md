# W4-release (X07): installeren, bijwerken, herstellen

Lane W4-release, golf 4. Alle commando's met `export PATH="$HOME/.cargo/bin:$PATH"` en
`CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/w4-release`, toolchain 1.99.1, `--locked`, macOS arm64.
B-9 gerespecteerd: geen netwerk, geen nieuwe dependency.

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| `runtime::update_gate::tests::{every_unfinished_state_defers_with_its_own_reason, live_machine_control_defers_even_without_an_execution, the_most_urgent_reason_wins_regardless_of_order}` (+ `an_idle_machine_allows_the_update`, `finished_executions_do_not_block_the_update`) | stub `decide_update` geeft altijd `Apply` | `cargo test -p burnburn-runtime --lib --locked update_gate`: 2 passed, 3 failed (`left: None, right: Some(TransferInProgress)` enz.) | zelfde commando: 5 passed | `defer_reason` zonder wildcard-arm, zodat een nieuwe `ExecutionState` een beslissing afdwingt |
| `io/tests/update_backup.rs`: `container_schema_reads_the_manifest_without_loading_the_project`, `saving_over_an_older_schema_file_keeps_a_byte_identical_copy_first`, `an_existing_backup_is_never_overwritten`, `a_newer_file_on_disk_is_refused_and_left_alone`, `an_unreadable_previous_file_is_kept_too`, `opening_a_newer_project_names_the_problem_and_the_way_out` | stubs: `container_schema` gaf 0, `save_project_keeping_previous` bewaart niets, melding noemt geen update | `cargo test -p burnburn-io --test update_backup --locked`: 2 passed, 6 failed | zelfde: 8 passed; `cargo test -p burnburn-io --locked` volledig groen | – |
| idem: `when_the_backup_cannot_be_written_the_original_stays_and_nothing_is_saved`, `a_file_of_the_current_schema_and_a_first_save_need_no_backup` | slaagden al onder de stub (de stub schrijft niets extra) | zie boven (de 2 passed) | blijven groen na implementatie | bewaken alleen de invariant (origineel blijft; geen back-up bij huidig schema) |
| `xtask dist::tests::*` (options, artifact_name, source_date_epoch, tool-argumenten, checksum_line, normalised_times, zip, dmg) | stubs gaven lege/standaardwaarden | `cargo test -p xtask --locked dist::`: 1 passed, 8 failed | zelfde: 9 passed (17 s, `hdiutil`) | – |
| `dist::tests::the_zip_holds_the_bundle_and_is_reproducible_across_builds` (uitgebreid met extended attribute) | `ditto --sequesterRsrc` schrijft `__MACOSX/`-AppleDouble-bestanden met de huidige tijd | gevonden in een echte run (twee zips van dezelfde binary hadden verschillende SHA-256); test uitgebreid met `xattr -w`; rood: 2 failed | `--norsrc --noextattr --noqtn --noacl`: 9 passed | – |
| zelfde test, nu met 2,1 s wachttijd tussen de twee bouwsels, plus `zip_inputs_are_every_path…` en aangepaste `the_system_tool_invocations_are_exactly_what_we_document` | `ditto` schrijft de toegangstijd (Info-ZIP-extraveld 0x5855) van de bestanden in het archief; die verandert bij elke run (de eerste versie van de test was te snel om dat te zien: beide zips binnen dezelfde seconde) | `cargo test -p xtask --locked dist::`: 7 passed, 3 failed | `zip -X -y -q -@` met gesorteerde namen, `TZ=UTC`: 10 passed (6 s) | – |
| `xtask msix::tests::*` (versieomzetting, manifest-inhoud, bestandstype, logo's, escaping, mapindeling, argumenten) | stubs gaven lege waarden | `cargo test -p xtask --locked msix::`: 8 failed | zelfde: 8 passed | – |
| `xtask workflows::tests::*` (checker + echte bestanden) | stub-checker vond niets; `release.yml` en de Windows-job ontbraken | `cargo test -p xtask --locked workflows::`: 1 passed, 4 failed | na checker + workflows: 4 passed, 1 failed (mijn eigen commentaar in `release.yml` bevatte het woord `secrets.`), na tekstwijziging 5 passed | – |
| `bundle::tests::{bundle_version_is_the_workspace_version_from_cargo, info_plist_passes_plutil_lint}` | **geen red-fase**: karakteriseringstests van bestaand gedrag, slaagden direct | `cargo test -p xtask --locked bundle::`: 10 passed | – | – |

## Echte runs op deze Mac (niet in de unit-tests)
- `cargo xtask bundle --release --zip --dmg --sign-adhoc`: bouwt `burnburn`, maakt `.app`, `codesign --force --deep --sign -`,
  zip, dmg en `.sha256` (uitkomst: alle vier de bestanden geschreven).
- `codesign --verify --deep --strict --verbose=2 BurnBurn.app`: `valid on disk` / `satisfies its Designated Requirement`;
  `codesign -dv`: `Signature=adhoc`, `Identifier=app.burnburn.BurnBurn`.
- `shasum -a 256 -c` op beide `.sha256`-bestanden (in de dist-map): `OK`. `hdiutil verify` op de dmg: `checksum … is VALID`.
- Drie opeenvolgende `cargo xtask bundle --release --zip --dmg --sign-adhoc` runs (2 s tussenruimte): zip-SHA-256 steeds
  `c801544362009bd7…`. Uitgepakte zip: `codesign --verify --deep --strict` slaagt, de executable behoudt zijn rechten.
- `cargo xtask msix-layout --out …`: schrijft `AppxManifest.xml` en vier PNG's (version 0.1.0.0).
- CI-bestanden: `ruby -ryaml` (alleen lokaal, niet vastgelegd) laadt `ci.yml` (jobs core, macos-native, windows-host) en
  `release.yml` (macos, windows) zonder fout. Daarnaast de structuurcheck in `xtask/src/workflows.rs` (geen YAML-parser).

## Handmatig
- Installatie van de dmg/zip op een schone Mac, Gatekeeper-gedrag, Finder-koppeling van `.burnburn`, notarisatie.
- Alles op Windows: `makeappx pack`, ondertekenen, installeren, upgraden, bestandskoppeling, SmartScreen.
- Dat `.github/workflows/*.yml` door GitHub geaccepteerd wordt en slaagt: **nooit gedraaid**. Acties zijn op major-tags (`@v4`)
  gepind, niet op commit-SHA.

## Onbevestigd
- Reproduceerbaarheid van de binary zelf buiten deze machine/map; alleen het archiefformaat is bewezen.
- `MaxVersionTested` (10.0.26100.0) en capabilities in het MSIX-manifest zijn niet door de Microsoft-manifestvalidatie gehaald
  (`makeappx` ontbreekt hier); de tests controleren inhoud en well-formedness, niet het XSD-schema.
- De opslaan-route in de apps gebruikt `save_project_keeping_previous` nog niet; de UI toont `decide_update` nog niet
  (wachtrijregels, `docs/plan/integratie-wachtrij.md`).

## Wijzigingen buiten de lane
Additief in `burnburn-io` (nieuwe publieke items `container_schema`, `save_project_keeping_previous`, `SaveOutcome`,
`SaveError::NewerFileOnDisk`; foutmelding van `LoadError::NewerSchema` kreeg de toevoeging "Update BurnBurn to open it") en
`burnburn-runtime` (`update_gate`). `.github/workflows/ci.yml` kreeg een extra job `windows-host`.
