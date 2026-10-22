# G6-editor (golf 6): E10, S01/S05, P08, X01, LOADFILE

Lane G6-editor. Basis 481966e. Alleen werkelijk uitgevoerde commando's. Toolchain 1.99.1, `--locked`,
`CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/g6-editor`, `CARGO_INCREMENTAL=0`. macOS arm64; geen hardware, geen
netwerk. Een red-fase telt alleen als de test na een minimale stub op gedrag faalde. Waar er geen aparte rode run is
(stap en test in één keer geschreven, of een bewaker die vanaf het begin groen is) staat dat erbij.

## Wat er bedienbaar is

| Feature | macOS | Windows |
|---|---|---|
| Ellips in een schets (S01): ellips-tools (`sketch.ellipse`, `sketch.ellipse.box`) in schetsmodus, kiezen, relaties (horizontaal/verticaal op de hoofdas, evenwijdig, loodrecht, concentrisch), Maat = diameter van de hoofdas | via de bestaande schetsmodus | idem |
| Vertakkingswissel zichtbaar (S05): "Naar de andere kant gesprongen: n" naast de vrijheidsgraden in het schetspaneel | `panels/sketch.rs` (gedeeld) | idem (formulier gebruikt `freedom`) |
| Doos of scharnier (E10): paneel met soort, velden, deksel, kerf-notitie, voorbeeld op het canvas, Voeg in / Pas toe; menu Tekst, naast Streepjescode…: Doos of scharnier…, Bewerk generator… | `G5Panel::Generator` (zwevend, opent vanzelf) | `FormKind::G5(Generator)` |
| Streepjescode als object met stabiele key (E10): dialoog maakt een `symbology`-generator; Bewerk generator… opent de dialoog opnieuw (knop "Toepassen") | bestaand blad | bestaand formulier |
| Projecten zoeken (P08): Archief ▸ Zoek projecten…, map + zoekterm, resultatenlijst met datum, voorbeeldplaatje, revisies, open project / open revisie naast het ontwerp | `G5Panel::Search` (+ nieuw `G5Line::Image`) | `FormKind::G5(Search)` (+ nieuw `FormLine::Image`) |
| LOADFILE vervangt het schone actieve venster; `HostError::Dirty` | `automation_host.rs` (gedeeld) | idem |
| X01 offline-bewijs | `scenarios/tests/x01_offline.rs` | idem (platformonafhankelijk) |

## LOADFILE vervangt het schone venster + HostError::Dirty

| Test | Rood | Groen | Refactor |
|---|---|---|---|
| `runtime/tests/automation::host_failures_are_distinct_replies` (nieuwe rij `HostError::Dirty -> ERR:DIRTY`) | stub: variant bestond, gemapt op `Refusal::Failed`: `cargo test -p burnburn-runtime --locked --test automation host_failures` -> FAILED (`left: "ERR:FAILED" right: "ERR:DIRTY"`) | map op `Refusal::Dirty` -> 16 ok | |
| `macos/tests/automation_host::a_race_with_a_change_after_the_status_check_is_dirty_not_failed` | `Err(Failed)` i.p.v. `Err(Dirty)` (`cargo test -p burnburn-macos --locked --test automation_host`: 2 FAILED, 13 ok) | pomp stuurt `HostError::Dirty` -> 15 ok | |
| `macos/tests/automation_host::loadfile_replaces_the_clean_active_window` | 2 vensters i.p.v. 1 (zelfde run) | `Stage::Opening { replaces }`: na `OpenWindow` een `UiIntent::Close` van het schone oude venster (alleen als het nog schoon is) -> 15 ok | |
| native `tests/native/macos/automation.rs` (`automation_native`) | de bestaande assert "venster erbij" faalde na de wijziging (`cargo test -p burnburn-macos --test automation_native`: FAILED "a window for the project") | assert omgezet naar "aantal vensters gelijk, oud venster weg, één Open + één Close" -> ok (echte vensters) | |

Ontwerp: een venster dat tijdens het laden vuil werd blijft staan (geen bewaar-blad, geen verlies); het nieuwe project
krijgt dan een eigen venster. De Windows-host gebruikt dezelfde `automation_host`-pomp.

## S01 ellips in een schets; S05

Ontwerp: `Entity::Ellipse { center, major, minor }` (document): middelpunt en hoofdas-eindpunt zijn gewone schetspunten,
`minor` is een waarde (> 0). De solver ziet een ellips als zijn hoofdas (een lijn middelpunt-eindpunt), dus
horizontaal/verticaal/evenwijdig/loodrecht/lengte werken op de hoofdas; Radius/Diameter op een ellips sturen de hoofdas
(halve/hele lengte); Concentric(ellips, X) wordt Coincident van de middelpunten. Punt-op-ellips, raakt en gelijk met een
ellips worden door het document geweigerd (`WrongEntityKind`). Uitvoer: `Path::ellipse` met libm-hoek (DR-13).

