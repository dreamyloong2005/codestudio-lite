use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::json;

use super::*;

fn build_locale_runtime_source() -> &'static str {
    TRANSLATION_RUNTIME
}

fn cdp_locale_patterns() -> Vec<Value> {
    [
        "*ion-dist/i18n/zh-CN.json*",
        "*ion-dist/i18n/en-US.json*",
        "*/i18n/zh-CN.json*",
        "*/i18n/en-US.json*",
        "*/zh-CN.json*",
    ]
    .into_iter()
    .map(|url_pattern| json!({ "urlPattern": url_pattern }))
    .collect()
}

fn cdp_locale_response(id: u64, event: &Value) -> Option<Value> {
    let request_id = cdp_request_id(event)?;
    let url = event.get("params")?.get("request")?.get("url")?.as_str()?;
    let payload = locale_payload_for_url_with_locale(url, "zh-CN")?;
    Some(json!({
        "id": id,
        "method": "Fetch.fulfillRequest",
        "params": {
            "requestId": request_id,
            "responseCode": 200,
            "responseHeaders": [
                { "name": "Content-Type", "value": "application/json; charset=utf-8" },
                { "name": "Cache-Control", "value": "no-store" },
                { "name": "Access-Control-Allow-Origin", "value": "*" }
            ],
            "body": BASE64_STANDARD.encode(payload.as_bytes())
        }
    }))
}

fn cdp_request_id(event: &Value) -> Option<&str> {
    event
        .get("params")?
        .get("requestId")?
        .as_str()
        .filter(|value| !value.is_empty())
}

fn locale_payload_for_url(url: &str) -> Option<&'static str> {
    locale_payload_for_url_with_locale(url, "en-US")
}

fn locale_payload_for_url_with_locale(url: &str, current_locale: &str) -> Option<&'static str> {
    let normalized = url.replace('\\', "/");
    let bare = normalized
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let is_zh = bare.ends_with("/zh-cn.json");
    let is_en = bare.ends_with("/en-us.json");
    let local_like = bare.starts_with("app://") || bare.starts_with("file://");
    if !is_zh && !(current_locale == "zh-CN" && is_en && local_like) {
        return None;
    }
    if bare.contains("/dynamic/") {
        Some(CLAUDE_ION_DYNAMIC_ZH_LOCALE)
    } else if bare.contains("/ion-dist/i18n/") || bare.contains("/i18n/") {
        Some(CLAUDE_ION_ZH_LOCALE)
    } else {
        Some(CLAUDE_SHELL_ZH_LOCALE)
    }
}

fn patch_source() -> &'static str {
    include_str!("claude_desktop_patch.rs")
}

fn production_source() -> &'static str {
    patch_source()
}

fn source_between<'a>(source: &'a str, start: &str, end: &str, label: &str) -> &'a str {
    source
        .split(start)
        .nth(1)
        .and_then(|tail| tail.split(end).next())
        .unwrap_or_else(|| panic!("{label} body should exist"))
}

fn patch_between(start: &str, end: &str, label: &str) -> &'static str {
    source_between(patch_source(), start, end, label)
}

fn production_between(start: &str, end: &str, label: &str) -> &'static str {
    source_between(production_source(), start, end, label)
}

fn function_body_between(start_fn: &str, end_fn: &str, label: &str) -> &'static str {
    source_between(
        production_source(),
        &format!("\nfn {start_fn}"),
        &format!("\nfn {end_fn}"),
        label,
    )
}

fn windows_debugger_request_body() -> &'static str {
    patch_between(
        "fn request_windows_claude_main_process_debugger_once()",
        "#[cfg(target_os = \"macos\")]",
        "request_windows_claude_main_process_debugger_once",
    )
}

fn assert_contains_all(source: &str, expected: &[&str]) {
    for needle in expected {
        assert!(
            source.contains(needle),
            "expected source to contain {needle:?}"
        );
    }
}

fn assert_contains_none(source: &str, forbidden: &[&str]) {
    for needle in forbidden {
        assert!(
            !source.contains(needle),
            "expected source not to contain {needle:?}"
        );
    }
}

fn assert_order(source: &str, before: &str, after: &str, message: &str) {
    let before_idx = source
        .find(before)
        .unwrap_or_else(|| panic!("{message}: missing earlier fragment {before:?}"));
    let after_idx = source
        .find(after)
        .unwrap_or_else(|| panic!("{message}: missing later fragment {after:?}"));
    assert!(before_idx < after_idx, "{message}");
}

fn assert_contains_in_order(source: &str, expected: &[&str], message: &str) {
    let mut offset = 0;
    for needle in expected {
        let relative_idx = source[offset..]
            .find(needle)
            .unwrap_or_else(|| panic!("{message}: missing ordered fragment {needle:?}"));
        offset += relative_idx + needle.len();
    }
}

fn main_process_injection_source() -> String {
    build_main_process_injection_source_for_paths(
        Path::new(r"C:\CodeStudio\translation-runtime.js"),
        Path::new(r"C:\CodeStudio\zh-CN.json"),
        Path::new(r"C:\CodeStudio\ion-dist\i18n\zh-CN.json"),
        Path::new(r"C:\CodeStudio\ion-dist\i18n\dynamic\zh-CN.json"),
    )
}

struct LocaleExpectation {
    key: &'static str,
    label: &'static str,
    expected: &'static str,
    forbidden: &'static [&'static str],
}

fn assert_locale_expectations(
    map: &serde_json::Map<String, Value>,
    expectations: &[LocaleExpectation],
) {
    for LocaleExpectation {
        key,
        label,
        expected,
        forbidden,
    } in expectations
    {
        let actual = map
            .get(*key)
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("missing {label} ({key})"));
        assert_eq!(actual, *expected, "unexpected {label} ({key})");
        for fragment in *forbidden {
            assert!(
                !actual.contains(fragment),
                "{label} ({key}) should not contain {fragment:?}: {actual}"
            );
        }
    }
}

#[test]
fn non_localized_command_is_unchanged() {
    assert_eq!(
        patched_launch_command("claude-desktop", "Claude", false).unwrap(),
        "Claude"
    );
}

#[test]
fn localized_windows_command_uses_official_launcher_without_fused_inspect_args() {
    if cfg!(target_os = "windows") {
        let script = windows_launch_script(true);
        assert!(script.contains("localized-launch.flag"));
        assert!(!script.contains("--inspect"));
        assert!(!script.contains("remote-debugging-port"));
    } else {
        let script = macos_localized_launch_script();
        assert!(script.contains("localized-launch.flag"));
        assert!(!script.contains("--inspect"));
        assert!(!script.contains("remote-debugging-port"));
    }
}

#[test]
fn windows_localization_is_runtime_only_and_does_not_patch_installed_app() {
    let production_source = production_source();
    assert_contains_all(
        production_source,
        &["request_windows_claude_main_process_debugger_once"],
    );
    assert_contains_none(
        production_source,
        &[
            "fn enable_claude_main_process_debugger()",
            "fn ensure_windows_claude_main_process_debugger()",
            "fn request_windows_claude_main_process_debugger_until_available()",
            "WINDOWS_MAIN_PROCESS_DEBUGGER_WAIT_TIMEOUT",
            "WINDOWS_MAIN_PROCESS_DEBUGGER_RETRY_MS",
            "WINDOWS_MAIN_PROCESS_DEBUGGER_POLL_MS",
            "mpsc::channel",
        ],
    );

    let ensure_body = production_between(
        "pub fn ensure_localization_patch()",
        "pub fn ensure_localized_launch_prerequisites",
        "ensure_localization_patch",
    );
    assert!(ensure_body.contains("ensure_patch_files()?"));
    assert!(!ensure_body.contains(concat!("apply_", "localization_patch")));

    let windows_launch_body = function_body_between(
        "launch_windows_claude_desktop",
        "launch_windows_claude_msix",
        "Windows launch",
    );
    assert_contains_all(
        windows_launch_body,
        &[
            "ensure_patch_files()?",
            "write_localized_launch_marker()?",
            "spawn_silent_localization_injector_with_app(app)",
        ],
    );
    assert_order(
        windows_launch_body,
        "ensure_patch_files()?",
        "write_localized_launch_marker()?",
        "Windows localized launch should prepare runtime files before writing the zh marker",
    );
    assert_contains_none(
        windows_launch_body,
        &[
            "ensure_windows_claude_main_process_debugger()?",
            "enable_claude_main_process_debugger()",
            "retry_inject_localization()?",
            concat!("apply_", "localization_patch"),
            concat!("activate_", "localized_claude"),
        ],
    );

    for removed_symbol in [
        concat!("resolve_", "claude_install_for_patch"),
        concat!("resolve_", "native_claude_install_for_patch"),
        concat!("activate_", "localized_claude"),
        concat!("build_", "inspector_shim"),
        concat!("build_", "patched_claude_asar"),
        concat!("elevated_", "patch_script"),
        concat!("try_", "direct_patch_write"),
        concat!("verify_", "localization_patch_landed"),
        concat!("run_", "elevated_powershell_script"),
        concat!("fuse_", "integrity_offset"),
        concat!("asar_", "shim_needs_update"),
        concat!("CLAUDE_", "FUSE_MARKER"),
        concat!("CLAUDE_", "INSPECTOR_SHIM_NAME"),
        concat!("app.", "patched.asar"),
        concat!("Claude.", "patched.exe"),
        concat!("apply-claude-", "patch.ps1"),
        concat!("Shell", "ExecuteExW"),
        "takeown",
        "icacls",
    ] {
        assert!(
            !production_source.contains(removed_symbol),
            "old Windows install patch symbol should be removed: {removed_symbol}"
        );
    }
}

#[test]
fn direct_claude_desktop_launch_spawns_background_injector_on_windows() {
    let launch_body = production_between(
        "pub fn launch_with_app",
        "pub fn base_launch_command",
        "launch_with_app",
    );

    assert!(launch_body.contains("launch_windows_claude_desktop(localize, app)?"));
    assert!(
        !launch_body.contains("spawn_silent_localization_injector()"),
        "launch_with_app should delegate Windows background injection to the Windows launch helper"
    );

    let windows_launch_body = function_body_between(
        "launch_windows_claude_desktop",
        "launch_windows_claude_msix",
        "Windows launch",
    );
    assert!(
        windows_launch_body.contains("spawn_silent_localization_injector_with_app(app)"),
        "direct localized Windows launch should return after app activation and inject in the background"
    );
}

