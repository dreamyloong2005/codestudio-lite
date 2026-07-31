# Large Rust/Svelte Module Refactor Design

## Scope

This project decomposes two oversized modules without changing observable application behavior:

1. Extract the Codex enhancement injection workflow from `src-tauri/src/core/chatgpt_desktop.rs` into a Rust controller module.
2. Extract the profile usage-query workflow from `src/routes/Profiles.svelte` into a TypeScript state machine and a Svelte dialog module.

The Rust tranche is implemented, verified, and committed before the Svelte tranche begins. Other large Rust, TypeScript, Panda, and Svelte files remain out of scope.

## Goals

- Put each workflow behind a small interface that hides ordering, state, retries, timers, and cleanup.
- Preserve the existing Tauri command interface, frontend behavior, text, styling, launch behavior, and updater behavior.
- Replace source-location-coupled tests with interface and composed-source assertions without weakening their behavioral coverage.
- Make async state transitions deterministic enough to test directly.
- Keep each tranche independently reviewable and revertible.

## Non-Goals

- No new application features or settings.
- No visual redesign or copy changes.
- No broad rewrite of ChatGPT Desktop installation, detection, history sync, Computer Use Guard, profile CRUD, or profile application.
- No general state-management framework.
- No deletion based only on static graph dead-code suggestions.

## Rust Architecture

### Seam

Create `src-tauri/src/core/chatgpt_desktop/enhancement/` as the internal module that owns:

- enhancement settings derived from `ChatGptDesktopSettings`;
- Codex model-catalog collection;
- debug-port reservation and launch arguments;
- CDP target selection and request transport;
- retry and page-recreation watchdog behavior;
- JavaScript template rendering;
- asynchronous injection lifecycle and activity logging.

The external interface is one launch wrapper:

```rust
enhancement::launch(settings, |args| {
    launch_installed_codex(installed, args)
})
```

The closure remains the adapter that performs the platform-specific desktop launch. The enhancement module owns the required sequencing around it. The caller does not manage debug ports, controller state, injection threads, or watchdogs.

### Internal State

The controller moves through these conceptual states:

```text
prepared -> waiting -> active -> stopped
```

- `prepared`: settings are derived, a debug port is reserved, and launch arguments are ready.
- `waiting`: the desktop process launched and the controller is retrying CDP discovery.
- `active`: injection succeeded and the controller monitors the selected page target.
- `stopped`: the CDP endpoint remained unavailable for the existing maximum miss count.

These states are internal implementation details, not exposed to `chatgpt_desktop.rs`.

### JavaScript Resource

Move the raw enhancement JavaScript into a dedicated resource under the enhancement module. Rust renders the resource by replacing explicit settings and marketplace placeholders with JSON produced by `serde_json`.

Rendering must fail if serialization fails or if a required placeholder is absent or remains after rendering. No updater key, API credential, or new environment value is introduced into the script.

### Behavior Preservation

- Remote plugin-cache and Computer Use Guard preparation remain before enhancement launch.
- Debug launch arguments remain present even when all optional enhancement switches are disabled, matching current behavior.
- A platform launch error is returned synchronously and no injection thread is created.
- Once the desktop process launches successfully, later injection failures are recorded in the activity log and do not turn the launch result into an error.
- Existing retry counts, retry delays, watchdog polling, target selection, and stop conditions remain unchanged.

## Svelte Architecture

### Controller Seam

Create `src/lib/profiles/profileUsageController.ts`. It owns:

- the active profile and form state;
- loading, saving, testing, querying, and deleting transitions;
- request construction and response application;
- usage-result state;
- automatic-query scheduling;
- stale-response rejection;
- timer cleanup.

The controller receives two internal adapters:

- `UsageApi`: load, save, test, query, and delete operations;
- `Scheduler`: interval creation and cancellation.

Production code supplies the existing API functions and browser scheduler. Tests supply deterministic adapters.

### State Model

The controller exposes an immutable view of a discriminated state:

```text
closed -> loading -> ready
                    -> saving -> ready/error
                    -> testing -> ready/error
                    -> querying -> ready/error
                    -> deleting -> ready/error
```

Only one remote operation may be active. Each `open(profile)` increments a request generation. Async responses update state only when their generation and profile still match. Closing, switching profiles, or disposing invalidates outstanding work.

Automatic querying runs only when the loaded configuration is enabled, the interval is positive, and the controller can query. Opening another profile, changing the applicable interval, closing, or disposing clears the previous timer.

### Dialog Module

Create `src/components/profiles/ProfileUsageDialog.svelte`. It owns the controller instance and renders the current controller state using the same Panda recipes, markup semantics, text, and actions as the existing inline dialog.

The parent `Profiles.svelte` retains only the selected usage profile and dialog open/close integration. It does not know the form fields, operation state, timer state, API sequencing, or result state.

Raw backend errors remain data in the controller. The UI applies the existing profile error localization before display. Success values are stable codes translated by the dialog.

## Error Handling

### Rust

- Preparation or desktop launch errors return through the existing `Result` path.
- Injection, reconnection, or watchdog errors after launch use the existing activity-log severity and wording wherever possible.
- A launch closure is called at most once.

### Svelte

- API failures return the controller to a usable state with the previous valid form/result retained where current behavior retains it.
- Stale responses are ignored rather than surfaced as errors.
- Commands issued while another operation is active are rejected without starting a second request.
- Close is blocked during active operations where the current dialog blocks it; disposal always cancels timers and invalidates responses.

## Testing Strategy

### Rust Tranche

Add tests before extraction for:

- one launch-closure call with unchanged debug arguments;
- synchronous propagation of launch failure;
- no asynchronous injection start after launch failure;
- complete JavaScript placeholder replacement;
- unchanged serialized settings and marketplace data;
- existing CDP target selection, retries, re-injection, and watchdog stop behavior.

Tests that currently read only `chatgpt_desktop.rs` will read the controller and JavaScript resource as appropriate. Assertions about enhancement behavior remain present.

Verification:

- targeted Rust tests;
- `cargo test --locked`;
- `cargo clippy --locked --all-targets -- -D warnings`;
- `cargo fmt --all -- --check`;
- frontend tests that assert desktop launch behavior.

### Svelte Tranche

Compile `profileUsageController.ts` through `tsconfig.tests.json` and run it with the existing Node test harness. Cover:

- initial load and form conversion;
- save, test, query, and delete transitions;
- request normalization;
- automatic-query start, replacement, and disposal;
- close and profile-switch stale-response rejection;
- mutual exclusion of remote operations;
- error recovery without state leakage.

Update structural tests to inspect `Profiles.svelte`, `ProfileUsageDialog.svelte`, and the controller together. Preserve Panda recipe, accessibility, and copy assertions.

Verification:

- `npm test`;
- `npm run build`;
- `git diff --check`;
- focused source and line-count review confirming responsibilities moved behind the intended interfaces.

## Delivery

The work is delivered as two implementation commits after this design and its implementation plan:

1. Rust enhancement controller extraction.
2. Svelte profile usage state machine and dialog extraction.

Each commit must pass its applicable verification before the next tranche begins. Existing unrelated working-tree changes are excluded from these commits.
