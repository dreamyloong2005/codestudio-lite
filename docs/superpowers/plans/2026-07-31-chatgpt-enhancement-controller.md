# ChatGPT Enhancement Controller Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract Codex enhancement launch, rendering, CDP injection, retry, and watchdog behavior from `chatgpt_desktop.rs` into one tested controller module without changing launch behavior.

**Architecture:** A private `chatgpt_desktop::enhancement` module exposes one `launch(settings, launcher)` interface. It owns the controller lifecycle and renders a dedicated JavaScript resource; the parent module retains installation, process launch, history sync, plugin-cache preparation, and Computer Use Guard orchestration.

**Tech Stack:** Rust 2021, Tauri 2, serde/serde_json, reqwest blocking client, tungstenite, Node test runner.

---

### Task 1: Define the Enhancement Module Contract

**Files:**
- Create: `src/lib/chatgptEnhancementModule.test.mjs`
- Modify: `src-tauri/src/core/chatgpt_desktop.rs:1-35, 957-970`
- Create: `src-tauri/src/core/chatgpt_desktop/enhancement.rs`

- [ ] **Step 1: Write the failing source-contract test**

Create `src/lib/chatgptEnhancementModule.test.mjs`:

```js
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(path, "utf8");

test("ChatGPT desktop launch delegates enhancement sequencing to one controller", () => {
  const parent = read("src-tauri/src/core/chatgpt_desktop.rs");
  const controller = read("src-tauri/src/core/chatgpt_desktop/enhancement.rs");

  assert.match(parent, /mod enhancement;/);
  assert.match(parent, /enhancement::launch\(settings, \|args\|\s*launch_installed_codex\(installed, args\)\s*\)/);
  assert.match(controller, /pub\(super\) fn launch</);
  assert.match(controller, /struct EnhancementController/);
});
```

- [ ] **Step 2: Run the test and verify the expected failure**

Run:

```bash
node --test src/lib/chatgptEnhancementModule.test.mjs
```

Expected: FAIL because `src-tauri/src/core/chatgpt_desktop/enhancement.rs` does not exist.

- [ ] **Step 3: Add the module declaration and minimal launch interface**

Add near the imports in `chatgpt_desktop.rs`:

```rust
mod enhancement;
```

Create `src-tauri/src/core/chatgpt_desktop/enhancement.rs` with the final external seam and a temporary controller body:

```rust
use super::{
    codex_enhancement_settings_from, codex_patch_launch_args, select_debug_port,
    spawn_codex_enhancement_injection, ChatGptDesktopSettings,
    CodexEnhancementInjectionSettings,
};

pub(super) fn launch<F>(
    settings: &ChatGptDesktopSettings,
    launcher: F,
) -> Result<(), String>
where
    F: FnOnce(&[String]) -> Result<(), String>,
{
    EnhancementController::prepare(settings)?.launch_with(launcher, |controller| {
        spawn_codex_enhancement_injection(controller.debug_port, controller.settings);
    })
}

struct EnhancementController {
    debug_port: u16,
    settings: CodexEnhancementInjectionSettings,
}

impl EnhancementController {
    fn prepare(settings: &ChatGptDesktopSettings) -> Result<Self, String> {
        Ok(Self {
            debug_port: select_debug_port()?,
            settings: codex_enhancement_settings_from(settings),
        })
    }

    fn launch_with<F, S>(self, launcher: F, start: S) -> Result<(), String>
    where
        F: FnOnce(&[String]) -> Result<(), String>,
        S: FnOnce(Self),
    {
        launcher(&codex_patch_launch_args(self.debug_port))?;
        if self.settings.enabled() {
            start(self);
        }
        Ok(())
    }
}
```

Replace the debug-port and spawn block in `launch_detected_chatgpt_desktop` with:

```rust
enhancement::launch(settings, |args| launch_installed_codex(installed, args))?;
```

Do not delete the old enhancement functions yet; the temporary controller delegates to them so this intermediate commit preserves injection behavior. Task 3 moves their implementations and removes this temporary dependency.

