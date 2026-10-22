# G9-editor (golf 9): de software-restpunten van "Editor"

Lane G9-editor. Basis d751627. Alleen werkelijk uitgevoerde commando's. Toolchain 1.99.1, `--locked`,
`CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/g9-editor`, `CARGO_INCREMENTAL=0`. macOS arm64; geen hardware, geen
netwerk, Windows nooit gedraaid (alleen type-check, clippy en headless formuliertests). Een rode fase telt alleen als de test
na een minimale stub (of zonder stub, op een bestaande API) op gedrag faalde; waar die ontbreekt staat dat erbij.

## Wat er bedienbaar is

| Feature | macOS | Windows |
|---|---|---|
| E06 De stapel van de selectie ingebouwd in de inspecteurkolom (onderaan de kolom, scrollt mee; komt in beeld als hij opent) | inspecteurkolom van het documentvenster | sectie "Stapel van de selectie" in het inspecteurformulier (geen apart formulier meer) |
| E06 Lineaire reeks en reeks langs een pad toevoegen (soort 8 en 9 van "Stap toevoegen", ook als nieuwe keten) | Structuurpaneel + de ingebouwde stapel | idem (formulier) |
| E07 Invoegen en wissen midden in de tekst voor meerdere teksten tegelijk | paneel "Tekst bewerken" (opent vanzelf bij een selectie van teksten) | idem (`FormKind::G5(TextContent)`) |
| E08 Een gebonden tekst dwars op de gids verplaatsen = afstand tot de gids; richting uit de echte raaklijn; draaien/schalen/schuinen geweigerd met uitleg; voorbeeldwaarde van variabele tekst naast de sjabloon | canvas-slepen, inspecteur-X/Y, paneel "Tekst bewerken" | idem |
| S07 Functies in formules (`min max abs round floor ceil sqrt pow clamp pi`) | Structuurpaneel (vormgetallen) | idem |
| S07 Vaste vormen met parameters en formules voor hun doos (X, Y, breedte, hoogte) | Structuurpaneel, "Onderdeel bewerken" | idem |
| P06 "Verdeel en nest": plaat kiezen (rijen) en daarna de bestaande nest per plaat | Structuurpaneel, blok "Automatisch verdelen" | idem |
| S01 Ellips-ellipsraak vanuit een diepe overlap; een boog die een ellips raakt | schetsmodus (Raakpunt-tool) | idem |
| S01 Bézier verlengen voorbij 3 parameterlengtes als stabiele kubiek, anders de raaklijn met reden in de statusregel | gereedschap Verlengen | idem |

## E06 de stapel in de inspecteur, reeksen toevoegen

macOS: `appkit/panels/g5.rs::embed_stack` bouwt de kolom om. Wat de kolom al had (transformvelden, status, plan, machine) blijft
op zijn plaats; de kolom krijgt onderaan 340 pt ruimte voor de stapel en wordt het documentview van een `NSScrollView` die bij de
bovenkant begint. De `G5Section` van `G5Panel::Stack` verhuist van zijn `NSPanel` naar die kolom (`embedded`); zijn regels komen
uit hetzelfde headless model als het zwevende paneel (`g5_panel(.., Stack, ..)`) met de titel als eerste regel, in 244 pt breed
(bijschrift 96 pt). Een stapel die verschijnt scrolt de kolom naar zichzelf toe (`scrollRectToVisible`); verdwijnt de selectie,
dan wordt de sectie verborgen. Windows: `forms/inspector.rs` hangt de regels van het Stack-model onder een kopje aan het
inspecteurformulier; `forms::react` stuurt een `ControlId::G5(_)` van dat formulier naar `g5::react(Stack, ..)`; `g5::forms`
maakt geen apart Stack-formulier meer.

Reeksen: `NewModifierStep::{Linear {count, step_x, step_y}, Path {count, orient}}`. Een lineaire reeks wordt op dezelfde grens
getoetst als de andere reeksen (`grid_array` met één rij: aantal 0 of te groot is `InvalidCount`); een padreeks neemt het eerste
andere geselecteerde object als pad (niet de bron, niet de eigen uitvoer van de keten; geen pad: `NeedsOperands`) en toetst met
`path_array`. Bijkomend gevonden: het controlevak "kopieën draaien mee" werd door de Mac-widgetlaag niet als vinkje gelezen
(`is_check` ontbrak) en zou altijd "aan" gezegd hebben; toegevoegd.

