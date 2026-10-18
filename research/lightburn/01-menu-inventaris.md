# Menu-inventaris — LightBurn Core 2.1.04

**UI-bewijs.** Geopend op macOS met Ruida-profiel. Alle tien hoofdmenu’s staan hieronder. Dit register bevat de zichtbare actie-items en lege dynamische lijsten. Parentmenu’s zijn in het pad opgenomen. Selectie-afhankelijke items waren bij een leeg project vaak disabled; dat is niet als ontbrekende functionaliteit geïnterpreteerd.

De native macOS-invoegingen Services, AutoFill, Dictation en Emoji behoren tot platformintegratie. De concrete Services-lijst hangt van geïnstalleerde software af en hoort niet tot de eigen CAM-engine. `View Style:` is een niet-uitvoerbare tussenkop.

Elke rij heeft een stabiele research-ID in `reference/menu-index.json`. De hoofdstukken `02-features.md` en `03-instellingen.md` beschrijven gedrag en herbouwcontracten per samenhangende featurefamilie; de volledige officiële onderwerpen/optie-index staat in `reference/feature-index.md`.

## LightBurn

About toont productinformatie. Hide/Show/Quit gebruiken de desktop-lifecycle; bij Quit horen dirty-document-afhandeling en atomair opgeslagen voorkeuren. Services zijn OS-koppelingen.

| ID | Actie / submenu-pad |
|---|---|
| M001 | About LightBurn |
| M002 | Services / Activity Monitor |
| M003 | Services / Allocations & Leaks |
| M004 | Services / File Activity |
| M005 | Services / System Trace |
| M006 | Services / Time Profile Active Application |
| M007 | Services / Time Profile App Under Mouse |
| M008 | Services / Time Profile Entire System |
| M009 | Services / Toggle Instruments Recording |
| M010 | Services / Services Settings… |
| M011 | Hide LightBurn |
| M012 | Hide Others |
| M013 | Show All |
| M014 | Quit LightBurn |

## File

New/Open vervangen een document na dirty-check; Import voegt samen. Save bewaart een bewerkbaar project; Save RD file compileert machine-uitvoer. Export levert uitwisselgeometrie. Templates moeten hergebruik mogelijk maken zonder het origineel stilzwijgend te overschrijven. Prefs en Bundles zijn profiel/assetbeheer; print- en bitmapuitvoer zijn andere pipelines dan RD.

| ID | Actie / submenu-pad |
|---|---|
| M015 | New |
| M016 | New Window |
| M017 | Recent Projects |
| M018 | Recent Templates |
| M019 | Open |
| M020 | Import |
| M021 | Show Notes |
| M022 | Save |
| M023 | Save As |
| M024 | Save As Template |
| M025 | Save RD file |
| M026 | Export |
| M027 | Preferences / Import Prefs |
| M028 | Preferences / Export Prefs |
| M029 | Preferences / Open Prefs Folder |
| M030 | Preferences / Load Prefs Backup |
| M031 | Preferences / Edit Hotkeys |
| M032 | Bundles / Import Bundle |
| M033 | Bundles / Export Bundle |
| M034 | Print (Black Only) |
| M035 | Print (Keep Colors) |
| M036 | Save Processed Bitmap |
| M037 | Save Background Capture |

## Edit

Bewerkingen horen bij één undo-transactie. Close Path voegt een sluiting toe; Close With Tolerance en Auto-Join gebruiken geometrische toleranties. Optimize Shapes vereenvoudigt geometrie en is verschillend van de uitvoerplanner. Delete Duplicates wijzigt objecten; Remove Overlapping Lines in CAM kan uitsluitend de uitvoer wijzigen. Refresh/Replace-beelden moeten plaatsing, aspectbeleid en bronrelatie expliciet behandelen.

