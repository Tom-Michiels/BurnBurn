# G5-native (golf 5: de presentatie-features bedienbaar in beide native hosts)

Omgeving: `export PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/g5-native CARGO_INCREMENTAL=0`,
`cargo ... --locked`, macOS arm64, toolchain 1.99.1. Basis: c6615b1 (`integratie/golf-5`). Geen hardware, geen netwerk
buiten één UDP-socket op 127.0.0.1 binnen het testproces. Een red-fase telt alleen als de test na een minimale stub op
gedrag faalde (niet op een compilerfout).

## Wat er is

Eén headless paneelmodel voor beide hosts: `crates/burnburn-macos/src/panels/g5.rs` (door de Windows-crate via
`#[path]` meegecompileerd, zoals de andere paneelmodellen) met teksten in `l10n_g5.rs`. Het model levert per paneel
gewone regels (`G5Line`: kop, tekst, probleem, memo, veld, keuzelijst, vinkje, knop) en vertaalt een bediende control
met de waarden van het paneel naar precies één `UiIntent` (`g5_intent`; veldtekst ongeparsed, DR-05; een waarde die
presentation al toont verstuurt niets). Beide hosts tekenen alleen die regels.

| Paneel | Opent | macOS (AppKit) | Windows (Win32) |
|---|---|---|---|
| Notities (P04) | vanzelf zodra `ShowNotes` de notities opent; menu Archief ▸ Notities tonen (beide hosts, gewoon `CommandKey`) | `appkit/panels/g5.rs`, zwevend NSPanel met NSTextView, Bewaar/Sluit, tekenteller | `forms/g5.rs` -> `FormKind::G5(Notes)`, `FormLine::Memo` (multi-line EDIT) |
| Offset en reparatie (E05, E09) | menu Venster ▸ Offset en reparatie… (`MenuItem::OffsetPanel`, tag/id 1030) | idem | idem, `PanelState.g5_shown` |
| Centerline-beslissingen (T04, T08) | vanzelf zolang `DocumentViewModel.centerline` bestaat | idem | idem |
| Structuur (S06, S07, P06) | menu Venster ▸ Structuur… (`MenuItem::StructurePanel`, tag/id 1031) | idem, scrollt boven 640 pt | idem |
| Desktop-automatisering (API01) | voorkeur "Desktop-automatisering" | `appkit/app/automation.rs` | `win32/mod.rs` (`automation_step`, `automation_follow`) |

Wijzigingen buiten de nieuwe bestanden zijn additief: `adapter.rs` (twee `MenuItem`s, `NativeEvent::G5Control`,
`Reaction::ToggleG5Panel`), de menutabellen (Mac `appkit/app.rs`, Windows `menus/{file,app}.rs`), `window.rs`, `app.rs`,
`forms/{mod,layout}.rs`, `win32/forms.rs`, `host_panels/image.rs` (knop "Pas toe als trace" in het Windows-tracerpaneel;
die ontbrak), `tests/menus.rs` (twee tokens) en `tests/form_layout.rs` (`..PanelState::default()`).

## Ontwerpkeuzes en aannames

- **Offset en reparatie in één paneel.** Er bestond geen offset- of reparatiepaneel (de opdracht sprak van "bestaande
  panelen"). Beide staan nu in één zwevend paneel met twee kopjes; het menu-item staat in Venster aan beide kanten (de Mac
  heeft daar een eigen lijst `window_menu`, Windows zet ze na de bibliotheken). Voorbeeld/Toepassen sturen de afstand als
  getypt; een gewijzigde hoekvorm of "Bron behouden" gaat als `SetOffsetOptions` (bestaand voorbeeld wordt herberekend).
  Standaardtolerantie voor de reparatie is `NEARLY_CLOSED_MM` (0,5 mm); aanname, LightBurn-gedrag onbekend. Het
  probleem van de laatste geweigerde bewerking (`DocumentViewModel.problem`) staat onderaan het paneel.
- **Centerline naast het tracer-blad, niet erin.** Het tracer-blad is op de Mac een vast (700 x 520) modaal blad en in
  Windows een rij-specificatie; de beslissingen (een knop per koppeling, een knop per te bevestigen primitief) horen bij een
  lijst van variabele lengte. Daarom een eigen zwevend paneel dat vanzelf opent en sluit met het centerline-resultaat. Een
  zwevend paneel blijft bedienbaar terwijl het blad open staat (macOS: bevestigd in de native test). De knop op een keten
  noemt de eerste kandidaat binnen de tolerantie; wat presentation echt bevestigt staat daarna in de tekst "bevestigd als".
- **Notities:** Bewaar is de enige commit (één undo-stap); een tekst boven `max_chars` (20 000) wordt niet verstuurd: Bewaar
  gaat uit en het paneel zegt waarom. Daarvoor krijgt `g5_panel` een `draft` (wat de gebruiker intikte).
- **Structuur:** bewerken per instantie (variant, lokale override), plaatkeuze voor Compile. Een override die gelijk is aan wat
  presentation toont stuurt niets (blur is geen bewerking); een geweigerde override (<= 0) verandert niets en het veld toont
  weer de opgeslagen waarde. Geen UI om onderdelen te definiëren/plaatsen of rand-referenties te maken: het domein biedt
  die UI niet (zie G5-UI-gaten), hier niet bedacht.
- **Windows tag/id:** de twee nieuwe host-items gebruiken 1030/1031 (Mac-tag = Windows-id), bewust ver van 1009+ zodat
  parallelle lanes hun eigen tags houden.