| Test | Rood | Groen |
|---|---|---|
| `presentation/tests/g8_modifier_add.rs` (4 nieuwe: lineair, pad, weigeringen, keten met lineair/pad; de wereld kreeg een gids "Pad") | stub (`Err(CommandRejected)`): `cargo test -p burnburn-presentation --locked --test g8_modifier_add` -> 4 FAILED (de oude `a_chain_is_made_on_the_selection...` faalde toen alleen op de nieuwe gids in zijn zoekfilter: aangepast) | 13 ok |
| `macos/tests/g9_editor_series.rs` (3) | stub (alleen de enum-varianten): 3 FAILED | 3 ok (twee orakels van mij hersteld: een ongerapporteerd veld is leeg, niet de getoonde tekst) |
| `windows/tests/g9_editor_series.rs` (2) | geen rode run (na de Mac-bekabeling geschreven) | ok |
| `tests/native/macos/g9_editor_inspector.rs` (2, `harness = false`, stil) | `BURNBURN_NATIVE_QUIET=1 cargo test -p burnburn-macos --locked --test g9_editor_inspector_native` -> 1 FAILED: "the stack control is in a floating panel". Daarna 1 FAILED op mijn eigen meetfout (de stapel lag onder de vouw van de kolom: nu scrollt hij in beeld) en 1 FAILED op een te strenge eis (verborgen widgets bestaan nog: nu `isHiddenOrHasHiddenAncestor`) | 2 ok: de bedieningselementen zitten in het documentvenster (geen `NSPanel`) binnen de rechterkolom, een klik schakelt de stap uit, de stapel gaat met de selectie, soort 8 "Lineaire reeks" voegt een stap toe |
| `windows/tests/g9_editor_inspector.rs` (3), `g8_editor_forms.rs` (stapeltest herschreven) | `cargo test -p burnburn-windows --locked --test g9_editor_inspector` -> 2 FAILED, 1 ok (bewaker: zonder keten geen stapel) | 3 ok |

## E07 invoegen en wissen midden in de tekst voor meerdere teksten

Er was geen cursor of wissen, ook niet voor één tekst: toetsenbordinvoer voegt alleen toe aan het eind. Gekozen: een
**gemeenschappelijke bewerking met één regel**. `TextEdit::InsertAt { position, text }` en `TextEdit::DeleteAt { position, count }`
(ruwe veldtekst, DR-05). Een positie telt tekens (Unicode-scalaarwaarden, geen bytes) vanaf het begin van elke tekst afzonderlijk;
een negatieve telt vanaf het eind; leeg is het eind; voorbij het begin of eind van een kortere tekst is dat begin of eind. Wissen
haalt `count` tekens vanaf de positie weg (minder als er minder zijn). Alle teksten veranderen in één `Compound` (één undo), of
geen: een ongeldig getal laat alles staan en staat in de werkbalk (`TextField::{Position, Count}`); een bewerking die niets
verandert is geen undo-stap. Beperking: een samengesteld teken (letter plus accent als losse tekens) kan zo gesplitst worden;
graphemes zouden een dependency vragen. De werkbalk (`TextToolbarViewModel`) kreeg `contents` en `samples`; het nieuwe paneel
`G5Panel::TextContent` ("Tekst bewerken") toont de inhoud per tekst en heeft velden Positie, Invoegen, Aantal en twee knoppen.

| Test | Rood | Groen |
|---|---|---|
| `presentation/tests/g9_text_content.rs` (10) | stub (varianten die niets doen): `cargo test -p burnburn-presentation --locked --test g9_text_content` -> 9 FAILED; de voorbeeldwaardetest eerst apart rood (`[None, None]`) | 10 ok |
| `macos/tests/g9_editor_text_content.rs` (4) | stub (variant zonder regels): 2 FAILED, 1 ok (bewaker: geen paneel zonder tekst). De regel met de voorbeeldwaarde: geen aparte rode run | 4 ok |
| `windows/tests/g9_editor_text_content.rs` (1) | geen rode run | ok |

## E08 dwars verplaatsen, echte raaklijn, draaien/schalen, voorbeeldwaarde