#[test]
fn direct_claude_desktop_launch_uses_async_helpers_on_both_platforms() {
    let source = production_source();
    let windows_launch_body = function_body_between(
        "launch_windows_claude_desktop",
        "launch_windows_claude_msix",
        "Windows launch",
    );
    let macos_localized_body = source_between(
        source,
        "fn launch_macos_claude_desktop_localized()",
        "#[cfg(any(target_os = \"windows\", target_os = \"macos\"))]",
        "macOS localized launch",
    );
    let macos_plain_body = source_between(
        source,
        "fn launch_macos_claude_desktop_plain_restart()",
        "#[cfg(any(target_os = \"macos\", test))]",
        "macOS plain launch",
    );

    assert_contains_all(
        windows_launch_body,
        &[
            "spawn_silent_localization_injector_with_app(app)",
            "return Ok(None);",
        ],
    );
    assert_contains_none(
        windows_launch_body,
        &[
            "retry_inject_localization()",
            "retry_inject_localization_until(",
        ],
    );

    assert_contains_all(
        macos_localized_body,
        &["spawn_silent_localization_injector()", "Ok(())"],
    );
    assert_contains_none(
        macos_localized_body,
        &[
            "enable_macos_claude_main_process_debugger()?",
            "retry_inject_localization()",
            "map(|_| ())",
        ],
    );

    assert_contains_all(macos_plain_body, &[".spawn()"]);
    assert_contains_none(macos_plain_body, &[".status()", "status.success()"]);
}

#[test]
fn silent_windows_injector_waits_for_manual_debugger_activation() {
    let silent_body = patch_between(
        "pub fn spawn_silent_localization_injector()",
        "fn emit_localization_progress",
        "spawn_silent_localization_injector",
    );
    let retry_body = patch_between(
        "fn run_background_localization_retry_loop",
        "fn retry_localization_after_background_debugger_request",
        "background localization retry loop",
    );
    assert_contains_all(
        silent_body,
        &[
            "thread::spawn(move || {",
            "run_background_localization_retry_loop(app)",
        ],
    );
    assert_contains_all(
        retry_body,
        &[
            "manualDebuggerActivationFallback",
            "request_claude_main_process_debugger_for_background_retry()",
            "retry_inject_localization_until(",
            "CLAUDE_ZH_BACKGROUND_INJECTION_WAIT_TIMEOUT",
        ],
    );
    assert_order(
        retry_body,
        "request_claude_main_process_debugger_for_background_retry()",
        "retry_inject_localization_until(",
        "extended localization retry loop should keep running after debugger request",
    );
    assert_order(
        silent_body,
        "thread::spawn(move || {",
        "run_background_localization_retry_loop(app)",
        "silent injector should spawn a helper thread before trying to open the debugger",
    );
}

#[test]
fn silent_injector_reports_background_progress_and_retries_debugger_enablement() {
    let silent_body = patch_between(
        "pub fn spawn_silent_localization_injector()",
        "fn emit_localization_progress",
        "spawn_silent_localization_injector",
    );

    assert_contains_all(
        silent_body,
        &[
            "spawn_silent_localization_injector_with_app",
            "run_background_localization_retry_loop",
        ],
    );
    assert_order(
        silent_body,
        "spawn_silent_localization_injector_with_app",
        "run_background_localization_retry_loop",
        "silent injector should delegate to the retry loop instead of swallowing one failure",
    );

    let retry_body = patch_between(
        "fn run_background_localization_retry_loop",
        "fn retry_localization_after_background_debugger_request",
        "background localization retry loop",
    );
    let request_body = patch_between(
        "fn request_claude_main_process_debugger_for_background_retry()",
        "fn request_claude_main_process_debugger_once()",
        "background debugger request helper",
    );
    assert_contains_all(
        retry_body,
        &[
            "CLAUDE_ZH_BACKGROUND_DEBUGGER_RETRY_LIMIT",
            "emit_localization_progress",
            "\"debugger\"",
            "\"injecting\"",
            "\"done\"",
            "\"failed\"",
        ],
    );
    assert_contains_all(
        retry_body,
        &[
            "for attempt in 1..=CLAUDE_ZH_BACKGROUND_DEBUGGER_RETRY_LIMIT",
            "request_claude_main_process_debugger_for_background_retry()",
            "retry_inject_localization_until",
            "CLAUDE_ZH_BACKGROUND_INJECTION_WAIT_TIMEOUT",
            "thread::sleep(Duration::from_millis(",
            "CLAUDE_ZH_BACKGROUND_DEBUGGER_RETRY_MS",
            "last_error",
            "emit_localization_progress",
        ],
    );
    assert_contains_all(
        request_body,
        &[
            "claude_node_inspector_available()",
            "request_claude_main_process_debugger_once()?",
            "wait_for_claude_node_inspector()",
        ],
    );
    assert_order(
        request_body,
        "claude_node_inspector_available()",
        "request_claude_main_process_debugger_once()",
        "background retry loop should try the already-open inspector before opening Claude menus",
    );
    assert_order(
        retry_body,
        "request_claude_main_process_debugger_for_background_retry()",
        "retry_inject_localization_until",
        "background retry loop should make one fast debugger request before injection",
    );
    assert_order(
        retry_body,
        "Err(err) =>",
        "thread::sleep(Duration::from_millis(",
        "failed attempts should delay and retry instead of being ignored",
    );
}

#[test]
fn background_localization_retry_loop_avoids_long_debugger_wait() {
    let retry_body = patch_between(
        "fn run_background_localization_retry_loop",
        "fn retry_localization_after_background_debugger_request",
        "background localization retry loop",
    );
    let request_body = patch_between(
        "fn request_claude_main_process_debugger_for_background_retry()",
        "fn request_claude_main_process_debugger_once()",
        "background debugger request helper",
    );

    assert_contains_all(
        retry_body,
        &["request_claude_main_process_debugger_for_background_retry()"],
    );
    assert_contains_all(
        request_body,
        &[
            "claude_node_inspector_available()",
            "request_claude_main_process_debugger_once()?",
            "wait_for_claude_node_inspector()",
        ],
    );
    assert_contains_none(
        retry_body,
        &[
            "enable_claude_main_process_debugger()",
            "ensure_windows_claude_main_process_debugger",
            "request_windows_claude_main_process_debugger_until_available",
            "WINDOWS_MAIN_PROCESS_DEBUGGER_WAIT_TIMEOUT",
            "WINDOWS_MAIN_PROCESS_DEBUGGER_RETRY_MS",
            "WINDOWS_MAIN_PROCESS_DEBUGGER_POLL_MS",
            "MACOS_MAIN_PROCESS_DEBUGGER_WAIT_TIMEOUT",
        ],
    );
}

#[test]
fn terminal_windows_injector_keeps_waiting_after_debugger_automation_failure() {
    let spawn_body = patch_between(
        "pub fn spawn_localization_injector",
        "pub fn spawn_silent_localization_injector",
        "spawn_localization_injector",
    );

    assert_contains_all(
        spawn_body,
        &[
            "manualDebuggerActivationFallback",
            "retry_localization_after_background_debugger_request()",
        ],
    );
    assert!(!spawn_body.contains("return;"));
    assert_order(
        spawn_body,
        "manualDebuggerActivationFallback",
        "retry_localization_after_background_debugger_request()",
        "terminal injector should mark manual fallback before retrying injection",
    );

    let retry_body = patch_between(
        "fn retry_localization_after_background_debugger_request()",
        "fn request_claude_main_process_debugger_for_background_retry",
        "terminal background retry helper",
    );
    assert_contains_all(
        retry_body,
        &[
            "run_background_localization_retry_loop_for_terminal()",
            "request_claude_main_process_debugger_for_background_retry()",
            "for attempt in 1..=CLAUDE_ZH_BACKGROUND_DEBUGGER_RETRY_LIMIT",
        ],
    );
}

