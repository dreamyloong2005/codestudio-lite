# Codex Plugin Unlock Comparison

Goal: Compare codestudio-lite's forced Codex plugin unlock behavior with the local Codex++ source under D:\迅雷下载\CodexPlusPlus-main.

## Phase 1: Locate Implementations
Status: complete

Find the plugin unlock code paths in both repositories.

## Phase 2: Compare Mechanisms
Status: complete

Compare injection transport, launch prerequisites, JavaScript patch logic, and runtime behavior.

## Phase 3: Answer User
Status: complete

Summarize whether the implementations are equivalent, partially equivalent, or different.

## Phase 4: Add Modern Marketplace Unlock
Status: complete

Extend codestudio-lite's `patchForcePluginUnlock` injected script with Codex++-style modern marketplace request/filter/result patching.

## Phase 5: Verify Implementation
Status: complete

Run targeted tests and relevant project checks after implementation.

## Phase 6: Diagnose Windows Codex Update Hash Failure
Status: complete

Trace the Windows Codex desktop update flow to find why latest updates can fail SHA-256 verification.

## Phase 7: Fix Hash Failure
Status: complete

Add a regression test for the root cause and implement the minimal safe fix.

## Phase 8: Replace Windows Claude Desktop Patch Scheme
Status: complete

Completely remove the old Windows in-place Claude Desktop app.asar/fuse patch scheme and replace it with the same non-destructive debugger/runtime injection approach used by macOS.

## Phase 9: Verify Runtime-Only Windows Localization
Status: complete

Run targeted Rust/JS tests and warning checks proving Windows localized launch prepares runtime resources, opens the official debugger path, injects localization at runtime, and no longer rewrites Claude installation files.

## Phase 10: Fix Windows Claude Launch Blocking and Uninstall Failure
Status: complete

Prevent direct localized Claude Desktop launch from blocking CodeStudio Lite while debugger injection waits, and replace ambiguous Windows winget uninstall with Claude-specific MSIX/EXE uninstall handling.

## Phase 11: Fix Claude Desktop Windows Uninstall Verification
Status: complete

Verify Windows Claude Desktop uninstall against the selected install kind instead of the global Claude Desktop status, so uninstalling MSIX is not reported as failed when the native EXE install remains, and stale AppX residue does not count as a registered MSIX install.

## Phase 12: Stop Resolving Stale WindowsApps Claude Packages As Installed
Status: complete

Keep stale `C:\Program Files\WindowsApps\Claude_*` MSIX payloads available only for launch-time registration repair, but remove them from the normal Claude Desktop installed-state and per-kind tab detection so the UI no longer resolves unregistered package directories as installed.

## Phase 13: Fully Delete Claude Desktop Files During Windows Uninstall
Status: complete

Make Windows Claude Desktop uninstall remove the selected install kind's remaining files and verify deletion: MSIX/AppX payload directories under `C:\Program Files\WindowsApps` must be removed for Claude package identities, and native EXE install roots from registry/detection must be deleted before reporting uninstall success.

## Phase 14: Remove Claude Desktop Background Service Residue
Status: complete

Stop and remove Claude Desktop's `CoworkVMService` / `cowork-svc.exe` during Windows uninstall so partial WindowsApps payloads are no longer locked and uninstall success requires the background service/process residue to be gone or clearly reported.

## Phase 15: Add Claude Desktop Install/Update Progress UI
Status: complete

Make Claude Desktop install/update show a Codex Desktop-style progress panel. Reuse the existing tool-install progress event where possible, add optional structured progress fields, and make Windows MSIX download emit real byte/percent progress before Add-AppxPackage runs.

## Phase 16: Remove Stale Claude EXE State Before MSIX Install
Status: complete

Prevent deprecated/stale Claude Desktop EXE uninstall registry entries from resurfacing old versions during Windows MSIX install/update, and make Claude Desktop Windows update guidance use the official MSIX installer path rather than winget.

## Phase 17: Fix Windows Main Process Debugger UI Automation
Status: complete

Debug the Windows Claude Desktop UI automation until it can open the official Main Process Debugger route reliably. The fix respects Windows' in-window hamburger menu behavior instead of assuming the macOS top-menu pattern. Live debugging on this machine showed Claude opens the official Node inspector on `127.0.0.1:9229`; CodeStudio Lite already scans `9229..=9300`, so Phase 17 verification proves the endpoint appears in that scanned range rather than hard-coding `9233`.