`generators::guide_tangent_at`: het punt van `guide_frame_at`, de richting is de echte raaklijn van de getekende kromme (de
afgeleide van het segment op de dichtstbijzijnde plaats, in de looprichting; voorbij een uiteinde de raaklijn van dat uiteinde)
in plaats van de eerste koorde van de gevlakte gids. Het orakel is de analytische raaklijn van een cirkel; de koorde zit er
0,022 rad naast, de raaklijn binnen 1e-3 (de booglengte van de vlakke gids is een haar korter dan die van de kromme). Een
gebonden tekst die alleen wordt verplaatst: de component langs de raaklijn is de nieuwe start (begrensd op `[0, ruimte]`), de
component dwars daarop (langs de linkernormaal van de looprichting) is de nieuwe afstand tot de gids (`offset_mm`), in één
`TextCommand::Set`. Draaien, schalen of schuinen van alleen de tekst blijft door het document geweigerd; de weigering heet nu
`MessageKey::BoundTextFollowsGuide` (de regel: de tekst volgt zijn gids; verplaats hem, pas Start en Afstand aan, draai of schaal
de gids samen met de tekst, of maak hem los). Draait of schaalt de selectie de gids mee, dan volgt de tekst. Variabele tekst: het
document (de werkbalk) toont de sjabloon (`contents`) naast de opgeloste voorbeeldwaarde voor de volgende job (`samples`, uit
hetzelfde productierecord als de compile), en het paneel "Tekst bewerken" zet ze naast elkaar ("tekst wordt tekst"). De omtrek op
het canvas blijft de sjabloon gebogen: een omtrek van de voorbeeldwaarde zou selecteren en passen op een ander getal laten werken
dan de job; dat is niet gedaan.

| Test | Rood | Groen |
|---|---|---|
| `generators/tests/g9_guide_tangent.rs` (5) | stub (delegeert naar de koorde): `cargo test -p burnburn-generators --locked --test g9_guide_tangent` -> 3 FAILED, 2 ok (bewakers: rechte lijn en hoek). Eerste implementatie: orakel te streng (1e-4); 1e-3 met de uitleg | 5 ok |
| `presentation/tests/g8_text_path.rs` (5 nieuwe `g9_...`) | `cargo test -p burnburn-presentation --locked --test g8_text_path` -> 4 FAILED, 1 ok (bewaker: samen draaien met de gids werkte al). Daarna 2 FAILED door de afronding van het veld X/Y in de inspecteur (tolerantie 1e-3 resp. 2e-4 gezet; de koorde zit 1,2e-3 ernaast) | 12 ok |

## S07 functies in formules, vaste vormen met parameters, verdelen met nesten

Formules: `Function::{Min, Max, Abs, Round, Floor, Ceil, Sqrt, Pow, Clamp}` plus de constante `pi`, hoofdletter-ongevoelig, bij het
parsen gecontroleerd (`UnknownFunction`, `Arity`; `Domain` bij eval: wortel van een negatief getal, `pow` dat niet eindig is,
`round` met 0..12 decimalen, `clamp` met min > max). Een naam zonder haakje blijft een parameter (`max + 1`); `pi` telt niet als
naam. Argumenten scheiden met `;` of met `,` gevolgd door een spatie; `1,5` is het getal 1,5 (`max(1,5)` is dus één argument:
gedocumenteerd in de test). `PartShape::Fitted { path, x, y, width, height }` (nieuw, `Dim` per getal): de vaste omtrek wordt van
zijn eigen doos naar de doos `x, y, breedte, hoogte` gerekt; een lijn zonder breedte of hoogte past niet (`InvalidValue`); in het
paneel wordt een vaste vorm vanzelf `Fitted` zodra een parameternaam of formule in X, Y, breedte of hoogte komt (een getal blijft
de vorm zelf verplaatsen of schalen); project: `structure_dto::PartShape::Fitted`. "Verdeel en nest" (`StructureEdit::
DistributeAndNestOverPlates { gap }`): hetzelfde rijenplan kiest de plaat (gedeeld met "Verdeel", `plan_distribution`), een
gewone nest-taak (`TaskKind::Nest`, annuleerbaar, revisie-gebonden) legt per plaat de exemplaren neer: de platen liggen naast
elkaar vanaf de oorsprong in de volgorde van de platen, 10 mm uit elkaar (`SHEET_GAP_MM`); alle overige gesloten, zichtbare
objecten zijn in de weg; rotaties kwartslagen, geen spiegelen, afstand = de tussenruimte, rand 0. Het antwoord wordt één
`Compound` (`DistributeOverPlates`): plaat en nieuwe transform van elk exemplaar (`transform.then(plaatsing)`); een exemplaar
dat de nest niet kan plaatsen weigert alles (`NoRoomOnPlates`); een antwoord van een oudere revisie wordt niet toegepast. Een
exemplaar zonder omtrek weigert vóór de taak.

