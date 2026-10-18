# BurnBurn project instructions

## Product and architecture

- Implement BurnBurn application code in Rust, including native frontend adapters, CLI and developer build helpers.
- macOS uses native AppKit through Rust bindings. Keep platform integration in the macOS host.
- Prepare a separate native Windows frontend over the same core, presentation layer, editor interaction and renderer. Keep Windows APIs in the Windows host.
- Do not introduce a Swift, Objective-C, C++, C#, JavaScript or webview application layer. Calling system frameworks through Rust FFI is expected. Resources, manifests and GPU shader representations use their required formats.
- Prefer Rust-native algorithm dependencies. A Rust wrapper around a non-Rust solver or tracing library is not a Rust-native implementation. Keep research/reference tools separate from application runtime dependencies.
- Core modules must build and test without AppKit, Windows GUI frameworks or a GPU. Native types must not leak into shared document, geometry, solver, editor, CAM or machine contracts.
- Preview, RD export and upload refer to the same immutable compiled job revision. Document undo must never replay machine actions.
- Follow the implementation plan in `research/implementatieplan/README.md` and its linked TDD and work-package documents.

## Mandatory test-driven design for authored Rust

- Start every behavior change and bug fix with a test for the intended observable result. Run it and observe the expected failure before implementing the behavior.
- If an API does not exist yet, add only the minimum compileable stub, then demonstrate a behavioral failure. Compiler errors, missing dependencies and unavailable hardware do not count as the behavioral red phase.
- Implement the smallest correct change, run the relevant tests to green, then refactor with those tests running.
- Record test names and actual red/green commands/results in a short development note for the work package. Do not claim unexecuted checks passed. There is no need to publish failing commits.
- Prefer public behavior, analytical fixtures, independent reference outputs and meaningful invariants. Do not mirror the implementation formula as the only oracle or test trivial getters for coverage.
- Every discovered bug gets a regression test. Test boundaries and failure behavior as part of the contract.
- Inject clocks, transport and filesystem boundaries when needed. Tests must not unexpectedly contact a real laser or modify user projects.
- Native lifecycle, focus, text input, dialogs and canvas integration require targeted platform tests. Test translated adapter logic headlessly; use native integration tests for OS behavior. Report any explicit manual acceptance check as manual.
- Heavy geometry, trace and compile tasks must support cancellation and reject stale results by revision/request identity.
- Keep core Rust safe where practical. Isolate necessary unsafe code in reviewed platform/GPU adapters with explicit lifetime/thread invariants and tests. Do not unwind through FFI callbacks.

## Verification

- Use the pinned toolchain and lockfile once the Rust workspace exists.
- Run formatting, relevant Rust tests and clippy for affected targets. Run documentation tests where examples are executable.
- Test shared behavior on macOS and Windows as runners become available. Cross-compilation alone does not verify a native GUI or driver.
- Use property tests for numerical/state invariants, fuzzing for untrusted parsers and independently checked fixtures for Ruida. An encoder/decoder roundtrip alone is insufficient.
- Do not automatically accept snapshot/golden changes. Explain changed behavior and review the expected result.
- Hardware support requires evidence for the concrete model, firmware, interface and feature. Report unverified states honestly; never infer completion from an upload ACK or retry an uncertain start automatically.
