# ADR 0007 — Algoritme-dependencies (voorlopige keuze)

- Status: **Voorlopig aanvaard (Design)**, 17 oktober 2026. Elke keuze wordt definitief pas na de genoemde
  contracttests in het werkpakket; tot die tijd is het een *kandidaat met voorkeur*.
- Controle: `cargo search`/`cargo info` (versie, licentie, MSRV) en een proefresolutie met `cargo tree`/
  `cargo metadata` voor `aarch64-apple-darwin` en `x86_64-pc-windows-msvc`: transitieve afhankelijkheden, `-sys`-
  crates, buildscripts. Toolchain 1.99.1 ≥ alle MSRV's hieronder.

## Regels (uit AGENTS.md en het plan)

1. *Rust-native* betekent: het algoritme zelf is Rust, ook transitief. Een Rust-wrapper om C/C++ telt niet.
2. Geen extern modeltype in een publiek contract: elke dependency zit achter een eigen adaptermodule
   (`geometry::curve_kurbo`, `geometry::boolean`, `sketch::solver`, …) met eigen typen in de API.
3. Kandidaten krijgen contracttests op onze fixtures (degeneraties, toleranties, determinisme), niet een hertest van
   de hele library.
4. Referentie-/onderzoekstools (SolveSpace, Potrace, AutoTrace, Python-RD-decoder) blijven buiten de runtimegraaf;
   `layers.toml` verbiedt bekende wrappers (`slvs`, `potrace-sys`, `harfbuzz-sys`, …) transitief.

## Keuzes

| Domein | Keuze | Versie | Licentie | Rust-native? (gecontroleerd) | Crate | Beslisproef |
|---|---|---|---|---|---|---|
| Curves (Bézier, bogen, flattening, nearest, bounds) | **kurbo** achter adapter | 0.13.1 | Apache-2.0 OR MIT | Ja — deps `arrayvec`, `smallvec`, `polycool`; geen buildscript | geometry | Maximale afstandsfout flattening, boog→cubic, degeneraties (IM02/IM09) |
| Booleans / offsets | **i_overlay** | 9.1.0 | MIT OR Apache-2.0 | Ja — deps `i_float`, `i_shape`, `i_tree`, `i_key_sort`, `libm` (zelfde auteur) | geometry | Gaten, raakpunten, korte segmenten, integer-schaalbereik (IM09) |
| Sketchsolver | **sketch-solver** als eerste kandidaat; eigen residukern als fallback | 0.1.0 | MIT OR Apache-2.0 | Ja — enige dep `libm` | sketch | Montageplaat-constraints, onder-/overbepaald, conflict vs. niet-convergent, branch-flip, determinisme op Mac én Windows (IM11) |
| Tekstshaping | **harfrust** | 0.14.0 | MIT | Ja — `read-fonts` 0.45.1, `font-types`; geen `-sys` | (nieuw) tekstmodule in geometry of eigen crate bij IM18 | Glyphposities t.o.v. referentie-shaping-fixtures, ontbrekende glyphs, heropenen |
| Fontparsing / glyphcontouren | **skrifa** | 0.48.0 | MIT OR Apache-2.0 | Ja — deelt `read-fonts` 0.45.1 met harfrust | idem | Contouren van vaste fonts tegen vooraf gecontroleerde punten, variabele fonts |
| Contourtracing | **eigen Rust-pipeline** in `burnburn-trace`; `vtracer` alleen als benchmark/vergelijkingsreferentie (dev-dependency of aparte bench) | vtracer 1.0.0-alpha.4 | MIT OR Apache-2.0 (transitief ook Apache-2.0-only `flo_curves`, BSD-2 `roots`) | Ja, maar *alpha*, trekt `visioncortex` 0.9.3, twee `flo_curves`-versies en `ouroboros` (self-referential, unsafe) | trace | Ring met gat op fysieke schaal, contourboom, lokale correctie (IM12) |
| Beelddecoders | **image** met `default-features = false, features = ["png", "jpeg"]` | 0.25.10 | MIT OR Apache-2.0 | Ja — `png`, `zune-jpeg`, `moxcms` (ICC), geen `rayon` (determinisme) | image | Fuzz + begrensde afmetingen; EXIF-oriëntatie en DPI-metadata expliciet |

### Toelichting op afwijkingen van het plan

- **harfrust + skrifa i.p.v. RustyBuzz + ttf-parser.** Beide paren zijn Rust-native en permissief
  (rustybuzz 0.20.1 MIT, ttf-parser 0.25.1 MIT OR Apache-2.0). harfrust is de HarfBuzz-port uit hetzelfde
  `harfbuzz`-project en deelt met skrifa één font-leeslaag (`read-fonts`), wat één parser voor shaping én contouren
  geeft. Vello gebruikt skrifa 0.44; als Vello later komt (ADR 0003) ontstaat een tweede versie — acceptabel voor een
  benchmark. **Terugval:** rustybuzz 0.20.1 + ttf-parser 0.25.1, beslissing in IM18 op dezelfde fixtures.
