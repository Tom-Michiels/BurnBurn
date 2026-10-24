# LF-C — integratie (coördinerende sessie, 24-10-2026)

Drie restpunten uit de lanes van LF-C, opgepakt bij de integratie.

| Test | Rood | Groen |
|---|---|---|
| `burnburn-macos/tests/lfc_canvas_chrome.rs::{rulers_follow_the_unit_preference_and_the_locale, the_pointer_is_marked_on_the_rulers_when_it_is_over_the_canvas, snap_labels_are_in_the_window_language}` | `cargo test -p burnburn-macos --test lfc_canvas_chrome --locked` met `canvas_chrome_for` als doorgeefluik naar `canvas_chrome()` (altijd mm, komma, NL, geen cursor): 3 failed | 3 passed |
| `burnburn-macos/tests/lf11_preview.rs::the_preview_heading_names_the_compile_moment_not_the_plan_id` | met `plan_text` = "Plan <id>": FAILED ("Plan plan-4a506e4c64e8f6b0") | 14 passed (hele `lf11_preview`) |

- Linialen: `canvas_chrome_for(unit, locale, cursor_mm)`; `HostApp::redraw` geeft de eenheidsvoorkeur, de taal en de
  wereldpositie van de aanwijzer door; een aanwijzerbeweging tekent het canvas opnieuw (cursormarker op de linialen).
- Snaplabels volgen de venstertaal (`SnapLabels::ENGLISH` bij EN).
- Voorbeeldvenster (Q4): kop = `plan_title(compiled_at, duur)` zoals in de inspecteur; `PreviewViewModel.compiled_at`
  nieuw; de plan-id staat alleen nog in de tooltip (`PreviewPanel.plan_tooltip`). Windows toont de nieuwe kop ook.
- Handmatig: het opnieuw tekenen bij elke muisbeweging op een groot document voelen (de 100k-bewaking dekt de scène).
