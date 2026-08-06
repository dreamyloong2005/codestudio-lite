# macOS Applications Scope Design

## Goal

Apply one macOS installation-scope rule to CodeStudio Lite itself, ChatGPT/Codex Desktop, and Claude Desktop:

- If only `/Applications` contains the app, detect, launch, and update that copy.
- If only `~/Applications` contains the app, detect, launch, and update that copy.
- If both contain the app, prefer `/Applications`, warn the user, and provide a button that removes the user copy.
- If neither contains the app, install to `/Applications`.

## Architecture

Add `src-tauri/src/core/macos_app_scope.rs` as the single native source of truth. It resolves valid non-symlink application bundles under system and user roots, returns ordered candidates and the preferred destination, reports duplicate state, and safely moves allowlisted user bundles to `~/.Trash`.

ChatGPT/Codex Desktop, Claude Desktop, and the CodeStudio Lite updater consume this resolver instead of hard-coded paths or `open -a` application-name lookup. `ToolStatus` carries the duplicate flag for managed desktop clients. A separate application-scope command reports CodeStudio Lite's own duplicate state.

## Safety

Only fixed application IDs and bundle names are accepted. Bundle identifiers must match the expected application. Symlinked `~/Applications` and symlinked app bundles are rejected. Managed client processes are closed before cleanup. If CodeStudio Lite is running from the user copy, a helper waits for the process to exit, moves that copy to Trash, and launches the preferred system copy.

## UI

Dashboard client cards show an inline warning and delete button when `duplicateUserInstall` is true. Settings shows the same warning for CodeStudio Lite itself. The copy states that `/Applications` is being used. Cleanup does not open a confirmation dialog; pending, success, and failure states are explicit and localized in English, Simplified Chinese, and Traditional Chinese.

## Testing

Rust tests cover system-only, user-only, both, neither, bundle aliases, symlink parents, installer destinations, exact launch paths, Trash cleanup, and self-cleanup helper arguments. Frontend tests cover warning visibility, cleanup invocation, pending state, refresh, errors, and all locales. Full Rust, frontend, type, build, formatting, and diff checks are required.