| Test | Rood | Groen |
|---|---|---|
| `document/tests/g9_formula_functions.rs` (7) | `cargo test -p burnburn-document --locked --test g9_formula_functions` -> 6 FAILED (zonder stub: de API bestond al; `Syntax` i.p.v. `Ok`); de komma/puntkomma-test kwam na een eerste fout (`3.14159, 2` werd als decimale komma gelezen) | 7 ok |
| `presentation/tests/g8_parts.rs::g9_a_function_formula...` | geen rode run (de functie zit in de document-laag; deze test bewaakt de UI-weg en de weigeringen) | ok |
| `document/tests/g9_part_fitted.rs` (3) | stub (`Fitted` geeft het pad ongewijzigd): 3 FAILED | 3 ok |
| `io/tests/g9_part_fitted_roundtrip.rs` (1) | geen rode run (na de DTO geschreven; zonder DTO zou de vorm stil verdwijnen) | ok |
| `presentation/tests/g8_parts.rs::g9_a_fixed_shape_takes_parameters...`, `..._refuses_a_formula_that_is_no_formula` | `cargo test -p burnburn-presentation --locked --test g8_parts` -> 2 FAILED | 14 ok. De oude test die zei dat een parameternaam voor een vaste vorm `NotANumber` gaf is bewust gewijzigd (`nergens` i.p.v. `breedte`) |
| `presentation/tests/g9_distribute_nest.rs` (6) | stub (de edit werd geweigerd): 6 FAILED | 6 ok (plaatskeuze, binnen de plaat, tussenruimte, één undo, verouderd antwoord weggelaten, geen taak bij geen ruimte of geen platen) |
| `macos/tests/g9_editor_panels.rs` (2) | stub (variant zonder regel): 2 FAILED | 2 ok |
| `windows/tests/g9_editor_forms.rs` (1) | geen rode run | ok |

## S01 ellips-ellipsraak bij diepe overlap, boog raakt ellips, Bézier verlengen

Ellips-ellipsraak. De solver (`sketch-solver`, vast) convergeert alleen vanuit een toestand dicht bij de raking; twee ellipsen die
elkaars omtrek diep kruisen zijn daar ver vandaan (zelfs 1 mm overlap faalde al met `NotConverged`). `contact_seed.rs`: schuif de
tweede ellips langs de lijn tussen de middelpunten en zoek de afstanden waarop de omtrekken ophouden te kruisen (vlak buiten,
vlak binnen, om de ander heen), dichtstbijzijnde bij de getekende plek eerst. Mislukt de eerste poging (en ook de omgekeerde
asrol van G8), dan start de solve vanuit die plaatsen (startposities van middelpunt en hoofdas van de tweede ellips; de lijnen en
maten blijven wat ze zijn) en wordt de eerste geldige oplossing genomen; daarna nog twee starts van het contactpunt op de
steunpunten van de eerste ellips. Gevolg: de raking die de tweede ellips het minst verplaatst wint. **Beperking:** een onduidelijk
geval (de twee oplossingen even ver) kiest willekeurig tussen die twee; de bewerking wordt in dat geval niet gemeld.
Boog. `Tangent(ellips, boog)` mag (editor-tabel en documentcontrole); de boog telt als zijn hele cirkel (het raakpunt hoeft niet
op de boogspan te liggen); de straal komt uit het begin- en middelpunt zoals ze staan. `Equal(ellips, boog)` blijft geweigerd.
Bézier verlengen (`Path::extend_end_noted`, `ExtendNote`): tot 3 parameterlengtes altijd de eigen kubiek (zoals G7); tot 30 alleen
als de kubiek stabiel is (de snelheid verdwijnt niet en draait nooit meer dan 75 graden van de eindraaklijn); anders recht langs
de eindraaklijn en de reden in de statusregel (`MessageKey::ExtendedStraight`: de kromme draait weg, de grens ligt te ver, of de
kromme ontmoet de grens niet). `extend_end` is hetzelfde zonder de notitie. G7- en G8-tests bleven ongewijzigd groen (hun muren
zijn kort genoeg dat de parabool ze niet raakt).

