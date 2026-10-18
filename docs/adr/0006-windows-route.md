# ADR 0006 — Windows-route: WinUI 3 (voorkeur) of Win32 via windows-rs

- Status: **Voorgesteld (Design)**, 17 oktober 2026. Definitieve keuze na de IM08-proef **op Windows**.
- Geldt voor: `burnburn-windows`

## Context

Plan: eigen native Windows-frontend; WinUI 3 vanuit Rust is de voorkeursproef, Win32-controls via `windows` de
terugval; geen C#/C++-host. IM08 bewijst: één venster, objectlijst, maatveld, native file picker, focus en het
gedeelde canvas, met dezelfde `SetDimension`-command.

## Feiten (gecontroleerd 17-10-2026 met `cargo info` en een proefresolutie)

| Crate | Versie | Licentie | MSRV | Opmerking |
|---|---|---|---|---|
| `windows` | 0.62.2 | MIT OR Apache-2.0 | 1.82 | Win32/COM/WinRT-projecties; ook gebruikt door `wgpu` 30 (DX12) |
| `windows-reactor` | 0.100.0 | MIT OR Apache-2.0 | 1.95 | "Declarative Windows UI library"; hangt af van `windows-core` **0.100**, niet 0.62 |

Gevolg van de tweede regel: met `wgpu` 30 + `windows-reactor` 0.100 staan **twee** `windows-core`-versies in de
graaf. COM-interfaces/`HWND`-types zijn dan niet dezelfde Rust-types; de overdracht WinUI-host → wgpu-surface moet via
raw pointers/handles (`raw-window-handle`) en wordt een expliciet unsafe adaptercontract. Dit is geen blokkade, wel
een reden om de surface-integratie als aparte proef te zien (zoals het plan al zegt).

Bekend uit Microsoft-documentatie, **nog niet door ons geverifieerd**: WinUI 3 vereist de Windows App SDK-runtime
(framework package of self-contained bundeling, bootstrapper). Dat is een native, niet-Rust runtimecomponent naast
de app en raakt installatie/updates (X07). Win32 heeft die extra runtime niet.

## Besluit

1. De voorkeur uit het plan blijft staan: **WinUI 3 vanuit Rust** is de eerste proef.
2. IM08 wordt in twee onafhankelijke stappen bewezen, beide op een echte Windows-machine/runner:
   - **8a canvas**: Win32 top-level venster (`windows` 0.62.2) met child-`HWND` → `wgpu` DX12-surface, resize, DPI
     (per-monitor v2), focus. Dit is nodig in beide routes en vormt de terugval.
   - **8b controls**: `windows-reactor` 0.100.0 (exact gepind, `=0.100.0`) voor venster, lijst, maatveld en
     file picker; canvas als ingebed child-`HWND` of via een composition-route (`SwapChainPanel`). Toetsen: clipping,
     resize, focus-overgang lijst ↔ canvas ↔ veld, DPI.
3. Beslisregel na IM08: WinUI blijft als 8b alle vijf IM08-functies haalt met een vastgepinde, reproduceerbare
   dependencyset en een bekende Windows App SDK-route; anders Win32-controls (`windows`) voor de host. Beide routes
   gebruiken ongewijzigd `presentation`, `editor`, `render`. De keuze krijgt een amendement op deze ADR.
4. Unsafe-, hoofdthread- (STA/UI-thread) en panic-beleid volgen ADR 0002, met `catch_unwind` in elke `WndProc`,
   COM-callback en event handler.

## Wat op macOS **niet** getest kan worden

| Aspect | Op macOS mogelijk? |
|---|---|
| `cargo check --target x86_64-pc-windows-msvc -p burnburn-windows` (typecontrole) | Ja, als target geïnstalleerd is; **geen** verificatie van gedrag |
| Linken, starten, venster, controls, focus, IME, DPI, file picker | Nee |
| DX12-surface, device-loss, resize | Nee |
| Windows App SDK-bootstrap, MSIX/installer | Nee |
| Bestandsvervanging (`ReplaceFileW`/`MoveFileExW`), file locking door scanners | Nee |
| Gedeelde kern- en scenariotests (`cargo test` zonder hosts) | Alleen als indicatie; doorslaggevend is een Windows-runner |

Rapportage: alles in deze tabel met "Nee" staat als **onbevestigd** in dev-notes tot een Windows-runner het draaide.
Cross-compilatie wordt nooit als geslaagde UI-proef gemeld (AGENTS.md).

## Open (eigenaar)

- **Windows-runner**: welke machine/CI (GitHub `windows-latest` met GPU-beperkingen, of een eigen machine)? Zonder
  runner kan IM08 niet gereed worden gemeld.
- **Acceptatie van de Windows App SDK-runtime** als niet-Rust systeemcomponent (vergelijkbaar met een OS-framework),
  inclusief distributiemodel (MSIX vs. unpackaged + bootstrapper). Design ziet dit als toegestaan systeemframework
  onder AGENTS.md, maar dit raakt installatie en is een gebruikersbeslissing.