| Test | Rood | Groen |
|---|---|---|
| `sketch/tests/ellipse.rs` (10: bounds, rotatie, constructie, horizontaal, straal, diameter+gemeten, concentrisch met cirkel, afwijzingen, minor ongeldig, afstand) | stub = entity + canonical (compileplicht), `lower()` weigerde: `cargo test -p burnburn-sketch --locked --test ellipse` -> 8 FAILED, 1 ok (`Invalid` / `None`) | 9 ok na `lower`/`output`/relation-mapping; diameter-test later toegevoegd, ook ok (10) |
| `io/tests/sketch_ellipse_roundtrip.rs::an_ellipse_survives_save_and_reopen` | ellips werd bij opslaan weggelaten: `Invalid("... no sketch entity 2")` | `Shape::Ellipse` in dto + map -> 2 ok. `the_project_hash_covers_the_minor_radius` was meteen groen (canonical was al nodig om te compileren) |
| `editor sketch::draw::tests::{a_wide_ellipse…, a_tall_ellipse…, a_rotated_ellipse…, an_ellipse_centred_on_an_existing_point_shares_it, a_round_ellipse…}` | `cargo test -p burnburn-editor --locked --lib sketch::draw` -> 5 FAILED (`Unsupported`) | 18 ok; daarbij een echte fout gevonden: kwartslag gaf x = 3e-16 (niet verticaal) -> `snap` op sin/cos |
| `editor sketch::pick::tests::an_ellipse_is_picked_on_its_outline_not_in_its_middle` | `None` | Sampson-afstand (`ellipse_distance`) -> ok |
| `editor tool::constraint::tests::an_ellipse_is_an_axis_and_a_centre_but_no_circular_curve` | Horizontal op ellips gaf `None` | `PickKind::Ellipse` + tabel -> ok |
| `editor sketch::overlays::tests::an_ellipse_is_highlighted…` | "the ellipse is highlighted" faalde | `ellipse_path`, anker, maatlijn -> ok |
| `editor tool::dimension::tests::clicking_an_ellipse_dimensions_its_major_axis_as_a_diameter` | geen aparte rode run (test en code samen geschreven) | ok |
| `presentation w4_sketch::an_ellipse_drawn_in_sketch_mode_becomes_an_ellipse_entity_that_relations_and_dimensions_drive` | eerste deel (tekenen) was meteen groen via de editor-wijziging; tweede deel (Maat op ellips via paneel): constraints 1 i.p.v. 2 | `Kind::Ellipse` in presentation -> 13 ok |
| `presentation sketch_mode::a_shape_a_sketch_cannot_hold_is_refused…` | bestaande test gebruikte de ellips als voorbeeld van "niet ondersteund": FAILED (revisie 2 i.p.v. 1) | omgezet naar een sleuf (lijnen+bogen in één pad) -> 26 ok. Bewust gewijzigd gedrag |

S05 (`branch_flip_is_reported_not_silent`):

| Test | Rood | Groen |
|---|---|---|
| `sketch/tests/branch_flip.rs::{a_solution_on_the_drawn_side_reports_no_flip, branch_flip_is_reported_not_silent}` | geen rode run: de solver rapporteerde de wissel al (`Evaluation::flipped`, `RadiusSign`); de test pint het contract | ok |
| `sketch/tests/branch_flip.rs::a_flipped_tangent_circle_keeps_a_positive_radius_and_its_output` | echte bug: bij een wissel kwam de straal negatief terug en `outputs()` faalde (document: `InvalidDefinition`, geen uitvoer) -> `cargo test -p burnburn-sketch --locked --test branch_flip`: FAILED | straal `abs()` in de opgeloste schets -> 3 ok |
| `presentation w4_sketch::a_geometric_branch_change_is_visible_in_sketch_mode` | `branch_flips` 0 i.p.v. 1 (twee varianten van de test: eerst na heropenen, dan met een echte maatwijziging) | `Analysis` bewaart de vorige oplossing; de nieuwe revisie wordt ervan uit opgelost en geeft `branch_flips` -> ok |
| `macos/tests/sketch_panel::a_branch_change_is_said_in_the_panel_next_to_the_freedom` | `freedom` = "Vrijheidsgraden: 4" zonder melding | `HostText::SketchBranchFlipped` -> 13 ok |