#[test]
fn macos_localization_uses_official_main_process_debugger_menu() {
    let source = patch_source();
    assert_contains_all(
        source,
        &[
            "launch_macos_claude_desktop_localized",
            "enable_macos_claude_main_process_debugger",
            "request_macos_claude_main_process_debugger_once",
            "MACOS_MAIN_PROCESS_DEBUGGER_WAIT_TIMEOUT",
            "request_macos_claude_main_process_debugger_native",
            "AXIsProcessTrusted",
            "AXUIElementCreateApplication",
            "macos-main-debugger.log",
            "Enable Main Process Debugger",
            "Grant CodeStudio Lite Accessibility permission",
            "ensure_localized_launch_prerequisites",
            "ensure_macos_accessibility_trusted_for_localized_launch",
            "Current app bundle",
            "Current executable",
            "env::current_exe()",
            "Accessibility preflight check: AXIsProcessTrusted",
            "AXIsProcessTrustedWithOptions(prompt=true) returned",
            "CLAUDE_MACOS_ACCESSIBILITY_PENDING_LAUNCH_MARKER",
            "take_pending_claude_desktop_launch_after_restart",
            "restart_claude_desktop_after_accessibility_grant",
            "write_macos_accessibility_pending_launch_marker",
            "take_macos_accessibility_pending_launch_marker",
            "app.request_restart()",
            "macos_accessibility_is_trusted_raw()",
            "macos_accessibility_restart_required_error",
            "request_macos_accessibility_prompt",
            "launch-claude-macos-zh.sh",
            "macos_localized_launch_script",
            "write_localized_launch_marker()?",
            "claude_node_inspector_available()",
            "wait_for_claude_node_inspector()",
            "启用主进程调试器",
            "click_macos_claude_main_process_debugger_menu",
            "ax_find_and_press_debugger_menu_item",
            "CFRetain",
            "MACOS_AX_MAX_CHILDREN_PER_NODE",
        ],
    );
    assert_contains_none(
        source,
        &[
            concat!("apply_", "macos_localization_patch"),
            concat!("resolve_", "macos_claude_install_for_patch"),
            concat!("Macos", "ClaudePatchPaths"),
            concat!("Macos", "PatchPayloads"),
            concat!("ElectronAsarIntegrity", ":Resources/app.asar:hash"),
            concat!("update_", "macos_asar_integrity_hash"),
            concat!("ad_hoc_", "codesign_macos_app"),
            concat!("with administrator", " privileges"),
            concat!("apply-claude-", "macos-patch.log"),
            concat!("Privacy_", "Accessibility"),
            concat!("open_macos_", "accessibility_settings"),
            concat!("run_", "macos_main_process_debugger_", "apple", "script"),
            concat!("macos_enable_main_process_debugger_", "apple", "script"),
        ],
    );

    let ensure_body = patch_between(
        "pub fn ensure_localization_patch()",
        "pub fn spawn_localization_injector",
        "ensure_localization_patch",
    );
    assert_contains_all(
        ensure_body,
        &[
            "ensure_patch_files()?",
            "ensure_claude_desktop_developer_mode()",
        ],
    );
    assert_contains_none(
        ensure_body,
        &[concat!("apply_", "macos_localization_patch()")],
    );

    let macos_launch_body = patch_between(
        "fn launch_macos_claude_desktop_localized(",
        "fn ensure_claude_desktop_developer_mode",
        "macOS launch",
    );
    assert_contains_all(
        macos_launch_body,
        &[
            "ensure_patch_files()?",
            "ensure_claude_desktop_developer_mode()?",
            "write_localized_launch_marker()?",
            "close_existing_claude_for_localized_launch()?",
            "hidden_command(\"open\")",
            "spawn_silent_localization_injector()",
        ],
    );
    assert_contains_none(
        macos_launch_body,
        &[
            "allow_accessibility_restart",
            "ensure_macos_accessibility_trusted_or_restart_needed()?",
            "schedule_macos_accessibility_restart",
            "localization injection also failed",
            "enable_macos_claude_main_process_debugger()?",
            "retry_inject_localization()",
            concat!("apply_", "macos_localization_patch()?"),
        ],
    );
    assert_order(
        macos_launch_body,
        "ensure_macos_accessibility_trusted",
        "write_localized_launch_marker()?",
        "Accessibility preflight should run before writing the localized launch marker",
    );
    for after_preflight in [
        "close_existing_claude_for_localized_launch()?",
        "hidden_command(\"open\")",
    ] {
        assert_order(
            macos_launch_body,
            "ensure_macos_accessibility_trusted_for_localized_launch()?",
            after_preflight,
            "Accessibility preflight should run before touching Claude",
        );
    }

    let silent_body = patch_between(
        "fn retry_localization_after_background_debugger_request()",
        "fn ensure_patch_files",
        "terminal/background retry bridge",
    );
    assert_contains_all(
        silent_body,
        &[
            "enable_macos_claude_main_process_debugger()",
            "retry_inject_localization()",
        ],
    );

    let script = macos_localized_launch_script();
    assert_contains_all(
        &script,
        &[
            "/usr/bin/pgrep -x Claude",
            "/usr/bin/pkill -TERM -x Claude",
            "/usr/bin/pkill -KILL -x Claude",
            "/usr/bin/open -a Claude",
            "claude_debugger_open()",
            "lsof -nP -iTCP",
            "/usr/bin/curl -fsS --max-time 1",
            "port=9229",
            "\"webSocketDebuggerUrl\"",
            "Claude.app/Contents/MacOS/Claude",
            "while ! claude_debugger_open; do",
            "deadline=$(( $(/bin/date +%s) + 90 ))",
            "debugger_attempts=0",
            "debugger_attempts=$((debugger_attempts + 1))",
            "Waiting for CodeStudio Lite to enable Claude main process debugger via Accessibility",
            "Timed out waiting for Claude main process debugger",
            "localized-launch.flag",
        ],
    );
    assert_contains_none(
        &script,
        &[
            "developer_settings.json",
            "allowDevTools",
            "osascript",
            "tell application",
            "/usr/bin/plutil",
            "/usr/bin/seq 9229 9300",
            "APPLESCRIPT",
            "JXA",
            "clickDebuggerConfirmation",
            "clickedDebuggerMenu",
        ],
    );
}

#[test]
fn macos_plain_claude_launch_restarts_instead_of_activating_existing_process() {
    let launch_body = patch_between(
        "pub fn launch_with_app(",
        "pub fn take_pending_claude_desktop_launch_after_restart",
        "Claude Desktop launch",
    );
    assert_contains_all(
        launch_body,
        &[
            "launch_macos_claude_desktop_plain_restart()?",
            "localize",
            "launch_macos_claude_desktop_localized()?",
        ],
    );

    let script = macos_plain_launch_script();
    assert_contains_all(
        &script,
        &[
            "/usr/bin/pgrep -x Claude",
            "/usr/bin/pkill -TERM -x Claude",
            "/usr/bin/pkill -KILL -x Claude",
            "/usr/bin/open -a Claude",
        ],
    );
}

#[test]
fn windows_launch_script_launches_cleanly_without_debug_args() {
    let script = windows_launch_script(true);
    // Claude 已 fuse 掉 inspect 启动参数，本地化 MSIX 启动只正常激活 App；
    // 主进程调试器由菜单自动化开启。
    assert!(script.contains("Get-AppxPackage"));
    assert!(script.contains("shell:AppsFolder"));
    assert!(script.contains("Start-Process -FilePath $target"));
    assert!(!script.contains("--inspect"));
    assert!(!script.contains("--remote-debugging-port"));
    assert!(!script.contains("Invoke-CommandInDesktopPackage"));
    assert!(!script.contains("Wait-ClaudeLaunchProcess"));
    assert!(!script.contains("Start-Process -FilePath $exe -WorkingDirectory"));
    assert!(script.contains("app identity activation is required"));
    assert!(script.contains("localized-launch.flag"));
    assert!(script.contains("Set-Content"));
    assert!(script.contains("zh-CN"));
    assert!(!windows_launch_script(false).contains("localized-launch.flag"));
}

#[test]
fn localized_launch_uses_official_debugger_runtime_injection_without_debug_args() {
    assert!(claude_launch_args(true).is_empty());
    assert!(claude_launch_args(false).is_empty());
    let production_source = production_source();
    assert_contains_all(
        production_source,
        &[
            "request_windows_claude_main_process_debugger_once",
            "retry_inject_localization",
        ],
    );
    assert_contains_none(
        production_source,
        &[
            concat!("apply_", "localization_patch"),
            concat!("build_", "inspector_shim"),
        ],
    );
}

#[test]
fn localized_windows_msix_launch_uses_app_identity_and_menu_debugger() {
    let windows_launch_body = source_between(
        production_source(),
        "fn launch_windows_claude_desktop",
        "fn launch_windows_claude_msix",
        "Windows launch",
    );

    assert_contains_all(
        windows_launch_body,
        &[
            "launch_windows_claude_msix(&args)",
            "spawn_silent_localization_injector_with_app(app)",
        ],
    );
    assert_contains_none(
        windows_launch_body,
        &[
            "launch_windows_claude_localized_msix_desktop_package(&args)",
            "package::launch_first_desktop_package_with_args",
            "process_control::is_process_running(\"claude\")",
        ],
    );
}

#[test]
fn windows_debugger_automation_uses_in_window_menu_not_alt_top_menu() {
    let request_body = windows_debugger_request_body();
    assert_contains_all(
        request_body,
        &[
            "UIAutomationClient",
            "SetProcessDPIAware",
            "SetWindowPos",
            "Move-ClaudeMenuPopupsOffscreen",
            "shell:AppsFolder",
            "$bang = [char]33",
            "$packagePrefix = $pkg.PackageFamilyName + $bang",
            "Developer",
            "Enable Main Process Debugger",
            "TogglePattern",
            "ValuePattern",
            "Find-ClaudeMenuButton",
            "Invoke-Element",
            "Find-ClaudeDeveloperMenuByStructure",
            "Find-ClaudeDebuggerToggleByStructure",
            "Find-ClaudeMenuItems",
            "AutomationElement]::FromHandle($window.Hwnd)",
            "Close-ClaudeInspectorPromptWindows",
            "Test-ClaudeInspectorPromptCandidate",
            "IsInspectorPrompt",
            "Where-Object { -not $_.IsInspectorPrompt }",
        ],
    );
    assert!(!request_body.contains("$($pkg.PackageFamilyName)!"));
    assert_order(
        request_body,
        "Wait-CloseClaudeInspectorPromptWindows $window 2 | Out-Null",
        "if (-not (Open-ClaudeMenu $window $developerNames))",
        "inspector prompt should be closed before menu automation",
    );
    assert_contains_none(
        request_body,
        &[
            "Move-ClaudeWindowOffscreen",
            "Restore-ClaudeWindowPlacement",
            "$originalPlacement = Move-ClaudeWindowOffscreen $window",
        ],
    );
    assert_order(
        request_body,
        "if (-not (Open-ClaudeMenu $window $developerNames))",
        "Move-ClaudeMenuPopupsOffscreen $window",
        "menu popups should be moved offscreen immediately after opening Claude's menu",
    );
    assert_order(
        request_body,
        "Move-ClaudeMenuPopupsOffscreen $window",
        "$developer = Find-ClaudeDeveloperMenuElement $developerNames $window",
        "developer lookup should run after menu popup relocation",
    );
    assert_contains_all(
        request_body,
        &[
            "WindowPattern",
            "$windowPattern.Close()",
            "PostMessage",
            "WM_CLOSE",
            "windows-main-debugger.log",
            "Write-ClaudeDebuggerLog",
            "Format-ClaudeElementForLog",
            "$menuButton = Find-ClaudeMenuButton $window",
            "run_windows_debugger_powershell_with_timeout",
        ],
    );
    assert_contains_in_order(
        request_body,
        &[
            "function Open-ClaudeMenu",
            "Test-ClaudeMenuPopupOpen $window $developerNames",
            "$menuButton = Find-ClaudeMenuButton $window",
        ],
        "Open-ClaudeMenu should accept visible popup menus before button fallback",
    );
    assert_order(
        request_body,
        "if (-not (Open-ClaudeMenu $window $developerNames))",
        "$developer = Find-ClaudeDeveloperMenuByStructure $window",
        "developer lookup should run after opening the in-window menu",
    );
    assert_order(
        request_body,
        "Find-ClaudeDeveloperMenuElement $developerNames",
        "$developer = Find-ClaudeDeveloperMenuByStructure $window",
        "structural developer fallback should run after label lookup",
    );
    assert_order(
        request_body,
        "Find-ClaudeDebuggerToggleElement $debuggerNames",
        "$debuggerItem = Find-ClaudeDebuggerToggleByStructure $window",
        "structural debugger fallback should run after label lookup",
    );
    assert_contains_in_order(
        request_body,
        &[
            "$togglePattern.Toggle()",
            "Start-ClaudeInspectorPromptCleanupJob $window 4500",
            "for ($attempt = 0; $attempt -lt 3; $attempt++)",
        ],
        "inspector prompt cleanup should start without blocking after debugger opens",
    );
    assert_contains_none(
        request_body,
        &[
            concat!("Set", "Cursor", "Pos"),
            concat!("mouse", "_event"),
            concat!("Click", "-Point"),
            concat!("Click", "-Element", "Center"),
            concat!("System.Windows", ".Forms"),
            concat!("$window", ".Left"),
            concat!("$window", ".Top"),
            concat!("$window", ".Right"),
            concat!("$window", ".Bottom"),
            concat!("Send", "Keys"),
            "'%d'",
            "{DOWN}{ENTER}",
            "crate::core::platform::run_powershell(script)",
        ],
    );
    assert_contains_all(
        patch_source(),
        &[
            "WINDOWS_MAIN_PROCESS_DEBUGGER_SCRIPT_TIMEOUT",
            "child.kill()",
            "run_windows_debugger_powershell_with_timeout",
            "\"-WindowStyle\"",
            "\"Hidden\"",
        ],
    );
}

