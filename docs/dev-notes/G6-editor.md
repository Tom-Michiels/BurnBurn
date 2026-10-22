# G6-editor (golf 6): E10, S01/S05, P08, X01, LOADFILE

Lane G6-editor. Basis 481966e. Alleen werkelijk uitgevoerde commando's. Toolchain 1.99.1, `--locked`,
`CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/g6-editor`, `CARGO_INCREMENTAL=0`.

## LOADFILE vervangt het schone venster + HostError::Dirty

| Test | Rood | Groen | Refactor |
|---|---|---|---|
| `runtime/tests/automation::host_failures_are_distinct_replies` (nieuwe rij `HostError::Dirty -> ERR:DIRTY`) | stub: variant bestond, gemapt op `Refusal::Failed`: `cargo test -p burnburn-runtime --locked --test automation host_failures` -> FAILED (`left: "ERR:FAILED" right: "ERR:DIRTY"`) | map op `Refusal::Dirty` -> 16 ok | |
| `macos/tests/automation_host::a_race_with_a_change_after_the_status_check_is_dirty_not_failed` | `Err(Failed)` i.p.v. `Err(Dirty)` (`cargo test -p burnburn-macos --locked --test automation_host`: 2 FAILED, 13 ok) | pomp stuurt `HostError::Dirty` -> 15 ok | |
| `macos/tests/automation_host::loadfile_replaces_the_clean_active_window` | 2 vensters i.p.v. 1 (zelfde run) | `Stage::Opening { replaces }`: na `OpenWindow` een `UiIntent::Close` van het schone oude venster (alleen als het nog schoon is) -> 15 ok | |

Ontwerp: een venster dat tijdens het laden vuil werd blijft staan (geen bewaar-blad, geen verlies); het nieuwe project
krijgt dan een eigen venster. De Windows-host gebruikt dezelfde `automation_host`-pomp.
