//! Cross-crate scenario tests for BurnBurn (owner: Testing). This library is intentionally empty:
//! the scenarios live in `tests/` and chain the real domain crates end to end
//! (sketch → document → JobPlan → preflight → RD → independent decode). The mutation checks that
//! show each scenario fails when its chain breaks are logged in `docs/dev-notes/IM01-testing.md`.