| ID | Actie / submenu-pad |
|---|---|
| M038 | Undo |
| M039 | Redo |
| M040 | Undo History |
| M041 | Select All |
| M042 | Invert Selection |
| M043 | Cut |
| M044 | Copy |
| M045 | Duplicate |
| M046 | Paste |
| M047 | Paste in Place |
| M048 | Delete |
| M049 | Convert to Path |
| M050 | Convert to Bitmap |
| M051 | Close Path |
| M052 | Close Selected Paths With Tolerance |
| M053 | Auto-Join Selected Shapes |
| M054 | Optimize Selected Shapes |
| M055 | Delete Duplicates |
| M056 | Reverse Shape Direction |
| M057 | Select Open Shapes |
| M058 | Select Open Shapes Set to Fill |
| M059 | Select All Shapes in Current Layer |
| M060 | Flash Shapes in Current Layer |
| M061 | Select Contained Shapes |
| M062 | Select Shapes Smaller Than Selected |
| M063 | Select Circles |
| M064 | Image options / Refresh Image |
| M065 | Image options / Replace Image |
| M066 | Image options / Replace Image to Fit |
| M067 | Settings |
| M068 | AutoFill / Contact… |
| M069 | AutoFill / Passwords… |
| M070 | AutoFill / Credit Card… |
| M071 | Start Dictation… |
| M072 | Emoji & Symbols |

## Tools

Select en de teken/nodegereedschappen bepalen interactiemodi. Position Laser is een machineactie en mag nooit een gewone canvas-edit zijn. Boolean/offset/warp wijzigen geometrie. Mask en Crop verschillen: een masker behoudt een kniprelatie; crop maakt een kleiner beeldresultaat. Barcode en tangentcirkels zijn generators. Labs vereist een expliciete experimentele capability.

| ID | Actie / submenu-pad |
|---|---|
| M073 | Select |
| M074 | Draw Lines |
| M075 | Draw Shape / Rectangle |
| M076 | Draw Shape / Ellipse |
| M077 | Draw Shape / Triangle |
| M078 | Draw Shape / Pentagon |
| M079 | Draw Shape / Polygon |
| M080 | Draw Shape / Octagon |
| M081 | Draw Shape / Star |
| M082 | Draw Shape / Dual Star |
| M083 | Edit Nodes |
| M084 | Trim Shapes |
| M085 | Add Tabs |
| M086 | Edit Text |
| M087 | Position Laser |
| M088 | Measure |
| M089 | Create Bar Code |
| M090 | Offset Shapes |
| M091 | Weld Shapes |
| M092 | Boolean Union |
| M093 | Boolean Subtract |
| M094 | Boolean Intersection |
| M095 | Boolean Assistant |
| M096 | Cut Shapes |
| M097 | Adjust Image |
| M098 | Trace Image |
| M099 | Multi-File Trace Image (Labs) |
| M100 | Apply Path to Text |
| M101 | Apply Mask to Image |
| M102 | Crop Image |
| M103 | Resize Slots in Selection |
| M104 | Warp Selection (4 points) |
| M105 | Deform Selection (16 points) |
| M106 | Generate Tangent Circle |

## Arrange

Groepen, oriëntatie en alignment zijn documenttransformaties; Move Laser en Jog zijn controlleracties. Draw Order is render-volgorde en staat los van Cut Order Priority. Nesting moet echte contouren en uitsparingen behandelen; een convex rubber-band outline is een afzonderlijke omhullende. Docking raakt objectgrenzen; Distribute kan centra of tussenruimte verdelen.