#[test]
fn windows_debugger_automation_searches_same_claude_process_popup_menus() {
    let request_body = windows_debugger_request_body();

    assert_contains_all(
        request_body,
        &[
            "function Get-ClaudeAutomationRoots($window)",
            "if ([int]$processId -ne [int]$window.ProcessId)",
            "$className -notlike 'Chrome_WidgetWin_*'",
            "AutomationElement]::FromHandle($hWnd)",
        ],
    );
    assert_order(
        request_body,
        "function Get-ClaudeAutomationRoots($window)",
        "function Find-ClaudeMenuElement",
        "menu lookup should use same-process root helper",
    );
    assert_order(
        request_body,
        "foreach ($rootInfo in (Get-ClaudeAutomationRoots $window))",
        "$matches = $root.FindAll",
        "menu lookup should search each collected root",
    );
}

#[test]
fn windows_debugger_automation_does_not_treat_main_window_submenu_as_open_menu() {
    let request_body = windows_debugger_request_body();

    assert_contains_all(
        request_body,
        &[
            "function Find-ClaudeDeveloperMenuElement",
            "function Find-ClaudeDebuggerToggleElement",
            "function Test-ClaudeMenuPopupOpen",
            "if ($rootInfo.IsMainWindow) { continue }",
            "$controlType -eq 'ControlType.MenuItem'",
            "$className -eq 'MenuItemView'",
            "$patterns -contains 'ExpandCollapsePatternIdentifiers.Pattern'",
            "$controlType -eq 'ControlType.CheckBox'",
            "$patterns -contains 'TogglePatternIdentifiers.Pattern'",
        ],
    );
    assert!(!request_body.contains("Find-ClaudeMenuElement $developerNames $window $false"));
}

#[test]
fn windows_debugger_automation_closes_blocking_web_modals_before_menu() {
    let request_body = windows_debugger_request_body();

    assert_contains_all(
        request_body,
        &[
            "function Close-ClaudeBlockingWebModals($window)",
            "function Find-ClaudeBlockingWebModal($root)",
            "function Find-ClaudeModalCloseButton($modal)",
            "function Find-ClaudeBlockingWebCloseButton($root, $window)",
            "function Test-ClaudeBlockingWebCloseButton($button, $rootRect)",
            "function Test-ClaudeElementStillVisible($element)",
            "function Find-ClaudeCloseButton($root)",
            "ControlType.Window",
            "$controlType -eq 'ControlType.Window'",
            "if ($frameworkId -ne 'Chrome') { continue }",
            "if ($className -eq 'WinCaptionButton') { continue }",
            "InvokePatternIdentifiers.Pattern",
            "ControlType.Button",
            "$menuButton = Find-ClaudeMenuButton $window",
            "$button = Wait-ClaudeCondition 40 50 { Find-ClaudeBlockingWebCloseButton $root $window }",
            "ProgrammaticName -eq 'LegacyIAccessiblePatternIdentifiers.Pattern'",
            "if (Find-ClaudeMenuButton $window) { break }",
        ],
    );
    assert_order(
        request_body,
        "Find-ClaudeCloseButton $root",
        "if (-not (Open-ClaudeMenu $window $developerNames))",
        "blocking web modals should be closed before menu automation",
    );
    assert_contains_none(
        request_body,
        &[
            "Test-ClaudeOverlayCandidateText",
            "Test-ClaudeRootHasBlockingOverlayText",
            "Test-ClaudeOverlayCloseButtonName",
            "Find-ClaudeAnonymousOverlayCloseButton",
            "Close-ClaudeBlockingOverlayWindows",
            "Upgrade|Plan|Pro|Team|Try|Trial|Subscribe|Discount|Offer|New|Announcement|Promo",
            "message limit|free messages|keep chatting|out of free|usage limit|rate limit|limit reset",
            "升级|订阅|套餐|试用|优惠|公告|新功能|推广|广告",
            concat!("Set", "Cursor", "Pos"),
            concat!("Click", "-Element", "Center"),
            concat!("WM", "_KEY"),
            concat!("keybd", "_event"),
            "[System.Windows.Automation.LegacyIAccessiblePattern]::Pattern",
            "SendInput",
            "SendKeys",
        ],
    );
}

#[test]
fn windows_debugger_automation_prefers_existing_window_before_appx_activation() {
    let request_body = windows_debugger_request_body();

    assert_contains_in_order(
        request_body,
        &[
            "$window = Get-ClaudeMainWindow",
            "if ($window) {",
            "} else {",
            "Start-ClaudeWindowsApp",
            "Wait-ClaudeCondition 8 40",
            "if (-not $window) {",
            "Wait-ClaudeCondition 50 100",
        ],
        "existing Claude window should be preferred before fallback AppX activation",
    );
    assert!(!request_body.contains("if (-not $window) { Start-ClaudeWindowsApp }"));
    assert!(request_body
        .contains("Write-ClaudeDebuggerLog 'Using existing Claude window before app activation.'"));
    assert_contains_all(
        request_body,
        &[
            "function Get-ClaudeProcessMap",
            "function Build-ClaudeWindowCandidate",
            "function Get-ClaudeMainWindowFromProcessHandles",
            "Get-Process -Name 'claude'",
            "$candidate = Get-ClaudeMainWindowFromProcessHandles $claudeProcesses",
            "$claudeProcesses = Get-ClaudeProcessMap",
            "$proc = $claudeProcesses[[int]$processId]",
        ],
    );
    assert_order(
        request_body,
        "Get-ClaudeMainWindowFromProcessHandles $claudeProcesses",
        "[CslClaudeWin32]::EnumWindows",
        "main window handle fast path should run before full top-level window enumeration",
    );
    assert_contains_none(request_body, &["Get-Process -Id ([int]$processId)"]);
}

#[test]
fn windows_debugger_automation_polls_to_close_inspector_prompt() {
    let request_body = windows_debugger_request_body();

    assert_contains_all(
        request_body,
        &[
            "function Wait-CloseClaudeInspectorPromptWindows($window",
            "Close-ClaudeInspectorPromptWindows $window",
            "Start-Sleep -Milliseconds 40",
        ],
    );
    assert_order(
        request_body,
        "$togglePattern.Toggle()",
        "Start-ClaudeInspectorPromptCleanupJob $window 4500",
        "inspector prompt cleanup should start after toggling debugger",
    );
    assert_contains_in_order(
        request_body,
        &[
            "for ($attempt = 0; $attempt -lt 3; $attempt++)",
            "Wait-CloseClaudeInspectorPromptWindows $window 1 | Out-Null",
        ],
        "inspector prompt should also be polled after confirmations",
    );
}

#[test]
fn windows_debugger_request_waits_for_inspector_while_automation_runs() {
    let background_retry_body = production_between(
        "fn request_claude_main_process_debugger_for_background_retry()",
        "fn request_windows_claude_main_process_debugger_once()",
        "request_claude_main_process_debugger_for_background_retry",
    );

    assert_contains_all(
        background_retry_body,
        &[
            "claude_node_inspector_available()",
            "request_claude_main_process_debugger_once()?",
            "wait_for_claude_node_inspector()",
        ],
    );
    assert_order(
        background_retry_body,
        "request_claude_main_process_debugger_once()?",
        "wait_for_claude_node_inspector()",
        "Windows debugger requests should wait for inspector availability after the one-shot menu request",
    );

    let production_source = production_source();
    assert_contains_none(
        production_source,
        &[
            "fn enable_claude_main_process_debugger()",
            "fn ensure_windows_claude_main_process_debugger()",
            "fn request_windows_claude_main_process_debugger_until_available()",
            "WINDOWS_MAIN_PROCESS_DEBUGGER_WAIT_TIMEOUT",
            "WINDOWS_MAIN_PROCESS_DEBUGGER_RETRY_MS",
            "WINDOWS_MAIN_PROCESS_DEBUGGER_POLL_MS",
            "mpsc::channel",
            "request_thread",
        ],
    );
}

#[test]
fn windows_debugger_automation_uses_short_condition_polling() {
    let request_body = windows_debugger_request_body();

    assert_contains_all(
        request_body,
        &[
            "function Wait-ClaudeCondition",
            "Wait-ClaudeCondition 8 40",
            "Wait-ClaudeCondition 16 40",
            "Start-Sleep -Milliseconds 40",
        ],
    );
    assert_contains_none(
        request_body,
        &[
            "Wait-ClaudeCondition 30 40",
            "Wait-ClaudeCondition 30 50",
            "for ($phase = 0; $phase -lt 8; $phase++)",
            "Start-Sleep -Milliseconds 120",
            "for ($attempt = 0; $attempt -lt 20; $attempt++)",
            "Start-Sleep -Milliseconds 500",
        ],
    );
}

#[test]
fn windows_debugger_script_timeout_allows_menu_automation_to_complete() {
    assert_contains_all(
        patch_source(),
        &["WINDOWS_MAIN_PROCESS_DEBUGGER_SCRIPT_TIMEOUT: Duration = Duration::from_secs(30)"],
    );
}

#[test]
fn windows_debugger_prompt_cleanup_runs_after_toggle_without_blocking_completion() {
    let request_body = windows_debugger_request_body();

    assert_contains_all(
        request_body,
        &[
            "Start-ClaudeInspectorPromptCleanupJob $window 2000",
            "Start-ClaudeInspectorPromptCleanupJob $window 4500",
            "Windows Main Process Debugger automation completed.",
        ],
    );
    assert_order(
        request_body,
        "$togglePattern.Toggle()",
        "Start-ClaudeInspectorPromptCleanupJob $window 4500",
        "inspector cleanup job should start immediately after toggling the debugger",
    );
    assert_order(
        request_body,
        "Start-ClaudeInspectorPromptCleanupJob $window 4500",
        "Windows Main Process Debugger automation completed.",
        "post-toggle inspector cleanup should not block automation completion",
    );
    assert!(!request_body.contains("Wait-CloseClaudeInspectorPromptWindows $window 12 | Out-Null"));
}

