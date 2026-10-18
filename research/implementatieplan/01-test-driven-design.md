# Test-driven design voor alle Rust-code

**Iedere gedragswijziging begint met een falende test.** Dit is de ontwikkelwerkwijze voor de volledige Rust-applicatie, inclusief native adapters. Tests bepalen de contracten en modulegrenzen; zij worden niet achteraf als afvinkstap toegevoegd.

## De ontwikkelcyclus

1. Kies één gebruikersscenario of foutgeval uit een werkpakket en schrijf de verwachte uitkomst op.
2. Schrijf de kleinste test tegen het publieke gedragscontract, met een concrete fixture of een expliciete invariant.
3. Maak de minimale API/stub wanneer nodig om de test te compileren. Voer de test uit en controleer dat hij faalt op het nog ontbrekende gedrag. Een typefout, ontbrekende dependency of onbereikbare machine telt niet als functionele redfase.
4. Implementeer alleen het gedrag dat nodig is om de test groen te krijgen.
5. Refactor productiecode en tests met dezelfde observabele uitkomst. Voer de relevante suite opnieuw uit.
6. Voeg de volgende randvoorwaarde toe en herhaal. Voor een bug begint dit met een reproducerende regressietest.

Per werkpakket noteert een korte ontwikkelnotitie testnaam, verwachte fout, werkelijk uitgevoerde red-/green-commando's en resultaat. Het is niet nodig een falende commit te publiceren. Een CI-run achteraf kan de test-first-volgorde niet bewijzen; de notitie maakt de lokale cyclus navolgbaar.

Een test bevat geen gekopieerde productieformule als enige oracle. Testuitkomsten komen uit analytisch eenvoudige gevallen, onafhankelijke referenties of vooraf beoordeelde fixtures. Standaardcontracten testen we aan onze adaptergrens; we testen niet alle internals van een externe library opnieuw.

## Ontwerp door tests

| Testprobleem | Ontwerpconsequentie |
|---|---|
| Een geselecteerd object wijzigen vereist AppKit | Verplaats selectie-/commandgedrag naar `burnburn-editor`/`presentation` |
| Een timeouttest moet echt wachten | Injecteer een clock/timerpoort |
| Een tracecorrectie vereist een zichtbare dialoog | Maak masker en regionpatch een puur domeincontract |
| Een RD-test opent een UDP-socket | Scheid codec van transport |
| Een unit test heeft toegang tot de echte machine | Gebruik fake transport; hardwaretests zijn apart |
| Een save-test moet gebruikersbestanden aanpassen | Gebruik een tijdelijke workspace en filesystemadapter |
| Een test kan alleen een hele appmock aansturen | Verklein het contract tot intentie, invoer en uitkomst |

Mock alleen de externe grens wanneer dat iets bewijst. Gebruik echte geometrie en echte documentcommands in applicatiescenario's. Render- en GUI-tests vullen domeintests aan; ze vervangen ze niet.

## Testlagen

### Getypeerde data en numeriek gedrag

Begin met eenheden, IDs, transforms en expliciete toleranties. Asserties gebruiken exacte waarden voor integerprotocolvelden, IDs en structuureigenschappen. Geometrische floats gebruiken een onderbouwde absolute/relatieve foutgrens. Een globale epsilon mag verschillende toleranties niet verbergen.

Voorbeelden: mm↔inch bewaart fysieke maat; affine inverse herstelt punten binnen het vastgelegde budget; een spiegeling wijzigt winding voorspelbaar; een singuliere matrix en niet-eindige input geven een typed error. Identiteits- en revisietests gebruiken geen echte klok of willekeurige UUID wanneer dat niet nodig is.

### Document, undo en opslag

Test commands als gedrag: een object toevoegen, wijzigen, verwijderen, undo en redo. Controleer ook bronreferenties en proceskoppelingen. Een mislukte transactie verandert niets. Een sleepactie levert één historie-entry; Cancel herstelt het begintoestandbeeld.

Opslagtests gebruiken echte kleine containers in tijdelijke mappen. Injecteer fouten op write, flush en replace. Controleer de werkelijk herstelbare vorige of nieuwe versie; noem een vervangen bestand niet zonder meer duurzaam bij stroomverlies. OS-specifieke replace-/flush-semantiek krijgt eigen integratietests. Recoveryrecords worden gelezen alsof een nieuw proces start.

### Geometrie en solver