- [ ] **Step 4: Verify the Rust module compiles**

Run:

```bash
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

Expected: PASS with the existing injection behavior still active through the temporary controller dependency.

- [ ] **Step 5: Commit the contract scaffold**

```bash
git add src/lib/chatgptEnhancementModule.test.mjs src-tauri/src/core/chatgpt_desktop.rs src-tauri/src/core/chatgpt_desktop/enhancement.rs
git commit -m "refactor: define chatgpt enhancement controller seam"
```

### Task 2: Extract and Validate the JavaScript Renderer

**Files:**
- Create: `src-tauri/src/core/chatgpt_desktop/codex_enhancements.js`
- Modify: `src-tauri/src/core/chatgpt_desktop/enhancement.rs`
- Modify: `src/lib/chatgptEnhancementModule.test.mjs`

- [ ] **Step 1: Add failing renderer assertions**

Append to `src/lib/chatgptEnhancementModule.test.mjs`:

```js
test("Codex enhancement JavaScript is a dedicated validated resource", () => {
  const controller = read("src-tauri/src/core/chatgpt_desktop/enhancement.rs");
  const script = read("src-tauri/src/core/chatgpt_desktop/codex_enhancements.js");

  assert.match(controller, /include_str!\("codex_enhancements\.js"\)/);
  assert.match(controller, /SETTINGS_PLACEHOLDER/);
  assert.match(controller, /MARKETPLACES_PLACEHOLDER/);
  assert.match(controller, /render_script/);
  assert.match(script, /__CODESTUDIO_LITE_SETTINGS__/);
  assert.match(script, /__CODESTUDIO_LITE_PLUGIN_MARKETPLACES__/);
  assert.match(script, /codestudioLiteCodexEnhancementsVersion/);
});
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run:

```bash
node --test src/lib/chatgptEnhancementModule.test.mjs
```

Expected: FAIL because the JavaScript resource is absent.

- [ ] **Step 3: Move the raw script verbatim and add strict rendering**

Move only the contents of the raw string assigned at `chatgpt_desktop.rs:3399` through the matching `"#;` before the placeholder replacements into `codex_enhancements.js`. Preserve every byte inside the raw string.

Add to `enhancement.rs`:

```rust
const SCRIPT_TEMPLATE: &str = include_str!("codex_enhancements.js");
const SETTINGS_PLACEHOLDER: &str = "__CODESTUDIO_LITE_SETTINGS__";
const MARKETPLACES_PLACEHOLDER: &str = "__CODESTUDIO_LITE_PLUGIN_MARKETPLACES__";

pub(super) fn render_script(settings_json: &str, marketplaces_json: &str) -> Result<String, String> {
    for placeholder in [SETTINGS_PLACEHOLDER, MARKETPLACES_PLACEHOLDER] {
        if SCRIPT_TEMPLATE.matches(placeholder).count() != 1 {
            return Err(format!("Codex enhancement script must contain exactly one {placeholder} placeholder."));
        }
    }
    let rendered = SCRIPT_TEMPLATE
        .replace(SETTINGS_PLACEHOLDER, settings_json)
        .replace(MARKETPLACES_PLACEHOLDER, marketplaces_json);
    if rendered.contains(SETTINGS_PLACEHOLDER) || rendered.contains(MARKETPLACES_PLACEHOLDER) {
        return Err("Codex enhancement script contains an unresolved placeholder.".to_string());
    }
    Ok(rendered)
}
```

Add unit tests inside `enhancement.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_replaces_every_required_placeholder() {
        let script = render_script(r#"{"enabled":true}"#, "[]").unwrap();
        assert!(script.contains(r#"{"enabled":true}"#));
        assert!(!script.contains(SETTINGS_PLACEHOLDER));
        assert!(!script.contains(MARKETPLACES_PLACEHOLDER));
    }
}
```

Replace the removed raw-string construction at the end of the existing parent `codex_enhancement_script` with:

```rust
enhancement::render_script(&settings_json, &plugin_marketplaces_json)
```

Keep serialization in the parent until Task 3 moves the full renderer inputs into the controller.

- [ ] **Step 4: Run focused renderer verification**

Run:

```bash
node --test src/lib/chatgptEnhancementModule.test.mjs
cargo test --locked --manifest-path src-tauri/Cargo.toml renderer_replaces_every_required_placeholder
```

Expected: the resource test passes; the Rust renderer test passes.

- [ ] **Step 5: Commit the resource extraction**

```bash
git add src-tauri/src/core/chatgpt_desktop/codex_enhancements.js src-tauri/src/core/chatgpt_desktop/enhancement.rs src-tauri/src/core/chatgpt_desktop.rs src/lib/chatgptEnhancementModule.test.mjs
git commit -m "refactor: extract codex enhancement script resource"
```

### Task 3: Move the Full Injection Lifecycle into the Controller

**Files:**
- Modify: `src-tauri/src/core/chatgpt_desktop/enhancement.rs`
- Modify: `src-tauri/src/core/chatgpt_desktop.rs:2779-3060, 3131-5503`
- Test: `src-tauri/src/core/chatgpt_desktop/enhancement.rs`

- [ ] **Step 1: Write failing controller sequencing tests**

Add these final-location assertions to the source-contract test so it fails until the move is complete:

```js
assert.doesNotMatch(parent, /fn select_debug_port\(/);
assert.doesNotMatch(parent, /fn spawn_codex_enhancement_injection\(/);
```

Add to the `enhancement.rs` test module:

```rust
fn enabled_settings() -> EnhancementSettings {
    EnhancementSettings {
        plugin_marketplace_unlock: true,
        plugin_auto_expand: false,
        model_whitelist_unlock: false,
        service_tier_controls: false,
        model_catalog: CodexModelCatalog::default(),
    }
}

#[test]
fn launch_failure_never_starts_injection() {
    let controller = EnhancementController::for_test(4242, enabled_settings());
    let started = std::cell::Cell::new(false);
    let result = controller.launch_with(
        |args| {
            assert_eq!(args[0], "--remote-debugging-port=4242");
            Err("launch failed".to_string())
        },
        |_| started.set(true),
    );
    assert_eq!(result.unwrap_err(), "launch failed");
    assert!(!started.get());
}

#[test]
fn successful_enabled_launch_starts_injection_once() {
    let controller = EnhancementController::for_test(4242, enabled_settings());
    let starts = std::cell::Cell::new(0);
    controller
        .launch_with(|_| Ok(()), |_| starts.set(starts.get() + 1))
        .unwrap();
    assert_eq!(starts.get(), 1);
}
```

- [ ] **Step 2: Run the tests and verify the expected failure**

Run:

```bash
cargo test --locked --manifest-path src-tauri/Cargo.toml enhancement::tests::launch_
```

Expected: FAIL because `EnhancementSettings`, `for_test`, and injection start behavior are not implemented.

- [ ] **Step 3: Move settings, catalog, CDP, retry, and watchdog implementation**

Move these coherent blocks from `chatgpt_desktop.rs` into `enhancement.rs`, preserving constants and function bodies:

- `CodexEnhancementInjectionSettings` renamed to `EnhancementSettings`;
- `CodexModelCatalog` with a derived `Default` implementation;
- model-catalog collection helpers;
- debug launch arguments and CDP target types;
- injection retry, spawn, target selection, websocket request/response, and watchdog functions;
- plugin-marketplace serialization helpers;
- `codex_enhancement_script`, changed to call `render_script`;
- retry and watchdog constants used only by this workflow.

Complete the controller interface:

