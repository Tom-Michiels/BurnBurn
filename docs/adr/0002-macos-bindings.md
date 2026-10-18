# ADR 0002 — macOS-bindings: objc2, hoofdthread, unsafe en panics

- Status: **Aanvaard (Design)**, 17 oktober 2026. Minimale macOS-versie: **13 Ventura** (orchestratorbesluit 18-10-2026, voorlopig; eigenaar kan herzien).
- Geldt voor: `burnburn-macos` (en surface-aanmaak voor `burnburn-render` op macOS)

## Context

Het plan legt AppKit via `objc2`, `objc2-app-kit` en `objc2-foundation` vast. AppKit-objecten zijn alleen op de
hoofdthread geldig; Objective-C-callbacks komen via FFI binnen. AGENTS.md eist: unsafe alleen in reviewed
platformadapters met expliciete lifetime-/thread-invarianten, en nooit unwinden door een FFI-callback.

## Besluit

### Versies (gecontroleerd met `cargo search`/`cargo info`, 17-10-2026)

| Crate | Versie | Licentie | MSRV |
|---|---|---|---|
| `objc2` | 0.6.5 | MIT | 1.71 |
| `objc2-foundation` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | 1.71 |
| `objc2-app-kit` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | 1.71 |
| `objc2-quartz-core` | 0.3.2 (alleen als `CAMetalLayer` direct nodig is) | idem | 1.71 |
| `block2` | 0.6.2 (completion handlers, bijv. `NSSavePanel`) | MIT | — |
| `dispatch2` | 0.3.1 (alleen voor wake-up naar main queue) | Zlib OR Apache-2.0 OR MIT | — |

`wgpu` 30.0.1 gebruikt op macOS zelf `objc2` 0.6.5 en `objc2-metal`/`objc2-quartz-core` 0.3.2; met bovenstaande
versies ontstaat **één** objc2-familie in de lockfile (gecontroleerd met een proefresolutie). Upgrades van objc2 en
wgpu gebeuren daarom samen.

`objc2-app-kit` met `default-features = false` en alleen de benodigde framework-features
(`NSApplication`, `NSWindow`, `NSView`, `NSMenu`, `NSTextField`, `NSOutlineView`, `NSSplitView`, `NSSavePanel`, …)
om compileertijd te beperken; de lijst groeit per werkpakket.

### Hoofdthread

- Alle AppKit-objecten zijn `MainThreadOnly`; functies die ze maken of raken nemen een `MainThreadMarker`
  (`mtm`) als argument. `MainThreadMarker::new()` alleen in `main` en in callbacks die AppKit op de hoofdthread
  aanroept; nergens `new_unchecked`.
- `AppController` leeft op de hoofdthread (niet `Send`). Workers en de machine-actor leveren resultaten via een
  wake-up: `dispatch2` main-queue `exec_async` met alleen `Send`-data (`TaskOutcome`, actor-`Report`), waarna de
  hoofdthread `AppController::task_finished` aanroept. Geen AppKit-object kruist ooit een thread.
- Geen blokkerend wachten (bestand, netwerk, `pollster::block_on` op GPU-adapter) op de hoofdthread buiten de
  opstart; GPU-initialisatie die moet wachten gebeurt vóór het eerste venster of met een expliciet budget.

### Unsafe-beleid

- `burnburn-macos` kan `[lints] workspace = true` niet gedeeltelijk overschrijven (Cargo erft alles of niets).
  Daarom kopieert de crate de workspace-lints met `unsafe_code = "deny"` (niet `forbid`, zodat een module het met
  `#[allow(unsafe_code)]` kan toestaan) en voegt toe: `unsafe_op_in_unsafe_fn = "deny"`,
  `clippy::undocumented_unsafe_blocks = "deny"`, `clippy::multiple_unsafe_ops_per_block = "deny"`.
- `unsafe` alleen in modules `ffi/` of `appkit/<onderwerp>.rs`; elk blok heeft een `// SAFETY:`-commentaar met
  thread-, lifetime- en ownershipinvariant. Gebruik `Retained<T>` (geen raw `*mut AnyObject` opslaan).
- Delegates/target-actions via `define_class!`; ivars bevatten alleen `Weak`/ids naar Rust-state, nooit een
  `&mut` die de callback overleeft. Callback na sluiten van een document → no-op (test:
  `callback_after_close_is_ignored`).
- Surface: de host maakt de `wgpu::Surface` uit de `NSView`/`CAMetalLayer` via `raw-window-handle` 0.6
  (`create_surface_unsafe`) en houdt de view in leven zolang de surface bestaat (invariant gedocumenteerd naast de
  aanroep). `burnburn-render` krijgt alleen het `wgpu`-surface.

### Geen panic over FFI

- Elke Rust-functie die AppKit aanroept (delegate, action, `drawRect`/layer-callback, block) wikkelt zijn body in
  één helper `ffi_guard(name, default, || …)` met `std::panic::catch_unwind`. Bij een panic: log een crashrecord
  (lokaal), zet de app in toestand *poisoned* (geen nieuwe commands), vraag een recovery-save van de laatste
  `DocumentSnapshot` aan via de normale route en geef `default` terug. Daarna een nette `NSApp.terminate`.
- Release-profiel blijft `panic = "unwind"` zodat de guard kan werken; productiecode bevat geen
  `unwrap/expect/panic` (workspace-lints), dus de guard is een vangnet, geen foutpad.
- Objective-C-excepties: aanroepen waarvan AppKit documenteert dat ze kunnen raisen, gaan via
  `objc2::exception::catch` (feature `exception`) en worden een getypeerde fout.

### Testen

- Vertaalde adapterlogica (event → `UiIntent`, modifiers, keymap, puntomrekening Retina) is pure Rust en wordt
  headless getest in de host-crate zonder AppKit-objecten.
- Native integratietests in `tests/native/macos/` draaien op de hoofdthread met de echte eventloop
  (`harness = false`, eigen `main`), één scenario per proces. Ze worden in CI alleen op een macOS-runner met
  GUI-sessie gedraaid; zonder die runner rapporteren we "onbevestigd".

## Gevolgen

- GUI-rol voegt bij IM06 `objc2`, `objc2-foundation`, `objc2-app-kit`, `block2`, `dispatch2` toe aan
  `[workspace.dependencies]` met exacte versies hierboven, onder `cfg(target_os = "macos")` in de host.
- `burnburn-macos` compileert op Windows/Linux tot een lege binary zodat `cargo test --workspace` daar slaagt.

## Minimale macOS-versie (besluit fase B)

- **macOS 13 Ventura**, Apple Silicon eerst. Vastleggen op drie plekken die gelijk moeten blijven:
  `MACOSX_DEPLOYMENT_TARGET=13.0` in de build (xtask/CI, of `.cargo/config.toml` `[env]`), `LSMinimumSystemVersion`
  = `13.0` in de `Info.plist` van de bundle (xtask, IM20) en een test in xtask die beide vergelijkt.
- API's nieuwer dan 13 alleen achter een runtime-beschikbaarheidscontrole (`NSProcessInfo.isOperatingSystemAtLeastVersion`)
  in de host; objc2-features die alleen ≥ 14 bestaan, niet ongeconditioneerd aanroepen.
- Intel: niet gepubliceerd als ondersteund tot er een build- en testrunner is (plan).

## Open (eigenaar)

- Geen; macOS 13 is voorlopig en kan door de eigenaar worden herzien (bv. naar 14 als een benodigde API ontbreekt).
