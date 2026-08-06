# Findings & Decisions

## Requirements
- Begin the previously deferred large Rust/Svelte module decomposition.
- Preserve observable application behavior.
- Keep current release-pipeline changes intact and uncommitted unless separately requested.
- First implement the Rust Codex enhancement injection extraction, then the Svelte profile usage dialog extraction; verify and commit them separately without behavior changes.

## Research Findings
- Knowledge graph: 3,551 nodes and 35,026 edges across 202 files; major communities include core-profile, lib-claude, and routes-tool.
- Largest Rust files include `chatgpt_desktop.rs` (5,557 lines), `claude_desktop_patch.rs` (4,795), `gateway.rs` (4,228), `tool_installer.rs` (3,887), and `detector.rs` (2,727).
- Largest Svelte routes include `Profiles.svelte` (2,433 lines), `Dashboard.svelte` (1,940), and `SetupWizard.svelte` (1,242).
- `chatgpt_desktop.rs::codex_enhancement_script` alone spans roughly 2,034 lines; it is likely an embedded-script extraction candidate but needs dependency and test inspection.
- `Profiles.svelte` owns profile CRUD, usage querying, sorting, icon import, modal keyboard behavior, and rendering; likely multiple coherent seams exist.
- Graph refactor suggestions and dead-code findings are noisy for framework entry points and are not sufficient evidence for deletion.
- The two leading files together have a high graph blast radius: about 40 additional files within two dependency hops.
- Rust seam candidate: move the Codex enhancement injection subsystem out of `chatgpt_desktop.rs`; its renderer contains a roughly 2,000-line raw JavaScript payload plus CDP transport/watchdog logic.
- Svelte seam candidate: move the usage-query dialog out of `Profiles.svelte`; it owns loading, saving, testing, querying, deletion, auto-query timers, input normalization, and result rendering as one coherent workflow.
- Existing tests frequently read `chatgpt_desktop.rs` and `Profiles.svelte` as text and assert implementation placement. Those tests must be updated to inspect the extracted module/asset or verify the interface, while preserving behavioral assertions.
- `Profiles.svelte` already delegates list and tool-tab presentation to `src/components/profiles/`, so an extracted usage dialog matches the current local pattern.

## Technical Decisions
| Decision | Rationale |
|----------|-----------|
| Prefer deep extracted modules over line-count slicing | Each extracted interface should hide a coherent responsibility and reduce caller knowledge. |
| Require an existing or new test at every extraction seam | Behavior-preserving refactors need direct regression evidence. |
| Do not split both whole files in one pass | The combined blast radius is high; use independently verifiable tranches. |
| Rust external seam is one launch wrapper | It hides debug-port preparation, launch ordering, injection retry, and watchdog startup behind a closure that performs the actual desktop launch. |
| Svelte usage workflow uses a pure controller plus a dialog module | State transitions and timers become testable without coupling them to page markup. |
| Rust injection errors remain asynchronous activity-log failures | A successful desktop launch must not become a failed launch merely because optional enhancement injection later fails. |
| Svelte controller uses request generations and injected adapters | Stale async results and timer behavior can be made deterministic and tested directly. |

## Issues Encountered
| Issue | Resolution |
|-------|------------|
| Graph output was very large and truncated | Use targeted file/function inspection and minimal graph queries from here. |

## Resources
- `docs/superpowers/specs/2026-07-31-macos-release-pipeline-design.md` records the earlier decision to defer this refactor.