```rust
pub(super) fn launch<F>(settings: &ChatGptDesktopSettings, launcher: F) -> Result<(), String>
where
    F: FnOnce(&[String]) -> Result<(), String>,
{
    EnhancementController::prepare(settings)?.launch_with(launcher, |controller| {
        std::thread::spawn(move || controller.run());
    })
}

impl EnhancementController {
    fn launch_with<F, S>(self, launcher: F, start: S) -> Result<(), String>
    where
        F: FnOnce(&[String]) -> Result<(), String>,
        S: FnOnce(Self),
    {
        launcher(&self.launch_args())?;
        if self.settings.enabled() {
            start(self);
        }
        Ok(())
    }

    fn run(self) {
        match inject_codex_enhancements(self.debug_port, &self.settings) {
            Ok(active_url) => {
                let _ = activity_log::append(Severity::Ok, "Applied Codex launch enhancement patch.");
                watch_codex_enhancement_target(self.debug_port, &self.settings, active_url);
            }
            Err(error) => {
                let _ = activity_log::append(
                    Severity::Error,
                    format!("Codex launch enhancement patch failed: {error}"),
                );
            }
        }
    }
}
```

Delete the moved implementations and now-unused imports from `chatgpt_desktop.rs`. Keep Computer Use Guard preparation/watchdog and `codex_home_dir` in the parent unless the enhancement module is their only consumer; do not move unrelated behavior.

- [ ] **Step 4: Run sequencing, Rust, and source-contract tests**

Run:

```bash
cargo test --locked --manifest-path src-tauri/Cargo.toml enhancement::tests
node --test src/lib/chatgptEnhancementModule.test.mjs
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

Expected: PASS with no unused-code warnings from the moved implementation.

- [ ] **Step 5: Commit the controller extraction**

```bash
git add src-tauri/src/core/chatgpt_desktop.rs src-tauri/src/core/chatgpt_desktop/enhancement.rs src/lib/chatgptEnhancementModule.test.mjs
git commit -m "refactor: isolate chatgpt enhancement lifecycle"
```

### Task 4: Retarget Existing Behavioral Tests and Verify the Rust Tranche

**Files:**
- Modify: `src/lib/chatgptDesktopLaunch.test.mjs`
- Modify: `src/lib/chatgptDesktopComputerUseGuard.test.mjs` only if an assertion targets moved enhancement code
- Modify: `src/lib/chatgptDesktopBranding.test.mjs` only if an assertion targets moved enhancement code

- [ ] **Step 1: Add one composed-source helper**

In `chatgptDesktopLaunch.test.mjs`, add:

```js
const enhancementSource = () =>
  [
    read("src-tauri/src/core/chatgpt_desktop/enhancement.rs"),
    read("src-tauri/src/core/chatgpt_desktop/codex_enhancements.js")
  ].join("\n");
```

Change only assertions about model catalogs, plugin injection, service tiers, enhancement observers, CDP requests, retry, and watchdog code to use `enhancementSource()`. Assertions about installation, process launch, history sync, or parent orchestration continue reading `chatgpt_desktop.rs`.

- [ ] **Step 2: Run the frontend suite and fix only source-location assumptions**

Run:

```bash
npm test
```

Expected: PASS. If a test fails because it reads the old source location, point it to the composed source; do not remove its behavioral regex.

- [ ] **Step 3: Run full Rust quality gates**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Expected: PASS with zero test failures and zero clippy warnings. If an environment-only test failure occurs, record its exact test and error before proceeding; do not treat it as a refactor success.

- [ ] **Step 4: Verify blast radius and file reduction**

Run:

```bash
wc -l src-tauri/src/core/chatgpt_desktop.rs src-tauri/src/core/chatgpt_desktop/enhancement.rs src-tauri/src/core/chatgpt_desktop/codex_enhancements.js
git diff --check HEAD~3
```

Expected: `chatgpt_desktop.rs` no longer contains the raw JavaScript or CDP controller functions, and all three files have clean diffs.

- [ ] **Step 5: Commit test retargeting**

```bash
git add src/lib/chatgptDesktopLaunch.test.mjs src/lib/chatgptDesktopComputerUseGuard.test.mjs src/lib/chatgptDesktopBranding.test.mjs
git commit -m "test: follow chatgpt enhancement module seam"
```

Do not stage unrelated release-pipeline or planning files.