#[test]
fn windows_debugger_automation_closes_native_inspector_dialog_windows() {
    let request_body = windows_debugger_request_body();

    assert_contains_all(
        request_body,
        &[
            "function Test-ClaudeInspectorWindowClass([string]$className)",
            "'#32770'",
        ],
    );
    let close_body = source_between(
        request_body,
        "function Close-ClaudeInspectorPromptWindows($window)",
        "function Wait-CloseClaudeInspectorPromptWindows",
        "Close-ClaudeInspectorPromptWindows",
    );
    assert_contains_all(
        close_body,
        &["Test-ClaudeInspectorWindowClass $className", "$closed += 1"],
    );
    assert_contains_none(
        close_body,
        &[
            "if ($className -ne 'Chrome_WidgetWin_1') { return $true }",
            "$script:closed += 1",
        ],
    );
}

#[test]
fn node_inspector_uses_claude_default_port_only() {
    assert_eq!(CLAUDE_NODE_INSPECT_PORT, 9229);
    assert_contains_none(
        patch_source(),
        &[
            concat!("CLAUDE_NODE_INSPECT_PORT", "_SCAN_END"),
            concat!("..=", "CLAUDE_NODE_INSPECT_PORT"),
        ],
    );
}

#[test]
fn node_inspector_injection_source_targets_electron_windows() {
    let source = main_process_injection_source();
    assert_contains_all(
        &source,
        &[
            "BrowserWindow.getAllWindows",
            "process.getBuiltinModule(\"module\").createRequire",
            "contents.debugger.attach",
            "__cslZhAttachedVersion",
            "debuggerWasAttached",
            "contents.debugger.detach()",
            "Fetch.enable",
            "Page.addScriptToEvaluateOnNewDocument",
            "Page.reload",
            "withTimeout",
            "__CODESTUDIO_CLAUDE_ZH_MAIN__",
            "CSL_INJECTION_VERSION",
            "translation-runtime.js",
            "localePayloadForUrl",
            "ion-dist/i18n/en-US.json",
            "currentLocale === \"zh-CN\" && isEn && localLike",
            "webContents.getAllWebContents",
            "localWindowHotSwitchSync",
            "lower.startsWith(\"devtools://\")",
            "applyLocalWindowTitle",
            "setup-desktop-3p",
            "Configure Third-Party Inference",
            "aboutClaudeWindowFallback",
            "About Claude",
            "about_window",
        ],
    );
    // The runtime is delivered via addScriptToEvaluateOnNewDocument so it
    // survives the reload; executeJavaScript is intentionally NOT awaited
    // before reload (that would leave its promise pending on unload).
    assert_contains_none(
        &source,
        &["await contents.executeJavaScript(runtime, true)"],
    );
}

#[test]
fn node_inspector_injection_syncs_locale_after_language_menu_changes() {
    let source = main_process_injection_source();

    assert_contains_all(
        &source,
        &[
            "CSL_INJECTION_VERSION = 9",
            "let currentLocale",
            "setCurrentLocale",
            "zhActive",
            "pollLocale",
            "syncOpenWindowsLocale",
            "syncOneWindowLocale",
            "CSL_WANTED_LOCALE_KEY",
            "localStorage.getItem(\"__cslWantedLocale\")||localStorage.getItem(\"spa:locale\")",
            "localStorage.getItem(\"spa:locale\")",
            "localStorage.setItem(\"__cslWantedLocale\"",
            "localStorage.setItem(\"spa:locale\"",
            "claude-locale-change",
            "localeChangeListeners.push(syncOpenWindowsLocale)",
            "syncOpenWindowsLocale(currentLocale)",
            "fireLocaleChange(currentLocale)",
            "fallback",
            "setCurrentLocale(fallback)",
        ],
    );
}

#[test]
fn node_inspector_injection_localizes_macos_menu_bar() {
    let source = main_process_injection_source();

    assert_contains_all(
        &source,
        &[
            "macosMenuBarLocalization",
            "process.platform !== \"darwin\"",
            "Menu.setApplicationMenu",
            "Menu.getApplicationMenu",
            "__cslMenuBarLocalizationInstalled",
            "__cslLastApplicationMenu",
            "localeChangeListeners.push(retranslateMenuBar)",
            "en-US.json",
            "shellLocale",
            "labelToId",
            "rememberCatalog",
            "process.resourcesPath",
            "__cslMessageId",
            "labelMessageId",
            "menuHardcodedZh",
            "menuRoleZh",
            "roleKey(item)",
            "Hide Claude",
            "Enable Main Process Debugger",
            "\\u542f\\u7528\\u4e3b\\u8fdb\\u7a0b\\u8c03\\u8bd5\\u5668",
        ],
    );
}

#[test]
fn macos_menu_localization_translates_dynamic_update_prefix() {
    let source = main_process_injection_source();
    let macos_menu_block = source_between(
        &source,
        "const installMacosMenuLocalization = () => {",
        "installMacosMenuLocalization();",
        "macOS menu localization block",
    );

    assert_contains_all(
        macos_menu_block,
        &[
            "translateDynamicMenuLabel",
            "Restart to update to ",
            "\\u91cd\\u65b0\\u542f\\u52a8\\u4ee5\\u66f4\\u65b0\\u5230 ",
        ],
    );
    assert_order(
        macos_menu_block,
        "const dynamic = translateDynamicMenuLabel(label);",
        "if (id && enToZh[label]) return enToZh[label];",
        "macOS update menu labels should handle versioned update text before exact catalog lookup",
    );
}

#[test]
fn node_inspector_injection_localizes_windows_in_window_menu() {
    let source = main_process_injection_source();

    assert_contains_all(
        &source,
        &[
            "windowsMenuPopupLocalization",
            "process.platform === \"win32\"",
            "Menu.buildFromTemplate",
            "Menu.setApplicationMenu",
            "Menu.prototype.popup",
            "__cslMenuPopupLocalizationInstalled",
            "localizeMenuForCurrentLocale",
            "relabelMenuItems(menu, currentLocale",
            "origBuildFromTemplate(template)",
            "origSetApplicationMenu(menu)",
            "origPopup.call(this",
            "\"File\": \"\\u6587\\u4ef6\"",
            "\"Edit\": \"\\u7f16\\u8f91\"",
            "\"View\": \"\\u89c6\\u56fe\"",
            "\"Developer\": \"\\u5f00\\u53d1\\u8005\"",
            "\"Help\": \"\\u5e2e\\u52a9\"",
            "\"Show Dev Tools\"",
            "\"Open App Config File...\"",
        ],
    );
}

#[test]
fn windows_menu_localization_uses_catalog_and_dynamic_update_prefixes() {
    let source = main_process_injection_source();
    let windows_menu_block = source_between(
        &source,
        "const installWindowsMenuPopupLocalization = () => {",
        "installWindowsMenuPopupLocalization();",
        "Windows menu localization block",
    );

    assert_contains_all(
        windows_menu_block,
        &[
            "JSON.parse(shellLocale)",
            "en-US.json",
            "rememberCatalog",
            "labelToId",
            "zhLocaleObj",
            "labelMessageId",
            "item.__cslMessageId",
            "Restart to update to ",
            "\\u91cd\\u65b0\\u542f\\u52a8\\u4ee5\\u66f4\\u65b0\\u5230 ",
        ],
    );
    assert_order(
        windows_menu_block,
        "if (id && zhLocaleObj[id]) return zhLocaleObj[id];",
        "if (menuHardcodedZh[key]) return menuHardcodedZh[key];",
        "Windows menu should prefer Claude locale catalog translations before hardcoded fallbacks",
    );
}

#[test]
fn node_inspector_injection_syncs_windows_devtools_title_after_language_changes() {
    let source = main_process_injection_source();

    assert_contains_all(
        &source,
        &[
            "devToolsWindowTitleSync",
            "lower.startsWith(\"devtools://\")",
            "lower.startsWith(\"chrome-devtools://\")",
            "syncDevToolsTitleLater",
            "\"page-title-updated\"",
            "\"devtools-opened\"",
            "\"did-finish-load\"",
            "localeChangeListeners.push(() =>",
            "syncOpenWindowsLocale(currentLocale)",
            "\\u5f00\\u53d1\\u8005\\u5de5\\u5177",
        ],
    );
}

#[test]
fn node_inspector_injection_localizes_windows_tray_menu() {
    let source = main_process_injection_source();

    assert_contains_all(
        &source,
        &[
            "windowsTrayMenuLocalization",
            "electron.Tray",
            "Tray.prototype.setContextMenu",
            "__cslTrayMenuLocalizationInstalled",
            "knownTrayMenus",
            "localizeTrayMenuForCurrentLocale",
            "localeChangeListeners.push(retranslateTrayMenus)",
            "Show Claude",
            "Show App",
            "Quit Claude",
            "\\u663e\\u793a Claude",
            "\\u663e\\u793a\\u5e94\\u7528\\u754c\\u9762",
            "\\u9000\\u51fa Claude",
        ],
    );
}

#[test]
fn macos_menu_bar_can_return_to_chinese_from_other_locales() {
    let source = main_process_injection_source();

    assert_contains_all(
        &source,
        &[
            "rememberCatalog(enObj)",
            "rememberCatalog(zhObj)",
            "fs.readdirSync(process.resourcesPath)",
            "loadLocaleCatalog(target)",
            "item.__cslMessageId = labelMessageId(orig) || labelMessageId(item.label)",
            "translateLabel(orig, item.__cslMessageId, roleKey(item))",
            "const id = item.__cslMessageId || labelMessageId(orig)",
            "id && idToVal[id] ? idToVal[id]",
            "about: \"\\u5173\\u4e8eClaude\"",
            "quit: \"\\u9000\\u51fa Claude\"",
        ],
    );
}