- **Desktop-automatisering.** Luisterthread (`AutomationRuntime`) alleen zolang de voorkeur aan staat, alleen
  `127.0.0.1`, leestimeout 100 ms zodat stoppen binnen ~100 ms klaar is (thread gejoind, poort vrij); bij afsluiten ook gestopt.
  De UI-thread voert elke aanvraag uit via de eigen dispatcher; tijdelijk "uit de host gehaald" tijdens het stappen zodat een
  intent van de pomp zichzelf niet opnieuw start. `LOADFILE` opent in een nieuw venster (zie design-doc, "Open"). Een
  tekening zonder eenheid geeft `ERR:FORMAT` (de preview wordt geannuleerd); een mislukte taak `ERR:FAILED`.
  `HostError` heeft geen `Dirty`-variant: een wijziging tussen service-controle en uitvoering geeft `ERR:FAILED` (runtime-crate
  ongemoeid gelaten).

## TDD-log (echt uitgevoerd)

| Test | Rood | Groen |
|---|---|---|
| `macos/tests/g5_panels.rs` (17 modelstests: notities 2, offset 4, reparatie 2, centerline 3, structuur 5, teksten 1) | stub: `g5_panel` -> `None`, `g5_intent` -> `None`. `cargo test -p burnburn-macos --locked --test g5_panels` -> 15 FAILED, 2 ok (`there_are_no_centreline_decisions_without_a_centreline_trace` en `every_text_has_both_languages_with_the_same_placeholders` slaagden triviaal tegen de stub) | model geschreven; 15 groen. Twee testfouten van mijzelf gecorrigeerd (de vorige rechthoek stond nog geselecteerd; oppervlak is 332,5x i.p.v. 332,6) -> 17 ok |
| `g5_panels::adapter::*` (2: menu-items/tags/titels, controlevent -> intent) | geschreven nadat de adapter-bedrading er al stond: geen rode run | 19 ok |
| `tests/native/macos/g5_panels.rs` (4 scenario's, `harness = false`: notities, offset, structuur, centerline) | geschreven na de AppKit-code; mutatiecontrole: `refresh_g5` tijdelijk laten terugkeren -> `notes_panel_saves_one_undo_step` FAILED ("the note panel is shown"); mutatie teruggedraaid | `cargo test -p burnburn-macos --locked --test g5_panels_native` -> 4 ok |
| `windows/tests/g5_forms.rs` (11) | stub `forms()` leeg, `react()` Nothing: 10 FAILED, 1 ok (`the_panel_items_share_the_macos_tags_…` was al groen, adapter/menu bestond) | 11 ok |
| `windows/tests/w4b_image::the_tracer_offers_apply_as_a_re_editable_trace_next_to_apply` | constante `TR_APPLY_LINKED` zonder knop: FAILED (`expect("the button is there")`) | knop + klik toegevoegd -> 12 ok |
| `windows/tests/menus.rs` (bestaand, oracle = Mac-bron) | – (de Mac-menubron kreeg `ShowNotes`, `OffsetPanel`, `StructurePanel`; de Windows-tabellen volgden in dezelfde stap) | `cargo test -p burnburn-windows --locked` -> alles ok, incl. `every_popup_has_the_macos_items_in_the_macos_order` |
| `macos/tests/automation_host.rs` (13: status, LOADFILE x3, IMPORT x3, machinewerkwoorden, opgegeven job, UI-timeout, luisterthread volgt voorkeur, poort bezet) | stub: pomp deed niets, runtime startte niet: 12 FAILED, 1 ok (de busy-status-test was toen nog leeg; later herschreven met een echte taak en groen gedraaid, zonder aparte rode run) | 13 ok (één test opent echt 127.0.0.1) |
| `tests/native/macos/automation.rs` (`automation_native`: voorkeur aan -> thread, PING/STATUS/IMPORT/LOADFILE/DIRTY/machinewerkwoorden, voorkeur uit -> stilte) | geschreven na de wiring: geen rode run. Eerste groene poging faalde op een te strenge intent-assert van mijzelf (Save en Camera-intents horen er niet bij); aangepast | ok |
| `windows/tests/automation_host.rs` (3) | zelfde code als hierboven, in de Windows-crate gecompileerd: geen aparte rode run | 3 ok |

Controles (uitgevoerd, laatste stand): `cargo clippy -p burnburn-macos -p burnburn-windows --all-targets --locked -- -D warnings`
schoon; `cargo clippy -p burnburn-windows --target x86_64-pc-windows-msvc --all-targets --locked -- -D warnings` schoon;
`cargo test -p burnburn-windows --locked` ok; alle headless macOS-tests ok (zie het eindrapport voor `cargo xtask verify`).

## Handmatig / onbevestigd

- **Geen Windows-run.** `win32/forms.rs` (Memo), `win32/mod.rs` (G5-bedrading, automatisering) en de menu-ids zijn alleen
  type-gecheckt en door clippy gehaald voor `x86_64-pc-windows-msvc`, nooit gelinkt of gedraaid.
- macOS: hoe de panelen eruitzien in licht/donker, de scroll van een hoog structuurpaneel, het sluiten met het sluitvak van
  Offset/Structuur (het paneel meldt zich dan bij de volgende refresh af; alleen logica, geen test), en de voorkeursregel in
  het Voorkeuren-paneel met een echte klik.
- Reparatie voor/na is native niet getest (een open contour met de pen tekenen is niet pixelbetrouwbaar); het model is
  headless gedekt op beide hosts.
- Een echte automatiseringsclient buiten het proces; echte LightBurn-compatibiliteit is niet geclaimd.