Beperking: de melding is vluchtig (hoort bij de laatste wijziging; na heropenen niet meer). Er is bewust niets in het
bestandsformaat veranderd (golden hashes).

## E10 generatoren

Domein (`burnburn-generators`): evaluator kent nu ook `symbology` (alle 29 types via `stable_name`) en `living-hinge`
(één object met alle open sneden, sleutel 0). Fouten noemen de parameter (`payload`, `module`, `quiet`, `height`, …).

| Test | Rood | Groen |
|---|---|---|
| `generators/tests/evaluator_g6.rs` (5) | `cargo test -p burnburn-generators --locked --test evaluator_g6` -> 4 FAILED (`InvalidDefinition`), 1 ok | 5 ok |
| `presentation/tests/g6_generators.rs` (10) | stub-controller die niets deed: 7 FAILED, 1 ok (negatieve test) | 8 ok; daarna toegevoegd en rood gezien: `a_whole_generated_box_moves_through_its_placement…` (verplaatsen deed niets: "-272.5 -> -272.5") en `…rotates…but_is_not_scaled` (schalen werd geaccepteerd: revisie 3 i.p.v. 2) -> 10 ok |
| `presentation/tests/barcode_dialog.rs` | bestaande tests (CreatePath-label; schaalwaarschuwing via inspector) faalden na de omschakeling naar een generator-object: bewuste gedragswijziging | `create_makes_one_barcode_object_…` (label `Generator(Create)`), `a_generated_barcode_changes_size_through_its_module_and_the_limit_holds`, `a_generated_barcode_opens_again_and_keeps_its_object_when_type_and_payload_change` -> 10 ok |
| `macos/tests/g6_panels.rs` (generator, 6) | stub zonder regels: 6 FAILED | 6 ok |
| `windows/tests/g6_forms.rs` (generator, 3) | dezelfde code, in de Windows-crate gecompileerd: geen aparte rode run | 3 ok |
| native `tests/native/macos/g6_panels.rs::generator_panel_previews_refuses_inserts_and_opens_again` | geschreven na de wiring; eerste run faalde op taal (UI is Nederlands in de native sessie: test accepteert "at least" of "minstens") | ok (echt venster, `harness = false`) |
| `windows/tests/menus.rs` (bestaand, oracle = Mac-bron) | Mac-menu kreeg twee items: `every_popup_has_the_macos_items_in_the_macos_order` FAILED | Windows-tabel erbij -> 8 ok |

Ontwerpkeuzes:
- **C-8 en verplaatsen.** Afgeleide objecten kunnen niet direct bewerkt worden; een barcode of doos moet toch verplaatst
  kunnen worden. `apply_command` (presentation) vertaalt een *rigide* `Transform` (verplaatsen, draaien, spiegelen) van
  alle uitvoerobjecten van één generator naar `GeneratorCommand::Set` met een nieuwe `placement`. Schalen of een deel van
  de panelen wordt nog steeds geweigerd: de grootte is de parameter (module van de code, millimeters van de doos).
- **Kerf.** De dikte is de pasvorm van de vingers; de generator past nooit kerf toe. Het paneel zegt dat; dat de kerf
  precies één keer wordt toegepast bewijst het bestaande `scenarios::gate_d::headless_gate_d_box_thickness_change_no_double_kerf`
  (ongewijzigd groen); `a_generated_object_opens_again_and_a_thickness_change…` pint dat de buitenmaten van de panelen
  bij een diktewijziging gelijk blijven.
- **Naam.** De naam van een uitvoerobject wordt alleen bij het maken gezet (document: een hernoeming overleeft herberekening),
  dus een barcode die van type verandert houdt zijn oude naam; de geometrie volgt de definitie.
- De schaalwaarschuwing van de streepjescodedialoog (`barcode_scale_warning`) vervalt: schalen kan niet meer.
- Het formaat van het hinge-object is één pad met alle sneden (niet één object per snede).

## P08 projecten zoeken

Domein (`burnburn-io`, additief): `FileSystem::modified` (standaard `None`; Std en Memory implementeren), `IndexEntry.{modified,
outline}` (`MAX_OUTLINE_OBJECTS` = 400). Presentation: `FilesIntent::{OpenSearch, CloseSearch, SetSearchFolder, SetSearchQuery,
RescanSearch, OpenFoundProject, OpenFoundRevision}`, `FilesViewModel.search`, `CommandKey::SearchProjects`, worker-taken
`FilesTask::{SearchProjects, OpenRevision}`.

