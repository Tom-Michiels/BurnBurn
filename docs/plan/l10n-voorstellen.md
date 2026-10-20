# L10n-voorstellen (W2-L7)

Opgesteld 20-10-2026 op branch `claude/w2-l7-docs-8620aa` tegen main `8d0f019` (golf 1 + SolidCraft-golf). Hoort bij [lightburn-matrix.md](lightburn-matrix.md) en [lightburn-pariteit.md](lightburn-pariteit.md).

Dit document zijn **voorstellen**, nog niet toegepast. Ze zijn bedoeld voor de UI-lane (W2-L1-*) die ze in `crates/burnburn-macos/src/l10n.rs` en `assets/l10n/tools.toml` overneemt.

## Samenvatting

| Groep | Aantal | Waar het gebeurt |
|---|---:|---|
| A. Undo-labels zonder tekst (top-level en sub-labels) | 39 | `undo_pair` in `l10n.rs` heeft geen arm voor deze top-level labels; `_ => return None` (regel 592) |
| B. Overige sleutels zonder arm (EditLabel, ProcessLabel, SketchLabel, ToolRejection) | 11 | `_ => return None` / `_ => (“Nog niet beschikbaar.”)` in `l10n.rs` |
| C. Tekst die verkeerd of zwak is | 2 | `SketchLabel::Batch` (l10n.rs:589), `Feature::RampLength` (l10n.rs:888) |
| D. Tools zonder regel in `tools.toml` | 3 | `sketch.pen`, `sketch.polyline`, `sketch.ellipse.box` |
| E. Uitgeschakelde tools zonder eigen reden | 9 | `DisabledReason` generiek in `toolbar.rs:83` |
| **Voorstellen (NL + EN): A + B + C + D + E** | **64** | |
| F. Opruimen (geen nieuwe tekst) | 12 | 11 verweesde `tools.toml`-regels + stale `disabled_reason` bij `node` |

Echte gaten zitten in de `_`-takken van `undo_pair` (regel 592 voor de top-level labels, 563/576/590 voor de sub-enums) en in de `ToolRejection`-tak (regel 766). De overige `_`-takken dekken varianten die wel een arm hebben of zijn gewone fouttakken.

## Hoe gevonden

- `l10n.rs`: alle `_ =>`-takken en de enum-varianten van de bijbehorende types; per variant gecontroleerd of er een arm met `=>` op dezelfde regel staat.
- `tools.toml`: alle `[tools."…"]`-ids vergeleken met de 63 specs in `crates/burnburn-presentation/src/catalog.rs`; velden `panel`, `nl`, `en`, `tip_nl`, `tip_en` gecontroleerd (alle 71 regels compleet).
- `toolbar.rs`: `tool_label`, `tool_tip`, `reason_text`, `panel_title` op fallbacks gelezen.
- Gebruik gecontroleerd met `grep` op de producenten (bijv. `UndoLabel::Structure(s.label())` in `crates/burnburn-document/src/command.rs:84`).

**Waarom de bestaande test dit niet vangt:** `all_message_keys_have_nl_and_en` (`crates/burnburn-macos/tests/l10n.rs`) loopt over `known_message_keys()`. Die lijst bevat de undo-labels van `Edit`, `Process`, `Sketch` en de eerste vijf top-level labels, maar niet `Structure`, `Text`, `ApplyPatch`, `Trace`, `Generator` en `Shape`. Daarom faalt de test niet. Zie “Vervolg”.

**Niet gedaan:** de build en de tests zijn in deze sessie niet gedraaid. Alle bevindingen zijn statisch.

## Terminologie

