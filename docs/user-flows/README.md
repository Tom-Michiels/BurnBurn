# User flows

Stap-voor-stap gebruikersroutes per integratiepoort uit `research/implementatieplan/02-werkpakketten.md`.
Iedere stap noemt: gebruikershandeling (Mac, met Windows-equivalent waar het verschilt) → `UiIntent`/`PointerEvent`
→ `DocumentCommand`/`MachineIntent`/`TaskRequest` → wat het viewmodel toont. Daarna foutpaden en annuleren.
Namen van intenties en viewmodels: [presentation-api.md](presentation-api.md).

| Document | Poort | Werkpakketten |
|---|---|---|
| [gate-a.md](gate-a.md) | A — rechthoek, maat, undo, bewaren, heropenen | IM01–IM08 |
| [gate-b.md](gate-b.md) | B — montageplaat, logo, ringtrace, offline RD | IM09–IM15 |
| [gate-c.md](gate-c.md) | C — dagelijkse productie op één Ruida-configuratie | IM16–IM20 |
| [gate-d.md](gate-d.md) | D — doos, klantlogo, handschrift; daarna werkplaats | IM21–IM32 |
| [story-coverage.md](story-coverage.md) | Alle 74 stories → flow → scenariotest → werkpakket | — |

Conventies:
- **Flow-ID** `A1`, `B3`, … is stabiel; scenariotests verwijzen ernaar in hun doc-commentaar.
- Scenariotests staan in `crates/burnburn-presentation/tests/` (headless). Native gedrag staat apart als
  `tests/native/...` (GUI-rol) of **handmatig**.
- "Geplande test" betekent: nog niet geschreven of nog niet groen. Alleen `docs/dev-notes/IMxx.md` meldt resultaten.
- BurnBurn ontwerpt eigen gedrag; LightBurn-dialogen zijn alleen gebruikt als inzicht in gangbare laserworkflows.