#[test]
fn macos_debugger_menu_is_not_clicked_when_inspector_is_already_open() {
    let source = patch_source();
    let enable_body = patch_between(
        "fn enable_macos_claude_main_process_debugger()",
        "fn request_macos_claude_main_process_debugger_once",
        "enable_macos_claude_main_process_debugger",
    );
    assert!(
        enable_body
            .find("claude_node_inspector_available()")
            .expect("should check for an existing inspector first")
            < enable_body
                .find("request_macos_claude_main_process_debugger_once()")
                .expect("should request the debugger only after the guard")
    );
    assert!(enable_body.contains("wait_for_claude_node_inspector()"));
    assert!(enable_body.contains("request_count += 1"));
    assert!(enable_body.contains("request_macos_claude_main_process_debugger_once()"));
    assert!(enable_body.contains("macos_debugger_log_path()"));
    assert!(enable_body.contains("After granting Accessibility access"));
    assert!(
        enable_body
            .find("ACCESSIBILITY_NOT_TRUSTED")
            .expect("Accessibility denial should be handled")
            < enable_body
                .find("last_error = err")
                .expect("non-permission errors should keep retrying")
    );
    assert!(
        enable_body
            .find("request_count += 1")
            .expect("should count debugger requests")
            < enable_body
                .find("request_macos_claude_main_process_debugger_once()")
                .expect("should request the debugger inside the retry loop")
    );
    let request_body = patch_between(
        "fn request_macos_claude_main_process_debugger_once()",
        "fn macos_debugger_log_path",
        "request_macos_claude_main_process_debugger_once",
    );
    assert_contains_all(
        request_body,
        &[
            "request_macos_claude_main_process_debugger_native",
            "append_macos_debugger_log",
        ],
    );
    assert_contains_none(
        request_body,
        &[
            ".output()",
            "osascript",
            concat!("run_", "macos_main_process_debugger_", "apple", "script"),
        ],
    );
    let preflight_body = patch_between(
        "fn ensure_macos_accessibility_trusted_for_localized_launch()",
        "fn enable_macos_claude_main_process_debugger",
        "Accessibility preflight",
    );
    assert_contains_all(
        preflight_body,
        &[
            "macos_accessibility_is_trusted_raw()",
            "AXIsProcessTrusted=true",
            "AXIsProcessTrusted=false",
            "restart required before launching Claude",
            "macos_accessibility_restart_required_error()",
        ],
    );
    assert!(
        preflight_body
            .find("macos_accessibility_is_trusted_raw()")
            .expect("preflight should check the current Accessibility state")
            < preflight_body
                .find("request_macos_accessibility_prompt")
                .expect("preflight should request permission only after checking state")
    );
    assert!(
        preflight_body
            .find("Accessibility preflight check: AXIsProcessTrusted=true")
            .expect("trusted path should be logged")
            < preflight_body
                .find("request_macos_accessibility_prompt")
                .expect("permission prompt should exist")
    );
    assert!(!preflight_body.contains(concat!("Privacy_", "Accessibility")));

    let native_permission_body = patch_between(
        "fn macos_accessibility_trusted_or_prompt()",
        "fn request_macos_accessibility_prompt",
        "macos_accessibility_trusted_or_prompt",
    );
    assert_contains_all(
        native_permission_body,
        &[
            "macos_accessibility_is_trusted_raw()",
            "AXIsProcessTrusted=true before prompt",
            "AXIsProcessTrusted=false before prompt",
        ],
    );
    assert!(
        native_permission_body
            .find("macos_accessibility_is_trusted_raw()")
            .expect("debugger check should read Accessibility state first")
            < native_permission_body
                .find("request_macos_accessibility_prompt")
                .expect("debugger check should prompt only after reading state")
    );
    assert!(!native_permission_body.contains(concat!("Privacy_", "Accessibility")));

    let background_retry_body = patch_between(
        "fn retry_localization_after_background_debugger_request()",
        "fn ensure_patch_files",
        "background retry helper",
    );
    assert!(background_retry_body.contains("request_claude_main_process_debugger_once()"));
    assert!(!background_retry_body.contains("wait_for_macos_claude_main_process_debugger()"));
    let silent_body = patch_between(
        "pub fn spawn_silent_localization_injector()",
        "fn emit_localization_progress",
        "spawn_silent_localization_injector",
    );
    assert!(silent_body.contains("run_background_localization_retry_loop(app)"));
    assert!(!silent_body.contains("wait_for_macos_claude_main_process_debugger()"));

    assert_contains_all(
        source,
        &[
            "ax_find_and_press_debugger_menu_item",
            "macos_main_process_debugger_menu_title_matches",
            "macos_developer_menu_title_matches",
            "normalized_menu_title",
        ],
    );
    for title in [
        "Developer",
        "开发者",
        "開發者",
        "Entwickler",
        "Desarrollador",
        "Développeur",
        "डेवलपर",
        "Pengembang",
        "Sviluppatore",
        "開発",
        "開発者",
        "개발자",
        "Desenvolvedor",
    ] {
        assert!(
            macos_developer_menu_title_matches(title),
            "developer menu title should match {title}"
        );
    }
    for title in [
        "Enable Main Process Debugger",
        "启用主进程调试器",
        "Main Process Debugger",
        "啟用主進程偵錯器",
        "Activer le débogueur du processus principal",
        "Hauptprozess-Debugger aktivieren",
        "Activar depurador del proceso principal",
        "Ativar depurador do processo principal",
        "メインプロセスデバッガーを有効にする",
        "메인 프로세스 디버거 활성화",
        "मुख्य प्रक्रिया डिबगर सक्षम करें",
        "Aktifkan debugger proses utama",
    ] {
        assert!(
            macos_main_process_debugger_menu_title_matches(title),
            "main process debugger title should match {title}"
        );
    }
    for title in [
        "Continue",
        "允许",
        "继续",
        "繼續",
        "Continuer",
        "Fortfahren",
        "Continuar",
        "Permitir",
        "Apri",
        "開く",
        "계속",
        "जारी रखें",
        "Lanjutkan",
    ] {
        assert!(
            macos_debugger_confirmation_title_matches(title),
            "debugger confirmation title should match {title}"
        );
    }

    let script = macos_localized_launch_script();
    assert_order(
        &script,
        "while ! claude_debugger_open; do",
        "debugger_attempts=$((debugger_attempts + 1))",
        "script should count debugger wait attempts while waiting for the endpoint",
    );
    assert_contains_none(
        &script,
        &[
            "osascript",
            "APPLESCRIPT",
            "JXA",
            "clickDebuggerConfirmation",
        ],
    );
}

#[test]
fn node_inspector_injection_consumes_localized_launch_marker() {
    let source = main_process_injection_source();

    assert_contains_all(
        &source,
        &[
            "localized-launch.flag",
            "consumeLocalizedLaunchMarker",
            "fs.unlinkSync(marker)",
            "localizedLaunchDefaultZh",
            "var __CSL_LL=",
            "__CSL_LL_DONE",
            "localStorage.setItem('spa:locale','zh-CN')",
        ],
    );
    assert_contains_none(
        &source,
        &["if(typeof __CSL_LL==='undefined')var __CSL_LL=!1;"],
    );
}

#[test]
fn node_inspector_injection_waits_for_real_renderer_attach() {
    let source = main_process_injection_source();

    assert_contains_all(
        &source,
        &[
            "await globalThis.__CODESTUDIO_CLAUDE_ZH_MAIN__.refresh()",
            "const results = await Promise.all",
            "if (attached.has(contents)) return true;",
            "attached.add(contents);",
            "return { ok: true, reused: false, ...summary };",
        ],
    );
    assert_contains_all(
        patch_source(),
        &["\"Runtime.evaluate\"", "\"awaitPromise\": true"],
    );
}

#[test]
fn node_inspector_injection_reinstalls_when_injection_changes_without_version_bump() {
    let source = main_process_injection_source();

    assert_contains_all(
        &source,
        &[
            "CSL_INJECTION_SIGNATURE",
            "injectionSignature === CSL_INJECTION_SIGNATURE",
            "previousInjectionSignature !== CSL_INJECTION_SIGNATURE",
            "contents.__cslZhAttachedInjectionSignature",
            "dispose",
        ],
    );
    assert_order(
        &source,
        "injectionSignature === CSL_INJECTION_SIGNATURE",
        "return { ok: true, reused: true, ...summary };",
        "same-injection reuse should stay available after comparing signatures",
    );
}

#[test]
fn node_inspector_injection_reload_is_timeout_guarded() {
    let source = main_process_injection_source();

    // The reload is wrapped in a timeout so a stalled Page.reload cannot
    // hang the async injection (which would block the inspector read loop).
    assert_contains_all(&source, &["Promise.race", "Page.reload"]);
    // A read timeout guards the CDP eval round-trip on the Rust side too.
    assert_contains_all(patch_source(), &["CLAUDE_INSPECTOR_EVAL_TIMEOUT"]);
}

#[test]
fn windows_claude_process_lookup_uses_visible_claude_main_processes() {
    let source = windows_find_claude_process_script(Some(1234));

    assert_contains_all(
        &source,
        &[
            "Get-Process -Name 'claude'",
            "StartTime",
            "Where-Object { $_.Path",
            "Select-Object -First 1",
        ],
    );
    assert_contains_none(
        &source,
        &[
            "Get-CimInstance Win32_Process -Filter \"name = 'Claude.exe'\"",
            "CreationDate -Descending",
        ],
    );
}

#[test]
fn windows_claude_process_lookup_returns_all_candidates_for_attach() {
    let source = windows_find_claude_process_script(Some(1234));

    assert_contains_all(
        &source,
        &["ForEach-Object", "[string]$_.Id", "$ordered += @($visible"],
    );
    assert_contains_none(&source, &["exit 0"]);
}

#[test]
fn inspector_target_lookup_reads_only_default_claude_port() {
    assert_contains_all(
        patch_source(),
        &[concat!(
            "read_node_inspector_targets_from_port(",
            "CLAUDE_NODE_INSPECT_PORT",
            ")"
        )],
    );
    assert_contains_none(
        patch_source(),
        &[concat!("all_targets", ".extend(targets)")],
    );
}

#[test]
fn node_inspector_identity_rejects_non_claude_electron_apps() {
    let other = json!({
        "execPath": "D:\\OtherApp\\OtherApp.exe",
        "appName": "OtherApp",
        "appPath": "D:\\OtherApp\\resources\\app.asar",
        "userData": "C:\\Users\\dreamyloong\\AppData\\Roaming\\OtherApp"
    });
    let claude = json!({
        "execPath": "C:\\Program Files\\WindowsApps\\Claude_1.14271.0.0_x64__pzs8sxrjxfjjc\\app\\Claude.exe",
        "appName": "Claude",
        "appPath": "C:\\Program Files\\WindowsApps\\Claude_1.14271.0.0_x64__pzs8sxrjxfjjc\\app\\resources\\app.asar",
        "userData": "C:\\Users\\dreamyloong\\AppData\\Local\\Claude-3p"
    });

    assert!(!node_inspector_identity_is_claude(&other));
    assert!(node_inspector_identity_is_claude(&claude));
}

#[test]
fn unrelated_tool_command_is_unchanged() {
    assert_eq!(
        patched_launch_command("codex", "codex", true).unwrap(),
        "codex"
    );
}