- **EN** volgt LightBurn-terminologie waar het dossier (`research/lightburn/`) of de matrix die gebruikt: Offset, Trim, Break, Join, Round Corners, Optimize Shapes, Cut Shapes, Ramp Length, Scan Angle, Cross-Hatch, Flood Fill, Kerf, Overcut, Perforation, Dot, Tabs, Leads.
- **NL** volgt de bestaande NL-catalogus in `l10n.rs` (bijv. “Verplaatsen”, “Verwijderen”, “Maat wijzigen”, “Schets maken”). Het dossier bevat **geen** Nederlandse LightBurn-UI-tekst (M235 noemt alleen de taal). De NL-keuzes zijn dus afgeleid van BurnBurn zelf en moeten tegen de Nederlandse LightBurn-UI worden gecontroleerd als die beschikbaar is.
- Geen sneltoetsen in tekst (zoals `tools.toml` vermeldt).

## A. Undo-labels zonder tekst

`undo_pair` (`crates/burnburn-macos/src/l10n.rs:542`) kent de top-level labels `CreateRectangle`, `SetDimension`, `Transform`, `Delete`, `Rename`, `Edit`, `Process`, `Sketch`. De labels hieronder hebben geen arm. De Undo-tekst wordt dan `[UndoAction(…)]` in het menu.

### A1. Top-level (6)

| Sleutel | Geproduceerd in | NL | EN |
|---|---|---|---|
| `UndoLabel::Structure(StructureLabel)` | `burnburn-document/src/command.rs:84` | Structuur wijzigen | Change Structure |
| `UndoLabel::Text(TextLabel)` | `command.rs:85` | Tekst wijzigen | Change Text |
| `UndoLabel::ApplyPatch` | `command.rs:86` | Patch toepassen | Apply Patch |
| `UndoLabel::Trace(TraceLabel)` | `command.rs:87` | Trace wijzigen | Change Trace |
| `UndoLabel::Generator(GeneratorLabel)` | `command.rs:88` | Generator wijzigen | Change Generator |
| `UndoLabel::Shape(ShapeLabel)` | `command.rs:89` | Vorm wijzigen | Change Shape |

### A2. StructureLabel (20, `burnburn-document/src/structure/command.rs:103`)

| Variant | NL | EN |
|---|---|---|
| CreateModifier | Modifier maken | Create Modifier |
| EditModifier | Modifier wijzigen | Edit Modifier |
| ToggleModifierStep | Modifierstap aan/uit | Toggle Modifier Step |
| RemoveModifier | Modifier verwijderen | Remove Modifier |
| AddReference | Verwijzing toevoegen | Add Reference |
| RebindReference | Verwijzing opnieuw koppelen | Rebind Reference |
| RemoveReference | Verwijzing verwijderen | Remove Reference |
| AddRelation | Relatie toevoegen | Add Relation |
| RemoveRelation | Relatie verwijderen | Remove Relation |
| DefinePart | Onderdeel definiëren | Define Part |
| EditPart | Onderdeel wijzigen | Edit Part |
| RemovePart | Onderdeel verwijderen | Remove Part |
| PlaceInstance | Exemplaar plaatsen | Place Instance |
| EditInstance | Exemplaar wijzigen | Edit Instance |
| RemoveInstance | Exemplaar verwijderen | Remove Instance |
| CreatePlate | Plaat maken | Create Plate |
| EditPlate | Plaat wijzigen | Edit Plate |
| RemovePlate | Plaat verwijderen | Remove Plate |
| AssignToPlate | Toewijzen aan plaat | Assign to Plate |
| ApplyNest | Nesting toepassen | Apply Nest |

### A3. TraceLabel (4, `burnburn-document/src/trace.rs:101`), ShapeLabel (3, `shape_edit.rs:27`), GeneratorLabel (3, `generator.rs:88`), TextLabel (3, `text/command.rs:27`)