- **Tracer: geen vtracer in de runtime (voorlopig).** De architectuur vraagt contourboom/centerlinegraaf, fitmetingen
  en lokale correcties; vtracer levert paden zonder die structuur en is alpha. We bouwen de pipeline zelf
  (threshold → componenten met gatenboom → contour → fitting) en vergelijken op het corpus met vtracer.
  Herzien als de eigen pipeline op het IM12-corpus slechter scoort.
- **sketch-solver** is 0.1.0 en jong. De claim "bit-identical on every platform" wordt onafhankelijk getest op
  macOS (aarch64) en Windows (x86_64). Faalt de dekking voor de montageplaat, dan start het afgebakende
  Rust-solverwerkpakket (plan), nooit een `slvs`-wrapper.

## Gevolgen

- Versies en "wie mag het gebruiken" staan in het [dependencyregister](../design/dependency-register.md).
- Kern-implementatie voegt een dependency pas toe in het werkpakket waarin de eerste test hem nodig heeft.

## Amendement fase B (18 oktober 2026)

Orchestratorbesluiten verwerkt (voorlopig; eigenaar kan herzien):

- **Tekst: harfrust 0.14.0 + skrifa 0.48.0** is de keuze (niet langer "voorkeur"); crate `burnburn-text` (laag 3,
  IM18). Terugval rustybuzz + ttf-parser blijft beschreven maar vergt een amendement.
- **Tracer: eigen Rust-pipeline** in `burnburn-trace`; `vtracer` uitsluitend als dev-dependency/bench-referentie
  (`traces/bench` of `tests/`), nooit in `[dependencies]`. `layers.toml` vangt dat niet automatisch (vtracer is Rust);
  review van het register wel.
- **i_overlay 9.1.0** wordt toegevoegd door Kern zodra de eerste booleantest (IM09) hem nodig heeft, achter
  `geometry::boolean` (eigen `BooleanOp`, `FillRule`, integer-schaal expliciet en getest op het bereik
  ±10 m bij 1 µm). Booleans werken op geflattende paden met een eigen `BooleanTolerance`; bogen worden ná de operatie
  niet teruggefit tenzij een test dat vraagt.
- **image 0.25.10** (`default-features = false, ["png", "jpeg"]`) in `burnburn-image` alleen achter `image::decode`
  met `DecodeLimits` (`image::Limits` gezet vóór decoderen). Geen `rayon`-feature (determinisme).
- **SVG-import: usvg 0.48.1, `default-features = false`** (geen `text`/fontdb, geen systeemfonts). Proefresolutie
  18-10 voor beide triples: pure Rust, geen buildscript met C; deps `roxmltree`, `svgtypes`, `kurbo` 0.13.1 (dezelfde
  als geometry), `tiny-skia-path` (BSD-3-Clause), `simplecss`, `imagesize`, `data-url`. Alleen in `io::import::svg`.
- **DXF: eigen begrensde lezer** in `io::import::dxf` (contracts §14). `dxf` 0.6.2 (MIT, Rust) is geen
  runtime-dependency: geen invoerlimieten en een groot eigen model; mag als test-referentie (dev) na registerregel.
- **lyon_tessellation** blijft de render-tessellator (ADR 0003); toevoegen door GUI bij IM07-integratie.

Nieuwe kandidaten fase C/D (alleen geregistreerd, niet toevoegen zonder werkpakkettest):
`roxmltree` 0.21.1 (LBRN2-XML, IM28), `lopdf` 0.45.0 (PDF, IM28; trekt `aes`/`cbc`/`rand` voor versleutelde PDF —
zwaar; alternatief eigen content-stream-lezer bovenop een kleine objectparser, beslissen in IM28), `nalgebra` 0.35.0
(Apache-2.0; camera-calibratie/homografie IM27 — of eigen 3×3/8×8-oplosser als dat volstaat), `crc32fast` 1.5.2
(journalrecords, ADR 0008), `fluent-bundle` 0.16.0 (NL/EN, IM20). Alle vijf: pure Rust, geen `cc` in de
proefresolutie van 18-10.

## Open (eigenaar)

- Geen nieuwe. (Eigen tracer en harfrust/skrifa zijn door de orchestrator voorlopig besloten; herzien kan op basis van
  de IM12-corpusscore resp. de IM18-shapingfixtures.)