#[test]
fn patch_assets_intercept_native_locale_requests_without_dom_translation() {
    assert!(TRANSLATION_RUNTIME.contains("installLocaleWhitelist"));
    assert!(TRANSLATION_RUNTIME.contains("installLocalePersistence"));
    assert!(TRANSLATION_RUNTIME.contains("account_profile"));
    assert!(TRANSLATION_RUNTIME.contains("withZh"));
    assert!(TRANSLATION_RUNTIME.contains("fixLanguageRadio"));
    assert!(TRANSLATION_RUNTIME.contains("__cslWantedLocale"));
    assert!(
        TRANSLATION_RUNTIME
            .find("localStorage.getItem(CSL_WANTED_LOCALE_KEY)")
            .expect("wanted locale should be read")
            < TRANSLATION_RUNTIME
                .find("localStorage.getItem(\"spa:locale\")")
                .expect("spa locale should be a fallback")
    );
    assert!(TRANSLATION_RUNTIME.contains("rememberLocale"));
    assert!(TRANSLATION_RUNTIME.contains("localeRequestBodySync"));
    assert!(TRANSLATION_RUNTIME.contains(r#""Home": "\u9996\u9875""#));
    assert!(TRANSLATION_RUNTIME.contains(r#""Chat": "\u804a\u5929""#));
    assert!(TRANSLATION_RUNTIME.contains(r#""About Claude": "\u5173\u4e8eClaude""#));
    assert!(TRANSLATION_RUNTIME.contains(r#""Get support": "\u83b7\u53d6\u652f\u6301""#));
    assert!(TRANSLATION_RUNTIME.contains(r#""Version ": "\u7248\u672c""#));
    assert!(TRANSLATION_RUNTIME.contains("overrides.json"));
    assert!(TRANSLATION_RUNTIME.contains("gated_messages"));
    assert!(TRANSLATION_RUNTIME.contains("fetch"));
    assert!(format!("{:?}", cdp_locale_patterns()).contains("ion-dist/i18n/zh-CN.json"));
    assert!(format!("{:?}", cdp_locale_patterns()).contains("ion-dist/i18n/en-US.json"));
    assert!(cdp_locale_response(
        9,
        &json!({
            "method": "Fetch.requestPaused",
            "params": {
                "requestId": "intercept-1",
                "request": { "url": "https://claude.ai/ion-dist/i18n/zh-CN.json" }
            }
        })
    )
    .is_some());
}

#[test]
fn locale_runtime_translates_current_model_badges() {
    assert!(TRANSLATION_RUNTIME.contains("Currently unavailable"));
    assert!(TRANSLATION_RUNTIME.contains("For more complex tasks"));
    assert!(TRANSLATION_RUNTIME.contains("For complex tasks"));
    assert!(TRANSLATION_RUNTIME.contains("Disable bundled skills and workflows"));
    assert!(TRANSLATION_RUNTIME.contains("Disables Claude Code's bundled skills"));
    assert!(TRANSLATION_RUNTIME.contains("\\u7981\\u7528\\u5185\\u7f6e\\u6280\\u80fd"));
    assert!(TRANSLATION_RUNTIME.contains("\"Gateway\""));
    assert!(TRANSLATION_RUNTIME.contains("\"GATEWAY\""));
    assert!(!TRANSLATION_RUNTIME.contains("Can think for more complex tasks"));
    assert!(TRANSLATION_RUNTIME.contains("\\u590d\\u6742\\u4efb\\u52a1"));
}

#[test]
fn locale_runtime_translates_code_ui_label_without_skipping_code_named_containers() {
    assert!(TRANSLATION_RUNTIME.contains("\"Code\": \"\\u4ee3\\u7801\""));
    assert!(!TRANSLATION_RUNTIME.contains("[class*=\"code\"]"));
    assert!(TRANSLATION_RUNTIME.contains("codeUiLabelFallback"));
    assert!(TRANSLATION_RUNTIME.contains("uiOnlyDomFallback"));
    assert!(TRANSLATION_RUNTIME.contains("genSel"));
    assert!(TRANSLATION_RUNTIME.contains("uiHint"));
    assert!(TRANSLATION_RUNTIME.contains("shouldTranslateDomFallbackTextNode"));
    assert!(TRANSLATION_RUNTIME.contains("likelyUiTextNode(node)"));
    assert!(TRANSLATION_RUNTIME.contains("generatedContentTextNode(node)"));
    assert!(TRANSLATION_RUNTIME.contains("reversibleTextFallback"));
    assert!(TRANSLATION_RUNTIME.contains("__cslOrigText"));
    assert!(TRANSLATION_RUNTIME.contains("__cslTranslatedText"));
    assert!(TRANSLATION_RUNTIME.contains("restoreTextNode"));
    assert!(TRANSLATION_RUNTIME.contains("TEXT_EN"));
    assert!(TRANSLATION_RUNTIME.contains(r#"[class*="markdown"]"#));
    assert!(TRANSLATION_RUNTIME.contains(r#"[class*="prose"]"#));
}

#[test]
fn locale_runtime_translates_first_screen_copy_from_cache() {
    assert!(TRANSLATION_RUNTIME.contains("claudeFirstScreenFallback"));
    assert!(TRANSLATION_RUNTIME.contains("Let's knock something off your list"));
    assert!(
        TRANSLATION_RUNTIME.contains("\\u8ba9\\u6211\\u4eec\\u4ece\\u4f60\\u7684\\u6e05\\u5355")
    );
    assert!(TRANSLATION_RUNTIME.contains("What can I help you with today?"));
    assert!(TRANSLATION_RUNTIME.contains("Good morning, "));
    assert!(TRANSLATION_RUNTIME.contains("translatedWeekdayGreetingText"));
    assert!(TRANSLATION_RUNTIME.contains("Monday"));
    assert!(TRANSLATION_RUNTIME.contains("Tuesday"));
    assert!(TRANSLATION_RUNTIME.contains("Wednesday"));
    assert!(TRANSLATION_RUNTIME.contains("Thursday"));
    assert!(TRANSLATION_RUNTIME.contains("Friday"));
    assert!(TRANSLATION_RUNTIME.contains("Saturday"));
    assert!(TRANSLATION_RUNTIME.contains("Sunday"));
    assert!(TRANSLATION_RUNTIME.contains("\\u5468\\u4e00"));
    assert!(TRANSLATION_RUNTIME.contains("\\u5468\\u65e5"));
    assert!(TRANSLATION_RUNTIME.contains("\\u5feb\\u4e50"));
    assert!(TRANSLATION_RUNTIME.contains("translatedFirstScreenTextValue"));
}

#[test]
fn locale_runtime_translates_current_onboarding_cta_fallbacks() {
    assert!(TRANSLATION_RUNTIME.contains("Turn on memory"));
    assert!(TRANSLATION_RUNTIME.contains("\\u5f00\\u542f\\u8bb0\\u5fc6"));
    assert!(TRANSLATION_RUNTIME.contains("Get started with Claude"));
    assert!(TRANSLATION_RUNTIME.contains("\\u5f00\\u59cb\\u4f7f\\u7528 Claude"));
    assert!(TRANSLATION_RUNTIME.contains("Try Cowork"));
    assert!(TRANSLATION_RUNTIME.contains("\\u8bd5\\u8bd5 Cowork"));
    assert!(TRANSLATION_RUNTIME.contains("Upgrade to let Claude take on real tasks for you"));
    assert!(TRANSLATION_RUNTIME.contains("\\u5347\\u7ea7\\uff0c\\u8ba9 Claude"));
}

#[test]
fn bundled_zh_locale_uses_curated_terms_for_known_machine_translation_regressions() {
    let ion: Value = serde_json::from_str(CLAUDE_ION_ZH_LOCALE).expect("ion zh locale json");
    let Some(map) = ion.as_object() else {
        panic!("ion zh locale should be an object");
    };

    let expectations = [
        LocaleExpectation {
            key: "4ahpF5N/t0",
            label: "tedious task marketing copy",
            expected: "推进繁琐任务",
            forbidden: &["坚持"],
        },
        LocaleExpectation {
            key: "ye9sGm7rX3",
            label: "shipping features marketing copy",
            expected: "发布功能，而不是堆代码行数",
            forbidden: &["船只", "线条"],
        },
        LocaleExpectation {
            key: "HqlBRpo6tx",
            label: "relaunch update button",
            expected: "重新启动以应用更新",
            forbidden: &["发布", "以更新"],
        },
        LocaleExpectation {
            key: "0hPFsTuQ1X",
            label: "inference request header help text",
            expected: "每次向配置的提供方发送推理请求时额外附加 HTTP 请求头。可用于租户级路由、组织 ID、Bedrock Guardrails 等场景。",
            forbidden: &["租户路由", "基岩", "护栏"],
        },
        LocaleExpectation {
            key: "CJsWpnmYD4",
            label: "home hero task copy",
            expected: "让我们帮你完成一项待办",
            forbidden: &["砍掉"],
        },
        LocaleExpectation {
            key: "JQs8c3pGcl",
            label: "gateway base url label",
            expected: "网关 Base URL",
            forbidden: &["网关基网"],
        },
        LocaleExpectation {
            key: "/m0q/Dre6A",
            label: "api model id label",
            expected: "模型 ID",
            forbidden: &["型号"],
        },
        LocaleExpectation {
            key: "hv0F38ESRM",
            label: "api model list label",
            expected: "模型列表",
            forbidden: &["型号"],
        },
        LocaleExpectation {
            key: "g8BMTiGHB6",
            label: "api credential type label",
            expected: "凭据类型",
            forbidden: &["证书类型"],
        },
        LocaleExpectation {
            key: "y/6sGoi9YF",
            label: "connectors and extensions settings nav",
            expected: "连接器与扩展",
            forbidden: &["连接件", "延伸"],
        },
        LocaleExpectation {
            key: "xY1EE6Ndl5",
            label: "egress requirements settings nav",
            expected: "网络出口要求",
            forbidden: &["逃生"],
        },
        LocaleExpectation {
            key: "v7i2fnB2+A",
            label: "desktop general settings heading",
            expected: "桌面通用设置",
            forbidden: &["桌面一般设置"],
        },
        LocaleExpectation {
            key: "BYTC25E9Co",
            label: "all extensions breadcrumb",
            expected: "所有扩展",
            forbidden: &["所有延伸"],
        },
        LocaleExpectation {
            key: "5oTa1gWQsk",
            label: "allowed egress hosts label",
            expected: "允许外连主机",
            forbidden: &["逃逸"],
        },
        LocaleExpectation {
            key: "XhBz9JhRHE",
            label: "allowed surfaces label",
            expected: "允许使用的界面",
            forbidden: &["表面"],
        },
        LocaleExpectation {
            key: "aTTY7rU6Bh",
            label: "max tokens per window label",
            expected: "每个窗口最大 Token 数",
            forbidden: &["令牌数", "代币"],
        },
        LocaleExpectation {
            key: "P6EE/aQ7SS",
            label: "token unit label",
            expected: "Token",
            forbidden: &["代币"],
        },
        LocaleExpectation {
            key: "aH4De2Y20k",
            label: "remote settings source nav",
            expected: "配置来源",
            forbidden: &["资料来源"],
        },
        LocaleExpectation {
            key: "fHh9zisMLR",
            label: "general limits heading",
            expected: "通用限制",
            forbidden: &["一般限制"],
        },
        LocaleExpectation {
            key: "CCUxBOb3va",
            label: "disable claude protocol handling label",
            expected: "禁用 claude:// 深度链接处理",
            forbidden: &["深链"],
        },
        LocaleExpectation {
            key: "SwdskAOTqO",
            label: "bundled node setting help text",
            expected: "启用后，Claude 不会使用系统 Node.js 来运行扩展 MCP 服务器。当系统 Node.js 缺失或过时时，也会自动使用内置 Node.js。",
            forbidden: &["永远不会", "系统Node"],
        },
        LocaleExpectation {
            key: "Q6zr1vGzlF",
            label: "bootstrap config toggle label",
            expected: "使用 Bootstrap 配置",
            forbidden: &["bootstrap 配置"],
        },
        LocaleExpectation {
            key: "qjMZL1XFnR",
            label: "bootstrap config server label",
            expected: "Bootstrap 配置服务器",
            forbidden: &["config server"],
        },
        LocaleExpectation {
            key: "ozzKmITBMv",
            label: "usage limit help text",
            expected: "按账户设置的每日软上限，由客户端在指定时间窗口内统计。不是服务器强制执行的配额。",
            forbidden: &["下文期间"],
        },
        LocaleExpectation {
            key: "tmwK1KjFte",
            label: "gateway auth scheme label",
            expected: "网关认证方式",
            forbidden: &["方案"],
        },
        LocaleExpectation {
            key: "rdKIIOydC8",
            label: "block baseline telemetry label",
            expected: "阻止基础遥测",
            forbidden: &["块本质"],
        },
        LocaleExpectation {
            key: "6T78KTXhBM",
            label: "custom inference headers label",
            expected: "自定义推理请求头",
            forbidden: &["推理头"],
        },
        LocaleExpectation {
            key: "DPJjsItBww",
            label: "scheduled tasks nav",
            expected: "计划任务",
            forbidden: &["预定"],
        },
        LocaleExpectation {
            key: "4EAtPWhM42",
            label: "interface font Anthropic Sans option",
            expected: "Anthropic Sans",
            forbidden: &["拟人桑斯"],
        },
        LocaleExpectation {
            key: "BPnT3TVya+",
            label: "transcript text size small option",
            expected: "小",
            forbidden: &[],
        },
        LocaleExpectation {
            key: "ovJ26CKo4Q",
            label: "transcript text size and width medium option",
            expected: "中",
            forbidden: &["媒介"],
        },
        LocaleExpectation {
            key: "/06iwcQHPz",
            label: "transcript text size large option",
            expected: "大",
            forbidden: &["大型"],
        },
        LocaleExpectation {
            key: "Cs33xZFR6o",
            label: "transcript width narrow option",
            expected: "窄",
            forbidden: &["狭窄"],
        },
        LocaleExpectation {
            key: "PSiaaVYiAT",
            label: "transcript width wide option",
            expected: "宽",
            forbidden: &[],
        },
        LocaleExpectation {
            key: "akXG4ChYkN",
            label: "enable remote control by default setting",
            expected: "默认启用遥控",
            forbidden: &["遥控器"],
        },
        LocaleExpectation {
            key: "/JL5gAMv5z",
            label: "confidence level medium option",
            expected: "中",
            forbidden: &["媒介"],
        },
        LocaleExpectation {
            key: "6SI3PVzMTR",
            label: "severity medium badge",
            expected: "中",
            forbidden: &["媒介"],
        },
        LocaleExpectation {
            key: "5OIWPYyjEY",
            label: "turn on memory button",
            expected: "开启记忆",
            forbidden: &["内存"],
        },
        LocaleExpectation {
            key: "demo.h",
            label: "get started with Claude onboarding button",
            expected: "开始使用 Claude",
            forbidden: &["Get started"],
        },
        LocaleExpectation {
            key: "vm1bq3+TX8",
            label: "get started with Claude alternate onboarding button",
            expected: "开始使用 Claude",
            forbidden: &["Get started"],
        },
        LocaleExpectation {
            key: "ejEGdxSUGs",
            label: "home nav label",
            expected: "首页",
            forbidden: &["Home"],
        },
        LocaleExpectation {
            key: "5vDuzZxMbU",
            label: "try Cowork button",
            expected: "试试 Cowork",
            forbidden: &["Try Cowork"],
        },
        LocaleExpectation {
            key: "o2vHDAOpDQ",
            label: "try Cowork alternate button",
            expected: "试试 Cowork",
            forbidden: &["Try Cowork", "试试Cowork吧"],
        },
        LocaleExpectation {
            key: "MlGy39Hf4h",
            label: "upgrade for real tasks banner",
            expected: "升级，让 Claude 为你处理真正的任务",
            forbidden: &["Upgrade to let Claude"],
        },
        LocaleExpectation {
            key: "peYOflkXOK",
            label: "turn on memory in settings help text",
            expected: "如需在聊天间记住上下文，请<link>在设置中开启记忆</link>。",
            forbidden: &["turn on memory", "内存"],
        },
        LocaleExpectation {
            key: "+ZbIiH1928",
            label: "generate memory from chats opt-in",
            expected: "允许 Claude 根据你的聊天生成记忆。",
            forbidden: &["内存", "存储器"],
        },
        LocaleExpectation {
            key: "9jlvZMEAKh",
            label: "read memory label",
            expected: "读取记忆",
            forbidden: &["内存", "存储器"],
        },
        LocaleExpectation {
            key: "demo.memoryT",
            label: "import memory onboarding label",
            expected: "从其他 AI 导入记忆",
            forbidden: &["内存", "存储器"],
        },
        LocaleExpectation {
            key: "kvTeMA0Bfz",
            label: "save to memory failure message",
            expected: "Claude 无法保存到记忆。",
            forbidden: &["内存", "存储器"],
        },
        LocaleExpectation {
            key: "sjdp6mtP7Y",
            label: "reading memory progress label",
            expected: "正在读取记忆",
            forbidden: &["内存", "存储器"],
        },
    ];

    assert_locale_expectations(map, &expectations);
}

#[test]
fn locale_runtime_source_stays_small() {
    let source = build_locale_runtime_source();
    assert!(source.len() < 19_000);
    assert_contains_none(&source, &["__CLAUDE_ZH_ION_LOCALE__", CLAUDE_ION_ZH_LOCALE]);
}

#[test]
fn locale_payload_selection_matches_shell_and_ion_urls() {
    assert_eq!(
        locale_payload_for_url("https://claude.ai/ion-dist/i18n/zh-CN.json"),
        Some(CLAUDE_ION_ZH_LOCALE)
    );
    assert_eq!(
        locale_payload_for_url("file:///C:/Claude/i18n/zh-CN.json?v=1"),
        Some(CLAUDE_ION_ZH_LOCALE)
    );
    assert_eq!(
        locale_payload_for_url("https://claude.ai/ion-dist/i18n/en-US.json"),
        None
    );
    assert_eq!(locale_payload_for_url("file:///C:/Claude/en-US.json"), None);
    assert_eq!(
        locale_payload_for_url_with_locale("app://localhost/i18n/en-US.json", "zh-CN"),
        Some(CLAUDE_ION_ZH_LOCALE)
    );
    assert_eq!(
        locale_payload_for_url_with_locale("file:///C:/Claude/en-US.json", "zh-CN"),
        Some(CLAUDE_SHELL_ZH_LOCALE)
    );
    assert_eq!(
        locale_payload_for_url_with_locale("https://claude.ai/ion-dist/i18n/en-US.json", "zh-CN"),
        None
    );
    assert_eq!(locale_payload_for_url("https://claude.ai/app.js"), None);
}

#[test]
fn bundled_zh_locale_payloads_are_valid_and_substantial() {
    let shell = serde_json::from_str::<Value>(CLAUDE_SHELL_ZH_LOCALE).unwrap();
    let ion = serde_json::from_str::<Value>(CLAUDE_ION_ZH_LOCALE).unwrap();

    assert!(shell.as_object().map(|value| value.len()).unwrap_or(0) > 400);
    assert!(ion.as_object().map(|value| value.len()).unwrap_or(0) > 16_000);
    assert!(CLAUDE_SHELL_ZH_LOCALE.contains("复制"));
    assert!(CLAUDE_ION_ZH_LOCALE.contains("Claude"));
    assert_eq!(
        ion.get("+8nVZyI6SB").and_then(Value::as_str),
        Some("<b>{category}</b> 需要 {count, plural, one {{label}} other {# 个字段}}")
    );
    assert_eq!(
        ion.get("vN8KEpa87z").and_then(Value::as_str),
        Some("完成 {category} 中的 {count, plural, one {# 个必填字段} other {# 个必填字段}}")
    );
    assert_eq!(
        ion.get("2oJ53OuEpZ").and_then(Value::as_str),
        Some("连接器最多可发送 {max, plural, one {# 个请求标头} other {# 个请求标头}}。")
    );
    assert_eq!(
        ion.get("YYeIWoKm4P").and_then(Value::as_str),
        Some("{count, plural, one {# 个字段已更改} other {# 个字段已更改}}")
    );
    for key in ["+8nVZyI6SB", "vN8KEpa87z", "2oJ53OuEpZ", "YYeIWoKm4P"] {
        let value = ion.get(key).and_then(Value::as_str).unwrap_or("");
        assert!(!value.contains("fields"));
        assert!(!value.contains("field changed"));
        assert!(!value.contains("required field"));
        assert!(!value.contains("request header"));
        assert!(!value.contains("，复数"));
        assert!(!value.contains("另一个"));
    }
}
#[test]
fn extract_runtime_injection_to_temp_when_requested() {
    if std::env::var("CSL_EXTRACT_RUNTIME_INJECTION").is_err() {
        return;
    }
    let source = main_process_injection_source();
    let dir = std::env::temp_dir().join("csldiag");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("runtime-injection.js");
    std::fs::write(&path, &source).unwrap();
    println!("WROTE_RUNTIME_INJECTION:{}", path.display());
}