| Enum.variant | NL | EN |
|---|---|---|
| TraceLabel.Apply | Trace toepassen | Apply Trace |
| TraceLabel.Retrace | Opnieuw traceren | Retrace |
| TraceLabel.Move | Trace verplaatsen | Move Trace |
| TraceLabel.Delete | Trace verwijderen | Delete Trace |
| ShapeLabel.Create | Vorm maken | Create Shape |
| ShapeLabel.SetParams | Vormparameters wijzigen | Change Shape Parameters |
| ShapeLabel.ConvertToPath | Omzetten naar pad | Convert to Path |
| GeneratorLabel.Create | Object genereren | Generate Object |
| GeneratorLabel.Edit | Generator wijzigen | Edit Generator |
| GeneratorLabel.Delete | Generator verwijderen | Delete Generator |
| TextLabel.Create | Tekst maken | Create Text |
| TextLabel.Edit | Tekst wijzigen | Edit Text |
| TextLabel.Delete | Tekst verwijderen | Delete Text |

## B. Overige sleutels zonder arm

| Sleutel | Waar | Huidige uitkomst | NL | EN |
|---|---|---|---|---|
| `EditLabel::CutCorner` | `burnburn-document/src/edit.rs:139` (producent `editor/src/tool/fillet.rs:65`) | geen arm → `[UndoAction(…)]` | Hoek afsnijden | Cut Corner |
| `EditLabel::Break` | `edit.rs` (producent `GeometryCommand::BreakAt`, `edit.rs:177`) | geen arm | Pad breken | Break Path |
| `EditLabel::RoundCorners` | `edit.rs` | geen arm | Hoeken afronden | Round Corners |
| `EditLabel::OptimizeShapes` | `edit.rs`, `edit/ops.rs:32` | geen arm | Vormen optimaliseren | Optimize Shapes |
| `EditLabel::CutShapes` | `edit.rs`, `edit/ops.rs:21` | geen arm | Vormen snijden | Cut Shapes |
| `EditLabel::JoinShapes` | `edit.rs`, `edit/ops.rs:24` | geen arm | Vormen verbinden | Join Shapes |
| `EditLabel::Direction` | `edit.rs` | geen arm | Richting wijzigen | Change Direction |
| `ProcessLabel::SetAdvanced` | `burnburn-document/src/process/command.rs:73` | geen arm | Geavanceerde instellingen wijzigen | Change Advanced Settings |
| `SketchLabel::SetConstruction` | `burnburn-document/src/sketch/command.rs:96` | geen arm | Hulplijn wijzigen | Change Construction |
| `SketchLabel::AddExternal` | `sketch/command.rs:96` | geen arm | Externe geometrie toevoegen | Add External Geometry |
| `ToolRejection::Unavailable` | `burnburn-presentation/src/view_model.rs:509` (2 producenten) | valt op generiek “Nog niet beschikbaar.” | Nog niet beschikbaar: dit gereedschap heeft nog geen functie. | Not available yet: this tool has no function yet. |

## C. Tekst die verbeterd moet worden

| Sleutel | Huidig | Voorstel NL | Voorstel EN | Reden |
|---|---|---|---|---|
| `SketchLabel::Batch` (l10n.rs:589) | “Schets bewerken” / “Edit Sketch” | Schetswijzigingen | Sketch Changes | Dezelfde tekst als `CommandKey::SketchEdit`; de undo-stap is een batch, geen bewerking. |
| `Feature::RampLength` (l10n.rs:888) | “vermogensverloop (ramp)” / “power ramp” | rampelengte | ramp length | LightBurn en de matrix noemen het “Ramp Length”; NL “vermogensverloop” is een andere betekenis. |

## D. Gereedschappen zonder regel in `tools.toml`

De tools hebben nu een label via een fallback op het commando (`toolbar.rs`, `tool_label`), maar geen tip. Ze staan in de catalogus als `Tool(…)` en zijn dus actief.

| id | panel | nl | en | tip_nl | tip_en |
|---|---|---|---|---|---|
| `sketch.pen` | create | Pen | Pen | Pen: klik of sleep de punten van het pad. Esc annuleert zonder object. | Pen: click or drag the path points. Esc cancels without an object. |
| `sketch.polyline` | create | Polylijn | Polyline | Polylijn: klik de hoekpunten achter elkaar. Esc annuleert zonder object. | Polyline: click the corner points one after another. Esc cancels without an object. |
| `sketch.ellipse.box` | create | Ellips in vak | Ellipse in box | Ellips in vak: sleep een vak; de ellips past precies in het vak. Esc annuleert. | Ellipse in box: drag a box; the ellipse fits the box exactly. Esc cancels. |