| ID | Actie / submenu-pad |
|---|---|
| M107 | Group |
| M108 | Ungroup |
| M109 | Auto-Group |
| M110 | Flip Horizontal |
| M111 | Flip Vertical |
| M112 | Mirror Across Line |
| M113 | Rotate 90° Clockwise |
| M114 | Rotate 90° Counter-Clockwise |
| M115 | Two-Point Rotate / Scale |
| M116 | Align / Align Centers |
| M117 | Align / Align Left |
| M118 | Align / Align Right |
| M119 | Align / Align Top |
| M120 | Align / Align Bottom |
| M121 | Align / Align H-Center |
| M122 | Align / Align V-Center |
| M123 | Distribute / Distribute H-Spaced |
| M124 | Distribute / Distribute H-Centered |
| M125 | Distribute / Distribute V-Spaced |
| M126 | Distribute / Distribute V-Centered |
| M127 | Distribute / Move H-Together |
| M128 | Distribute / Move V-Together |
| M129 | Nest Selected |
| M130 | Quick Nest |
| M131 | Dock / Dock Left |
| M132 | Dock / Dock Right |
| M133 | Dock / Dock Up |
| M134 | Dock / Dock Down |
| M135 | Move Selected Objects / Move to Laser Position |
| M136 | Move Selected Objects / Move to Page Center |
| M137 | Move Selected Objects / Move to Upper Left |
| M138 | Move Selected Objects / Move to Upper Right |
| M139 | Move Selected Objects / Move to Lower Left |
| M140 | Move Selected Objects / Move to Lower Right |
| M141 | Move Selected Objects / Move to Left |
| M142 | Move Selected Objects / Move to Right |
| M143 | Move Selected Objects / Move to Top |
| M144 | Move Selected Objects / Move to Bottom |
| M145 | Move Laser to Selection / Move Laser to Selection Center |
| M146 | Move Laser to Selection / Move Laser to Upper Left of Selection |
| M147 | Move Laser to Selection / Move Laser to Upper Right of Selection |
| M148 | Move Laser to Selection / Move Laser to Lower Left of Selection |
| M149 | Move Laser to Selection / Move Laser to Lower Right of Selection |
| M150 | Move Laser to Selection / Move Laser to Top of Selection |
| M151 | Move Laser to Selection / Move Laser to Bottom of Selection |
| M152 | Move Laser to Selection / Move Laser to Left of Selection |
| M153 | Move Laser to Selection / Move Laser to Right of Selection |
| M154 | Move Laser to Selection / Jog Laser Left |
| M155 | Move Laser to Selection / Jog Laser Right |
| M156 | Move Laser to Selection / Jog Laser Down |
| M157 | Move Laser to Selection / Jog Laser Up |
| M158 | Grid / Array |
| M159 | Circular Array |
| M160 | Copy Along Path |
| M161 | Create rubber band outline from selection |
| M162 | Break Apart |
| M163 | Push Forward in Draw Order |
| M164 | Push Backward in Draw Order |
| M165 | Push to Front |
| M166 | Push to Back |
| M167 | Lock Selected Shapes |
| M168 | Unlock Selected Shapes |

## Laser Tools

Bevat productie- en kalibratieworkflows. Print and Cut registreert ontwerp op gemeten targets. Rotary en Feeder vereisen modelafhankelijke mogelijkheden. Device Settings zijn hostprofielen, Machine Settings zijn controllerparameters. Tests genereren eigen bewerkingen en moeten dezelfde preview/compiler gebruiken als reguliere projecten.

| ID | Actie / submenu-pad |
|---|---|
| M169 | Print and Cut / Start Wizard |
| M170 | Print and Cut / Set First Target Position |
| M171 | Print and Cut / Set Second Target Position |
| M172 | Print and Cut / Align Output to Targets |
| M173 | Print and Cut / Align Output to Targets (no scaling) |
| M174 | Print and Cut / Reset Print and Cut |
| M175 | Manage Cameras |
| M176 | Rotary Setup |
| M177 | Feeder Setup |
| M178 | Focus Test |
| M179 | Interval Test |
| M180 | Material Test |
| M181 | Center Finder |
| M182 | Taper Warp |
| M183 | Manage Devices |
| M184 | Device Settings |
| M185 | Machine Settings |
| M186 | Get Controller Info |

## Web Tools

Cuttle is een externe ontwerpservice-koppeling. Het waargenomen menu bewijst alleen het bestaan van de launcher; import-/accountgedrag is niet getest.

| ID | Actie / submenu-pad |
|---|---|
| M187 | Cuttle |

## Window

View-controls en dockpanelen zijn presentatielaag. Panelnamen staan in het menu ook als de gekozen controller niet alle functionaliteit ondersteunt. Console/Macros zijn volgens DOC G-codefuncties. Preview moet de daadwerkelijke geplande uitvoer tonen, inclusief selectie, sublagen en outputfilters.

