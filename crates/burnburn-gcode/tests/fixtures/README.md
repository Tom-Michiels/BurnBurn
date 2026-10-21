# Fixtures en oracle van burnburn-gcode

Er zijn **geen opgenomen G-code-bestanden of controller-captures**. Alle verwachtingen zijn analytisch of
met de hand opgesteld; geen enkele komt van een echte GRBL-controller.

| Fixture | Herkomst | Verwachting |
|---|---|---|
| `support/plans.rs::rectangle_cut` | handgebouwd JobPlan, 40x20 mm op (10,10), 20 mm/s, 60 % | brandlengte 120 mm, S600 bij `$30=1000`, F1200, energie 3,6 verhouding-seconden, bounds (10,10)-(50,30) |
| `fill_scan` (X en Y) | handgebouwd, 6 rijen van 20 mm, overscan 2 mm, 30 % bij 100 mm/s | brand 120 mm, 0,36 verhouding-s, overscan-bounds 48..72, laser uit tijdens overscan |
| `grayscale` | drie runs van 10 mm met vermogens 0,2/0,5/1,0 | S200/S500/S1000, energie 0,17 |
| `angled` | diagonaal (3,4)-(63,84) | lengte 100 mm (3-4-5 driehoek x20) |
| `tests/messages.rs` voorbeeldregels | uit het geheugen van de publieke GRBL 1.1 `interface.md` | handgeschreven, niet gecontroleerd tegen firmware of capture |
| `tests/console.rs` macrotekst | zelfbedacht tekstformaat (v1) | vaste tekst in de test |

**Oracle**: `support/interp.rs` is een onafhankelijke minimale G-code-interpreter (G0/G1/G21/G90/G94/G54-59,
M2/M3/M4/M5/M8/M9, S/F modaal, GRBL-lasermodus: G0 en S0 = laser uit). Onbekende woorden zijn een harde fout.
De interpreter is zelf vastgepind aan met de hand uitgerekende programma's (`tests/interpreter_oracle.rs`) en
deelt geen code met de lowering. `support/fake_grbl.rs` modelleert een RX-buffer van 128 bytes die echt
overloopt; het is een model van de beschrijving, geen bewijs over een controller.