## Phase 18: Harden Windows Debugger Automation For Logged-In Claude UI
Status: complete

Re-validate the Windows Main Process Debugger automation after Claude Desktop has an authenticated session. If the logged-in UI shifts the menu entry points, make the script prefer UI Automation-discovered controls and keep coordinates only as a final fallback, then verify the official Node inspector opens in the scanned port range.

## Phase 19: Add Language-Independent Windows Debugger Menu Fallback
Status: complete

Keep the fast localized-name UI Automation path, but add a structural fallback for unknown Claude UI languages. The fallback should discover the Developer submenu and Main Process Debugger toggle by menu geometry, control class, and TogglePattern/InvokePattern shape so non-English/non-Chinese labels do not block debugger enablement.

## Phase 20: Auto-Close Windows Inspector Prompt Window
Status: complete

After enabling Claude Desktop's Main Process Debugger on Windows, automatically close the extra Inspector/Debugger window that Electron opens, while keeping the Node inspector endpoint on `127.0.0.1:9229` available for runtime localization injection.

## Phase 21: Localize Windows Claude In-Window Menu
Status: complete

Extend the Claude Desktop main-process runtime injection so the Windows Electron in-window application menu is localized too. The fix should reuse the existing locale-aware menu label machinery, hook the Windows menu popup/build path, stay reversible when switching away from zh-CN, and not break the debugger automation's structural fallback.

## Phase 22: Keep Windows Claude Localization Injection Alive After Manual Debugger Activation
Status: complete

Fix the packaged Windows localized launch path so failing to auto-click Claude Desktop's Main Process Debugger does not prevent later injection. The background injector should keep waiting for the Node inspector and inject zh-CN when the user manually enables the debugger, while the debugger automation continues attempting the official menu route in parallel.

## Phase 23: Remove Unsafe Windows Coordinate Debugger Automation
Status: complete

Remove the Windows Claude Desktop debugger automation's screen-coordinate mouse fallback entirely. The script should use only Claude-window-scoped UI Automation controls/patterns to open the in-window menu, expand Developer, toggle Main Process Debugger, and close inspector prompt windows.

## Phase 24: Reinstall Runtime Injection When Packaged Logic Changes
Status: complete

Fix the Claude main-process injection freshness check so repackaged CodeStudio Lite builds reinstall the in-memory injection when the generated injection logic or bundled runtime/locale payloads change, even if `CSL_INJECTION_VERSION` was not manually bumped.

## Phase 25: Fix Windows Main Process Debugger Auto-Activation Regression
Status: complete

Investigate why the current Windows UI Automation script still fails to actively open Claude Desktop's Main Process Debugger even though manual debugger activation allows injection. Preserve the non-coordinate, Claude-window-scoped UIA approach, keep the inspector lookup limited to Claude's default port, and verify against the live installed Claude Desktop window.

## Phase 26: Close Windows Claude Blocking Promo Overlay Before Menu Automation
Status: complete

Add a Windows-only UI Automation preflight that closes Claude Desktop's occasional in-window promo/ad overlay before trying to open the hamburger menu. The close button may be an unnamed X, so the solution must use Claude-window-scoped UIA structure rather than button text or screen coordinates, and it must leave the macOS system-menu path unchanged.

## Phase 27: Sync Windows DevTools And Tray Menu Localization
Status: complete

Fix Windows Claude Desktop runtime injection so DevTools/local utility window titles follow the selected language, and localize Tray context menus as they are built or assigned. Keep the behavior reversible when switching away from zh-CN and avoid touching install/uninstall or debugger activation logic.

## Phase 28: Fix Literal Claude zh-CN Marketing Copy
Status: complete

Fix the awkward Claude zh-CN strings "通过繁琐的任务坚持" and "船只特征，而非线条" by correcting their static locale entries, and add regression coverage so these English idioms are not translated literally again.

## Phase 29: Tighten Windows Debugger Automation Timing
Status: complete

Reduce Windows Claude Main Process Debugger automation latency and make Inspector prompt cleanup robust. Prefer an already-open Claude window before AppX activation, close Inspector prompt windows with short polling around the toggle/confirmation phase, and keep the UIA-only/no-coordinate safety boundary.

## Phase 30: Preserve Page Edits During Background Refresh
Status: complete

Prevent background route refreshes from overwriting user edits made while the refresh is still in flight. Codex Client refreshes must update scan/install state without clobbering a newer settings draft, and Settings initial loads must not reset language/theme/auth-preservation controls after the user has already changed them.