| Test | Rood | Groen |
|---|---|---|
| `sketch/tests/g9_ellipse_contacts.rs` (8) | `cargo test -p burnburn-sketch --locked --test g9_ellipse_contacts` -> 7 FAILED (boogtests: `WrongEntityKind`; overlaptests: `NotConverged`). Eerste implementatie (steunpunt als start): 4 ok, 3 FAILED (`Conflict` bij een start precies op een as: gekanteld over 0,03 rad; daarna `NotConverged`); de startplaatsen langs de middenlijn losten ze op | 8 ok |
| `editor tool::constraint::an_ellipse_is_an_axis...` | de boog-asserties: 1 FAILED | ok |
| `geometry/tests/g9_extend_far.rs` (7) | stub (`extend_end_noted` zonder gedrag): 6 FAILED, 1 ok (bewaker). Twee orakelfouten van mij: een muur te kort voor de parabool, en een parabool-begin dat bij 6 lengtes al 85 graden gedraaid is | 7 ok |
| `editor tool::extend::tests` (2 nieuwe) | geen rode run | ok |
| `presentation/tests/g9_extend_note.rs` (2) | geen rode run | ok |

## Wijzigingen buiten de lane (alleen additief tenzij vermeld)

- `burnburn-document`: `Dim`-formules met functies (`FormulaError::{UnknownFunction, Arity, Domain}`), `PartShape::Fitted`.
- `burnburn-geometry`: `ExtendNote`, `Path::extend_end_noted`.
- `burnburn-generators`: `guide_tangent_at`.
- `burnburn-io`: `structure_dto::PartShape::Fitted`.
- `burnburn-sketch`: `contact_seed.rs`, arc in de ellipsrakingen.
- `burnburn-editor`: `EditCommit::Extend.note` (veld erbij).
- gedeeld: `StructureEdit::DistributeAndNestOverPlates`, `NewModifierStep::{Linear, Path}`, `TextEdit::{InsertAt, DeleteAt}`,
  `TextField::{Position, Count}`, `TextToolbarViewModel::{contents, samples}`, `MessageKey::{BoundTextFollowsGuide,
  ExtendedStraight}` (en de weigering van een afgeleide tekst zegt nu waarom, in `after_change`), `G5Panel::TextContent`
  (`G5Panel::ALL` is nu 14), `EditorControl`-varianten, teksten in `l10n_g7_editor.rs` en `l10n.rs`.
- Bestaande tests bewust aangepast: `macos/tests/g8_editor_panels.rs` (9 soorten, 14 panelen), `windows/tests/g8_editor_forms.rs`
  (9 soorten; de stapel is een sectie van de inspecteur), `presentation/tests/g8_modifier_add.rs` (een gids in de wereld),
  `g8_parts.rs` (vaste vorm met een woord), `editor` extend-tests (`..` bij `EditCommit::Extend`).

## Handmatig / onbevestigd

- Uiterlijk van de inspecteurkolom met de stapel onderaan (licht en donker, scrollen, het bijschrift van 96 pt in 244 pt), en of de
  automatisch scrollende stapel prettig voelt; de native test bewijst plaats en werking, niet het uiterlijk (stil, transparante vensters).
- Het paneel "Tekst bewerken" opent vanzelf bij elke selectie van teksten (naast de werkbalk): uiterlijk niet bekeken; het kan
  hinderlijk zijn, dan wordt het een menu-item.
- "Verdeel en nest": het uiterlijk van de platen naast elkaar op het canvas (ze zijn geen objecten); een plaat zonder omtrek
  is alleen een rechthoek van de maat. Een project met exemplaren die al op een plaat staan houdt die op hun plek (ze zijn in de
  weg als ze binnen een plaat liggen).
- Windows: alleen type-check, clippy (`x86_64-pc-windows-msvc`) en headless formuliertests; de inspecteursectie en het nieuwe paneel
  zijn nooit in Win32 gedraaid.
- Bézier verlengen: de drempels (3 en 30 parameterlengtes, 75 graden) zijn een keuze, niet gemeten op echte tekeningen.

## Verificatie

Op de commit vóór deze notitie: `cargo fmt --all --check` schoon; `cargo clippy --workspace --all-targets --locked -- -D warnings`
schoon (de eerste run faalde op een bijna-pi in mijn formuletest: aangepast); `cargo clippy -p burnburn-windows --target
x86_64-pc-windows-msvc --all-targets --locked -- -D warnings` schoon; `cargo xtask verify` exit 0 (766 testbinaries/-secties
`ok`, layers: geen schendingen, path-attrs: schoon; de native macOS-tests stil, niet met `--native`).
