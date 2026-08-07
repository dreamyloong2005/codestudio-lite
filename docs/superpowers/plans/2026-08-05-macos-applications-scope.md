# macOS Applications Scope Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Unify macOS install, detection, launch, update, and duplicate cleanup across CodeStudio Lite, ChatGPT/Codex Desktop, and Claude Desktop.

**Architecture:** A shared Rust scope resolver owns filesystem policy and safe cleanup. Existing detector/installer/launcher paths consume it, while Svelte surfaces duplicate state through existing status and settings flows.

**Tech Stack:** Rust, Tauri 2, Svelte, TypeScript, Vitest, Cargo tests.

## Global Constraints

- Preserve all existing release-pipeline edits.
- Do not commit, stage, or modify unrelated files.
- `/Applications` wins when both copies exist.
- `~/Applications` wins only when it is the sole valid installation.
- Duplicate cleanup is allowlisted, symlink-safe, bundle-ID checked, and recoverable through Trash.

---

### Task 1: Shared Native Scope Resolver

**Files:**
- Create: `src-tauri/src/core/macos_app_scope.rs`
- Modify: `src-tauri/src/core/mod.rs`
- Test: `src-tauri/src/core/macos_app_scope.rs`

**Interfaces:**
- Produces: `resolve(home, system_root, app_names, bundle_id)`, ordered candidates, preferred destination, duplicate state, and allowlisted Trash cleanup helpers.

- [ ] Write failing tests for system-only, user-only, both, neither, aliases, and symlinked user roots.
- [ ] Run focused tests and verify RED.
- [ ] Implement the resolver and safe cleanup primitives.
- [ ] Run focused tests and verify GREEN.

### Task 2: ChatGPT/Codex and Claude Integration

**Files:**
- Modify: `src-tauri/src/core/types.rs`
- Modify: `src-tauri/src/core/chatgpt_desktop.rs`
- Modify: `src-tauri/src/core/detector.rs`
- Modify: `src-tauri/src/core/tool_installer.rs`
- Modify: `src-tauri/src/core/claude_desktop_patch.rs`
- Modify: relevant Rust test modules.

**Interfaces:**
- Consumes: shared scope resolution.
- Produces: `ToolStatus.duplicate_user_install`, preferred install/update paths, and exact preferred launch paths.

- [ ] Write failing integration tests for both clients.
- [ ] Verify RED.
- [ ] Replace hard-coded detection/install/launch decisions.
- [ ] Verify GREEN.

### Task 3: CodeStudio Lite Self Scope and Cleanup Commands

**Files:**
- Modify: `src-tauri/src/core/app_updater.rs`
- Create or modify: scoped Tauri command module and `src-tauri/src/lib.rs`
- Modify: `src/types.ts` and `src/lib/api.ts`.

**Interfaces:**
- Produces: application scope status and allowlisted cleanup commands for self and managed clients.

- [ ] Write failing native/API tests.
- [ ] Verify RED.
- [ ] Implement self-scope status, post-exit helper cleanup, and command registration.
- [ ] Verify GREEN.

### Task 4: Dashboard and Settings UI

**Files:**
- Modify: `src/routes/Dashboard.svelte`
- Modify: `src/routes/Settings.svelte`
- Modify: `src/lib/locales/en-US.ts`
- Modify: `src/lib/locales/zh-CN.ts`
- Modify: `src/lib/locales/zh-TW.ts`
- Modify: related frontend tests and styles.

**Interfaces:**
- Consumes: duplicate flags and cleanup APIs.
- Produces: localized inline warnings, direct delete buttons, pending/error/success refresh flow.

- [ ] Write failing UI and localization tests.
- [ ] Verify RED.
- [ ] Implement the warning and cleanup flow.
- [ ] Verify GREEN.

### Task 5: Verification and Review

- [ ] Run the full frontend tests.
- [ ] Run the full Rust tests.
- [ ] Run type checking and production build.
- [ ] Run Rust formatting and `git diff --check`.
- [ ] Review the final diff without committing.