| ID | Actie / submenu-pad |
|---|---|
| M188 | Reset to Default Layout |
| M189 | Preview |
| M190 | Zoom to Page |
| M191 | Zoom In |
| M192 | Zoom Out |
| M193 | Frame Selection |
| M194 | View Style: / Wireframe / Coarse |
| M195 | View Style: / Wireframe / Smooth |
| M196 | View Style: / Filled / Coarse |
| M197 | View Style: / Filled / Smooth |
| M198 | Toggle Wireframe / Filled |
| M199 | Toggle Side Panels |
| M200 | Main |
| M201 | Numeric Edits |
| M202 | Creation Tools |
| M203 | Text Options |
| M204 | Modifiers |
| M205 | Arrange |
| M206 | Arrange (long) |
| M207 | Color Palette |
| M208 | Docking |
| M209 | Modes |
| M210 | Art Library |
| M211 | Cameras |
| M212 | Console |
| M213 | Cuts / Layers |
| M214 | Laser |
| M215 | File List |
| M216 | Macros |
| M217 | Material Library |
| M218 | Move |
| M219 | Shape Properties |
| M220 | Variable Text |

## Language

27 taalselecties waargenomen. Scheid vertaalde labels van stabiele command-IDs; numerieke parsing, decimale separatoren, sneltoetsen, fontfallback en RTL vereisen afzonderlijke ondersteuning.

| ID | Actie / submenu-pad |
|---|---|
| M221 | العربية (Arabic) |
| M222 | Čeština (Czech) |
| M223 | Dansk (Danish) |
| M224 | Deutsch (German) |
| M225 | Ελληνικά (Greek) |
| M226 | English |
| M227 | Español (Spanish) |
| M228 | Español (Latin American) |
| M229 | Suomi (Finnish) |
| M230 | Français (French) |
| M231 | Magyar (Hungarian) |
| M232 | Italiano (Italian) |
| M233 | 日本語 (Japanese) |
| M234 | 한국어 (Korean) |
| M235 | Nederlands (Dutch) |
| M236 | Norsk (Norwegian) |
| M237 | Język Polski (Polish) |
| M238 | Português Brasileiro (Brazilian Portuguese) |
| M239 | Português (Portuguese) |
| M240 | Русский (Russian) |
| M241 | Slovenski Jezik (Slovenian) |
| M242 | Svenska (Swedish) |
| M243 | ภาษาไทย (Thai) |
| M244 | Türkçe (Turkish) |
| M245 | Українська (Ukrainian) |
| M246 | 汉语 (Simplified Chinese) |
| M247 | 漢語 (Traditional Chinese) |

## Help

Supportdocumentatie, licentiebeheer en updates zijn productinfrastructuur. Debug-tools zijn diagnostiek: Convert to Cut mag niet worden verward met starten van een hardwarejob. Support Data moet uitdrukkelijk inzichtelijk maken welke project- en systeeminformatie wordt gebundeld.

| ID | Actie / submenu-pad |
|---|---|
| M248 | End User License Agreement |
| M249 | LightBurn Support Forum |
| M250 | Submit Feature Request |
| M251 | Quick Help and Notes |
| M252 | Online Documentation |
| M253 | PDF Documentation Link |
| M254 | Online Video Tutorials |
| M255 | Generate Support Data |
| M256 | Camera Selection Help |
| M257 | Check for Updates |
| M258 | License Management |
| M259 | Enable Debug Log |
| M260 | Debug Drawing |
| M261 | Convert to Cut (Debug) |
| M262 | Real-Time Statistics |

## Contextmenu’s en verborgen oppervlakken

Dit is een volledige transcriptie van de **waargenomen menubalk**, geen claim dat alle mogelijke contextmenu’s in iedere selectietoestand zijn geopend. Rechtermuisknoppen van lagen, bibliotheken, nodes, canvas en apparaatbestanden moeten in het vervolgcorpus worden vastgelegd. Dialogen kunnen extra opties tonen na Enable Z, Enable Laser 2, Rotary, een image-selectie, galvo-profiel of Beginner Mode uit/aan. De officiële index neemt deze families wel mee.

## Sneltoetsen en toegankelijke commando’s

Rechtstreeks waargenomen voorbeelden: New Cmd+N; Open Cmd+O; Save Cmd+S; Import Cmd+I; Undo Cmd+Z; Redo Cmd+Shift+Z; Group Cmd+G; Ungroup Cmd+U; Preview Alt+P; Offset Alt+O; Draw Lines Cmd+L; Trim Cmd+K; Add Tabs Cmd+Tab; Text Cmd+T; Position Laser Alt+L; Measure Alt+M. Gebruik de appwaarneming voor dit platform: documentatie kan platform- of versieafwijkingen bevatten. Bewaar sneltoetsen als configureerbare bindings bovenop command-IDs.