## E. Uitgeschakelde gereedschappen zonder eigen reden

`reason_text` (`toolbar.rs:83`) geeft voor alle uitgeschakelde gereedschappen dezelfde tekst. De 9 hieronder staan in de catalogus als `Unavailable` of `Deferred` en hebben geen `disabled_reason_*` in `tools.toml`. Voor `Deferred` geldt de catalogustekst “resultaat kan nog niet atomair worden vastgelegd (batchcommando volgt)”.

| id | Catalogus | disabled_reason_nl | disabled_reason_en |
|---|---|---|---|
| `sketch.centerline` | Unavailable | Nog niet beschikbaar: hulplijnen vragen een bouwgeometrie-vlag in het document. | Not available yet: centerlines need a construction flag in the document. |
| `sketch.point` | Unavailable | Nog niet beschikbaar: punten vragen een puntobject in het document. | Not available yet: points need a point object in the document. |
| `sketch.insert_svg` | Unavailable | Gebruik Bestand ▸ Importeer voor SVG. | Use File ▸ Import for SVG. |
| `sketch.insert_dxf` | Unavailable | Gebruik Bestand ▸ Importeer voor DXF. | Use File ▸ Import for DXF. |
| `sketch.mirror` | Deferred | Volgt: spiegelen komt met het batchcommando. | Follows: mirroring comes with the batch command. |
| `sketch.pattern.rectangular` | Deferred | Volgt: rechthoekig patroon komt met het batchcommando. | Follows: rectangular pattern comes with the batch command. |
| `sketch.pattern.circular` | Deferred | Volgt: cirkelvormig patroon komt met het batchcommando. | Follows: circular pattern comes with the batch command. |
| `sketch.constraint.collinear` | Deferred | Volgt: op één lijn komt met het batchcommando. | Follows: collinear comes with the batch command. |
| `sketch.auto_constrain` | Deferred | Volgt: automatisch beperken komt met het batchcommando. | Follows: auto-constrain comes with the batch command. |

## F. Opruimen (geen nieuwe tekst)

- `tools.toml`, stale: `[tools."node"]` heeft `disabled_reason_nl/en`, maar `node` is een actief gereedschap (`Tool(ToolKind::Node)`). Verwijderen.
- `tools.toml`, verweesd: 11 `[tools."…"]`-regels zonder catalogus-id: `copy`, `delete`, `fit`, `new`, `open`, `pan`, `paste`, `redo`, `save`, `undo`, `zoom`. Koppelen aan een catalogus-id of verwijderen; de plannende sessie beslist.

## Vervolg (voor de UI-lane)

1. Neem de voorstellen over in `l10n.rs` (arms in `undo_pair`, `command_pair`-achtige helpers, `feature_pair`, `message`) en in `assets/l10n/tools.toml`.
2. Breid `known_message_keys()` uit met de nieuwe `UndoLabel`-varianten (A1–A3, B), zodat `all_message_keys_have_nl_and_en` ze dekt. Dat is de regressietest voor deze gaten (AGENTS.md: elke gevonden bug krijgt een test).
3. Voeg een test toe dat elke catalogus-id een `[tools."…"]`-regel heeft, en dat `reason_text` niet valt op de generieke tekst voor `Unavailable`- of `Deferred`-tools met eigen reden.
4. Controleer de NL-termen tegen de Nederlandse LightBurn-UI, als een bron daarvoor beschikbaar is; de repo bevat die niet.

## Wat niet gecontroleerd is

- Build en tests zijn niet gedraaid. De gaten zijn gevonden met lees- en grep-werk op de code op `8d0f019`.
- Of de NL-voorstellen de Nederlandse LightBurn-UI volgen: niet te verifiëren uit de repo.
