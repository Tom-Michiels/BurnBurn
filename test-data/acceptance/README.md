# Acceptatiefixtures (W4-acceptatie)

Invoer en goldens voor `crates/burnburn-scenarios/tests/acc_*.rs` (plan `research/lightburn/05-acceptatieplan.md`).
Niets hier is uit een run van de Rust-implementatie gelezen.

| Bestand | Herkomst | Onafhankelijk? |
|---|---|---|
| `image_modes_reference.py` | Pure-Python referentie (alleen stdlib); kernels overgenomen uit `docs/dev-notes/IM18-image-modes-reference.py` (elk uit de gepubliceerde definitie), hier toegepast op een gemeenschappelijke bitmap | Ja, geen Rust |
| `image-modes-golden.txt` | Uitvoer van `python3 -I test-data/acceptance/image_modes_reference.py` (2026-10-22, Python 3.13.7). Niet automatisch bijwerken | Ja |

Opnieuw genereren: zie de kop van het script; een gewijzigde verwachting vraagt een gereviewde uitleg (AGENTS.md).