Analytische fixtures komen eerst: rechthoek, cirkel, twee overlappende rechthoeken en een plaat met gaten. Daarna bijna samenvallende punten, kleine gaten, raakpunten en grote coördinaten. Propertytests variëren transform, schaal en inputvolgorde waar de semantiek dat toestaat.

De solvercontracttests bevatten onderbepaald, volledig bepaald, redundant, conflict, niet-convergent en branch-flip. Een test beoordeelt residu, relevante vrijheidsgraden en behoud van de bedoelde oplossing. Een solverstatus “conflict” wordt niet geëist waar alleen niet-convergentie bewezen kan worden. Bij falen blijft de laatste geldige geometrie behouden en wordt nieuwe productie geblokkeerd.

### Editorcontroller en viewmodels

Een `HeadlessEditorHarness` stuurt dezelfde intents als de native hosts. Test selectie, snapkandidaten, gereedschaptransities, drag/cancel, focuscommando's, numerieke invoer en command-enablement. Een headless harness bevat geen verborgen alternatieve implementatie van de editor.

Controleer dat een oude workeruitkomst na een nieuwere wijziging wordt verworpen, een gesloten documentsessie geen callback meer ontvangt en twee vensters geen selectie delen. Apparente widgetveranderingen worden getoetst via het resulterende viewmodel en een beperkte native integratietest.

### Tracer en beeldketen

De eerste redtests zijn een ring met gat, losse componenten, een scherpe hoek en een expliciete schaal. Voor centerline volgen T/X/Y en twee bijna rakende streken. Verwachte topologie staat vóór implementatie in het corpusmanifest.

Synthetische beelden worden uit onafhankelijke vectorreferenties gemaakt met vastgelegde rasterisatie. Werkelijke scans hebben beoordeelde ankers en labels. Traces worden beoordeeld op maximale/bidirectionele afstand, gaten en graafstructuur; alleen pixeloverlap of een snapshot is onvoldoende. Lokale correctie moet niet-betrokken segmenten behouden.

### CAM en Ruida

Test eerst de semantische JobPlan: units, oorsprong, volgorde, moves, scans en vermogensbeleid. Test daarna afzonderlijk de encoder: known vectors, veldgrenzen, signedwaarden, scrambling, sommen en eindrecords. Een eigen encode→decode-roundtrip is aanvullend bewijs.

Gebruik de bestaande drie RD-bestanden, hun hashes en oorspronkelijke condities als externe fixtures. De Python-onderzoeksdecoder blijft een onafhankelijke ontwikkeloracle. Geen runtimeafhankelijkheid van Python. Nieuwe golden-output krijgt een inhoudelijke review voordat de verwachte bytes worden gewijzigd.

### Machine actor en foutpaden

Een gescripte fake controller laat pakketten vallen, antwoorden dupliceren, opslag weigeren, een startantwoord verliezen of een disconnect simuleren. De fake clock maakt de test direct en reproduceerbaar. Asserties gaan over uitgestuurde intenties en bewezen toestand.

Verplichte tests zijn: ACK ≠ Stored; Stored ≠ Completed; verloren startantwoord geeft Unknown; reconnect stuurt geen tweede start; Cancel verwijdert geen impliciet controllerbestand; een antwoord voor een andere machine of oude execution wordt genegeerd.

### Renderer en native GUI

De scenecompiler heeft zuivere tests voor geometrie, overlays, volgorde, clip en viewport. Offscreen GPU-tests controleren beperkte beelden met verklaarde pixeltolerantie. Pixelverschillen tussen Metal en DX12 kunnen voorkomen zonder geometriefout; gedeelde semantische resultaten moeten wel gelijk blijven.

Voor native gedrag schrijven we eerst een kleine adapter-/GUI-test: venster openen, een maatveld committen, native menu-undo, file-dialog annuleren, focus en sluiten tijdens een worker. Mac-tests draaien op de hoofdthread met de echte eventloop. Windows-tests draaien op Windows; cross-compilatie alleen telt niet als geslaagde UI-proef.

Rust kan via platformbindings testinterfaces en accessibility aansturen. Als een OS-gedrag geen betrouwbare automatische oracle heeft, leggen we vóór implementatie een concrete handmatige acceptatieproef vast en automatiseren de vertaalbare adapterlogica. Zo'n proef wordt als handmatig gerapporteerd, nooit als geslaagde geautomatiseerde test.

## Propertytests, fuzzing en snapshots