| Test | Rood | Groen |
|---|---|---|
| `io/tests/search_g6.rs` (3) | stub (velden leeg): `cargo test -p burnburn-io --locked --test search_g6` -> 2 FAILED, 1 ok (`indexing_only_reads` pint het bestaande contract: meteen groen) | 3 ok |
| `presentation/tests/g6_search.rs` (7, o.a. `reopening_old_revision_creates_no_production_intent`) | stub-controller: 7 FAILED | 7 ok |
| `macos/tests/g6_panels.rs` (zoeken, 4) | stub zonder regels: 4 FAILED (en de titel van het commando) | 10 ok in totaal |
| `windows/tests/g6_forms.rs` (zoeken, 2) | zelfde code in de Windows-crate: geen aparte rode run | 5 ok in totaal |
| native `tests/native/macos/g6_panels.rs::search_panel_finds_a_project_and_opens_an_old_revision_next_to_the_design` | geschreven na de wiring, geen rode run | ok (echt venster, echte schijf in een tijdelijke map) |

`reopening_old_revision_creates_no_production_intent`: tellende `HistoryStore` + takenlog: na zoeken en een oude revisie
openen is er geen enkele `record_*`-aanroep en geen Compile/PrepareJob/Export/TestJob-taak; het projectbestand is
byte-identiek; de kopie heeft een nieuwe uuid en geen pad (Opslaan vraagt een locatie).

Beperkingen: de datum is de bewaartijd van het bestand in UTC (de schijf kent geen tijdzone); een bevroren revisie heeft
zelf geen datum in het bestandsformaat (alleen nummer en label); de map typ je in (geen map-kiezer); de zoekterm werkt bij
Return/einde bewerken, niet per toetsaanslag.

## X01 volledig lokaal

`scenarios/tests/x01_offline.rs` (6). Dit zijn bewakers: ze waren vanaf het begin groen. Dat ze falen als het misgaat is
aangetoond met een tijdelijke mutatie (een `UdpSocket::bind` en een `https://`-literal in `burnburn-io/src/lib.rs`):
`x01_only_the_machine_transport_and_the_loopback_automation_create_sockets` en
`x01_no_source_file_contains_a_web_address_but_namespaces_and_doctypes` werden FAILED; de mutatie is teruggezet.
- `…controller_needs_a_task_port_a_file_system_and_an_identity_and_nothing_else`: exhaustieve destructurering van `AppPorts`
  (compileert niet meer als er een netwerkpoort bij komt).
- `…open_draw_trace_compile_and_the_local_machine_work_with_no_network_port`: openen, tekenen, traceren, compileren,
  klaarzetten en uploaden naar de scripted lokale controller; alleen de machinegesprekken verlaten de app.
- `…lock_file_has_no_http_tls_resolver_or_socket_crate`, `…only_the_machine_transport_and_the_loopback_automation_create_sockets`,
  `…no_source_file_contains_a_web_address…`.
- `…missing_optional_services_block_no_local_project`: beschadigde bibliotheekbestanden, geen lettertypen/camera/journaal:
  project opent, tekent, compileert, bewaart en heropent; het kapotte bibliotheekbestand wordt niet overschreven.

Niet gedekt: "updates buiten een actieve transfer toegepast" (er is geen updater; de updatepoort bij afsluiten staat in
de wachtrij van G5-machine-rest) en de lsof-controle van echte vensters (bestaat al native: `app_panels_open_no_network_socket`).

## Controles (uitgevoerd)

- `cargo fmt --all` schoon (aparte commit).
- `cargo clippy --workspace --all-targets --locked -- -D warnings` schoon; `cargo clippy -p burnburn-windows --target
  x86_64-pc-windows-msvc --all-targets --locked -- -D warnings` schoon.
- `cargo test -p burnburn-macos -p burnburn-windows --locked --no-fail-fast` (incl. native, echte vensters) groen vóór de
  laatste paneelwijzigingen; de eindstand staat in het eindrapport (`cargo xtask verify`).

## Handmatig / onbevestigd

- Hoe de panelen eruitzien (licht/donker), het voorbeeld van de generator op het canvas en het voorbeeldplaatje in het
  zoekpaneel: niet handmatig bekeken.
- Windows: alleen headless modellen + type-check + clippy op `x86_64-pc-windows-msvc`; de Win32-realisatie van
  `FormLine::Image` (klasse `BurnBurnBitmap` van de paneellaag, registratie via `ensure_registered`) is niet gedraaid.
- Ellips: geen maat voor de kleine as (`minor` is een waarde, geen sturende maat) en geen raken/punt-op-ellips.
- Zoeken in grote mappen: geen limiet op het aantal projecten; de index staat in het geheugen.
