# Findings & Decisions

## Requirements
- Use `/Applications` unless only `~/Applications` contains the app.
- When both exist, prefer the system copy, warn, and provide direct cleanup.
- Apply to CodeStudio Lite itself, ChatGPT/Codex Desktop, and Claude Desktop.
- Preserve unrelated macOS release-pipeline changes.

## Research Findings
- ChatGPT/Codex detection already checks system aliases before user aliases, but its default macOS install root is fixed at `/Applications/ChatGPT.app`.
- Claude detection checks both roots, but the installer destination is fixed at `/Applications/Claude.app`.
- Claude launch uses `open -a Claude`, which is ambiguous when both copies exist.
- CodeStudio Lite self-update replaces the currently running bundle, so a running user copy remains the update target even when a system copy exists.
- The current worktree has unrelated uncommitted release-pipeline changes.

## Technical Decisions
| Decision | Rationale |
|----------|-----------|
| Add one shared native scope module | Detection, installation, launching, cleanup, and self-update must not drift again. |
| Move duplicates to Trash | Cleanup remains recoverable. |
| Use exact bundle paths for launch | `open -a` is ambiguous with duplicate copies. |
| Keep existing dirty files untouched | They belong to another active task. |

## Issues Encountered
| Issue | Resolution |
|-------|------------|

## Resources
- `src-tauri/src/core/chatgpt_desktop.rs`
- `src-tauri/src/core/detector.rs`
- `src-tauri/src/core/tool_installer.rs`
- `src-tauri/src/core/claude_desktop_patch.rs`
- `src-tauri/src/core/app_updater.rs`
- `src/routes/Dashboard.svelte`
- `src/routes/Settings.svelte`