Gebruik [Proptest](https://proptest-rs.github.io/proptest/intro.html) voor aanvullende invarianten, shrinking en vastgelegde regressiegevallen. Doelgevallen zijn geometrische transformaties, undo-/redo-sequenties, willekeurige geldige documentscenario's en protocolveldgrenzen.

Gebruik [cargo-fuzz](https://rust-fuzz.github.io/book/cargo-fuzz.html) voor onbetrouwbare bytes: projectcontainer, SVG/DXF-parsergrenzen, beelddecoders, RD-decoder en transportframes. Bounded input en uitvoerlimieten horen bij het contract. Een gevonden crash krijgt een minimale fixture en een gewone regressietest. Instrumented fuzzing draait apart van de stabiele productietoolchain.

[Insta](https://docs.rs/insta/latest/insta/) is een kandidaat voor compacte, leesbare snapshots van JobPlan, viewmodels en importsamenvattingen. Snapshotupdates worden inhoudelijk beoordeeld; automatisch accepteren in CI is verboden. Een snapshot van “wat de huidige code toevallig doet” is geen onafhankelijke productspecificatie.

## Corpus en testindeling

```text
crates/<crate>/src/              lokale gedragstests naast de code
crates/<crate>/tests/            publieke contracttests
tests/scenarios/                headless gebruikersscenario's
tests/native/macos/             AppKit integratie en accessibility
tests/native/windows/           Windows integratie en accessibility
test-data/geometry/              analytische en degeneratiegevallen
test-data/projects/              schema/migratie/recoveryprojecten
test-data/trace/                 manifest, bronbeelden, referenties, holdout
test-data/ruida/                 hashes en herleidbare RD-referenties
test-data/transport/             scripts en captures zonder onnodige privédata
fuzz/                           parser- en protocoltargets
```

Dit zijn geplande paden, nog geen aanwezige bestanden. Bestaand onderzoek blijft ongewijzigde herkomst. Productfixtures worden expliciet geselecteerd en van manifest, licentie/herkomst en hashes voorzien.

## CI en lokale verificatie

Het eerste werkpakket maakt een `cargo xtask verify`-route die de afgesproken checks uitvoert. De onderstaande commando's zijn de beoogde controlecontracten, nog geen claim dat deze workspace al bestaat.

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --doc --workspace --locked
```

De workspace gebruikt target-cfg op platformbinaries zodat de suite op een ander OS geen foreign frameworks probeert te linken. Kern- en hostjobs worden in CI expliciet geselecteerd. Geen ongerichte `--all-features` die onverenigbare backend-/platformfeatures combineert. Hostspecifieke GPU-/GUI-tests worden op passende runners geselecteerd.

| Job | Wanneer | Doorslaggevend bewijs |
|---|---|---|
| Snelle kernsuite | Iedere wijziging | Commands, geometry, document, solver, tracer, CAM en codec |
| macOS build/native | Iedere relevante wijziging | AppKit lifecycle, input, bundle-smoke en Metal |
| Windows kern/hostproef | Vanaf eerste editorfase | Geen Apple-dependencies in kern; gedeelde scenario-uitkomsten en native smoke |
| Fault/recovery | Wijzigingen aan runtime, opslag of machine | Fake-time scenario's en herstelgedrag |
| Fuzz en zwaardere properties | Periodiek of bij parserwijziging | Nieuwe crashgevallen blokkeren betrokken vrijgave |
| Benchmarks | Afhankelijk van gewijzigde module | Vaste machine/fixture, trend en p95; geen harde timinggrens op willekeurige CI-runner |
| Hardwarebench | Voor supportclaim of driverrelease | Model/firmware/interface, meetresultaat en operatorbeoordeling |

Een fysiek hardwaretestscript staat standaard uit in gewone CI. Het heeft een expliciete machineconfiguratie en testmodus. Tijdkritische of mechanische juistheid wordt niet door een fake controller gecertificeerd.

## Gereedcriteria

Een werkpakket is gereed wanneer de relevante falende tests eerst zijn waargenomen, de implementatie groen is, de refactor is uitgevoerd, fixtures herleidbaar zijn en de betrokken suites slagen. Platforms en hardware die niet getest zijn worden als onbevestigd gerapporteerd. Coverage wordt gebruikt om hiaten te zoeken; een arbitrair percentage vervangt geen goede asserties op kritieke uitkomsten.

Dependencies veranderen via hun eigen contracttests. Eigen Rust bevat in de zuivere kern `forbid(unsafe_code)` waar haalbaar; expliciete native-/GPU-grenzen krijgen kleine reviewed unsafe-blokken met invarianten, lifecycle-tests en passende tooling. Het bestaan van Rust-bindings maakt een verkeerd OS-lifetimecontract niet automatisch veilig.
