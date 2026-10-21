# W4-UI-gaten (golf 4, lane L1: presentation + macOS)

Omgeving: `export PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/w4-gaten`,
`cargo ... --locked`, macOS arm64. Basislijn voor de lane (commit 6dcdb60): `cargo test -p burnburn-presentation --locked
--no-fail-fast` gaf één falende test die niet van deze lane is: `barcode_dialog::all_29_types_are_listed_and_the_unsupported_ones_say_why`
(`25 of 29 are implemented`).

## TDD-log

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| S08 `w4_sketch::ambiguous_autoconstrain_is_not_applied` | auto-constrain past de duidelijke relatie toe en negeert de dubbelzinnige lijn (2 graden) | `cargo test -p burnburn-presentation --locked --test w4_sketch` → FAILED: `the document is unchanged left: RevisionId(1) right: RevisionId(0)` (minimale stub: `ToolRejection::AmbiguousRelations` + `Rejection::AmbiguousRelations`, nog niet geproduceerd) | zelfde commando → ok; `cargo test -p burnburn-editor --locked` ok (293 + overige) | — |

Aanname S08 (LightBurn kent geen auto-constrain; conservatief): een lijn tussen 1 en 3 graden van een as, of drie punten op één plek,
is dubbelzinnig. Dan past auto-constrain niets toe (alles-of-niets) en meldt `ToolRejection::AmbiguousRelations`.
