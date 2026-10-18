# Technische bronnen voor Rust en native GUI

Geraadpleegd op **17 oktober 2026**. Bronnen onderbouwen de technische mogelijkheden en kandidaatselectie. Het implementatieplan bepaalt de eigen modulegrenzen. Geen van de genoemde dependencies is in deze workspace al geïntegreerd of getest.

## Native platforms en canvas

| Bron | Onderbouwing | Grens |
|---|---|---|
| [objc2 documentatie](https://docs.rs/objc2/latest/objc2/) en [repository](https://github.com/madsmtm/objc2) | Rust-bindings naar Apple-frameworks en een AppKit-voorbeeld | AppKit-lifetimes, hoofdthread en unsafe-invarianten blijven verantwoordelijkheid van de adapter |
| [Microsoft Rust for Windows](https://github.com/microsoft/windows-rs) | Rust-koppeling met Windows-API's | Geen complete BurnBurn-GUI of automatische Mac-port |
| [Microsoft Rust samples](https://github.com/microsoft/windows-rs/tree/master/crates/samples) en [reactor-map](https://github.com/microsoft/windows-rs/tree/master/crates/samples/reactor) | Native WinUI-/reactorvoorbeelden en andere Windows-integraties | De bron waarschuwt dat ontwikkelsamples meestal prerelease-crates gebruiken; een concrete versie moet worden beproefd |
| [Microsoft WinUI 3](https://learn.microsoft.com/en-us/windows/apps/winui/winui3/) | Microsofts native Windows UI-richting | Bewijst niet dat elke benodigde WinUI-functie probleemloos vanuit de gekozen Rust-crates werkt |
| [wgpu](https://docs.rs/wgpu/latest/wgpu/) | Metal- en DX12-backends en expliciete surface/GPU-API's | Native canvasinbedding, compositing en herstel moeten per host worden bewezen |
| [lyon](https://github.com/nical/lyon) | Rust-padtessellatie voor GPU-rendering | Geen productiegeometriekern of widgettoolkit |
| [Vello](https://github.com/linebender/vello) | Rust GPU-gerichte 2D-rendererkandidaat | Alternatief in de renderproef, geen reeds gemaakte selectie |

## Geometrie, solver en tekst

| Bron | Gebruik in het plan |
|---|---|
| [kurbo](https://github.com/linebender/kurbo) | Kandidaat voor curvebewerkingen achter eigen geometrische contracten |
| [iOverlay](https://github.com/iShape-Rust/iOverlay) | Rust-kandidaat voor polygonale booleans/offsetroute; eigen tolerantie- en topologietests blijven vereist |
| [sketch-solver](https://docs.rs/sketch-solver/latest/sketch_solver/) | Jonge Rust-native solverkandidaat met gedocumenteerde lijnen/bogen/cirkels en diagnose; dekking en numeriek gedrag worden getest |
| [slvs](https://docs.rs/slvs/latest/slvs/) | De documentatie noemt expliciet een wrapper om SolveSpace; daarom geen Rust-native algoritmekern voor dit productbesluit |
| [RustyBuzz](https://github.com/harfbuzz/rustybuzz) | Rust-port van tekstshaping als kandidaat voor gedeelde productielettervormen |
| [VTracer](https://github.com/visioncortex/vtracer) | Rust-tracekandidaat; versie- en corpusgebonden selectie uit het bestaande traceplan |

Een afhankelijkheid wordt pas gekozen na tests op de eigen corpusgevallen, vastgelegde versie/commit, licentie-inventaris en controle van transitieve native builds. De aanwezigheid van een Rust-wrapper is geen bewijs dat de onderliggende algoritmen Rust zijn.

## Tests en referentiegegevens

| Bron | Gebruik |
|---|---|
| [Proptest](https://proptest-rs.github.io/proptest/intro.html) | Propertytests en shrinking als aanvulling op concrete voorbeeldtests |
| [Rust Fuzz Book](https://rust-fuzz.github.io/book/cargo-fuzz.html) | cargo-fuzz/libFuzzer-route voor bytes en parsergrenzen |
| [Insta](https://docs.rs/insta/latest/insta/) | Beoordeelbare snapshots van semantische resultaten |
| [Bestaande RD-fixtures](../lightburn/evidence/fixtures-manifest.json) | Herkomst en hashes voor oorspronkelijke LightBurn-exports |
| [Oorspronkelijke exportanalyse](../lightburn/ruida/04-exportanalyse.md) | Condities, onderzochte bytes en nog open grayscale-/firmwaresemantiek |

De test-first-werkwijze is een expliciete gebruikerskeuze. De genoemde tools ondersteunen die werkwijze; zij bewijzen niet automatisch dat tests vóór de implementatie zijn geschreven of dat een machinefeature fysiek werkt.

## Uitkomst van de technische keuzes

**Vast:** Rust, AppKit voor Mac, gedeelde kern/presentatie/editor, gedeeld canvascontract en TDD. **Te beproeven:** precieze libraryversies, Rust-solverdekking, renderer/tessellatie, canvasinbedding op Windows en houdbare WinUI/Rust-packaging. Die proeven mogen de gebruikerskeuze voor Rust en native GUI niet stilzwijgend vervangen.
