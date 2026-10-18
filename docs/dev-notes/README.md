# Ontwikkelnotities (TDD-log)

Per werkpakket één bestand `IMxx.md`. Elke red/green-cyclus krijgt één regel in de tabel:

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|

Regels:
- Alleen werkelijk uitgevoerde commando's en hun werkelijke uitkomst. Niets "aannemen".
- Een compileerfout, ontbrekende dependency of onbereikbare hardware is geen red-fase; noteer dan de
  minimale stub en de daaropvolgende gedragsfout.
- Handmatige acceptatieproeven staan apart onder "Handmatig" en worden nooit als geautomatiseerd geslaagd gemeld.
- Niet-geteste platforms/hardware staan onder "Onbevestigd".
