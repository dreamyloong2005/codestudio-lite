use super::*;
use crate::core::process_control;

pub(in crate::core::profile) struct RestartOutcome {
    pub performed: bool,
    pub message: Option<String>,
}

struct RestartProcessResult {
    total: u64,
    forced: u64,
    remaining: u64,
    paths: Vec<String>,
}

#[derive(Clone, Copy, Default)]
pub(in crate::core::profile) struct RestartContext {
    pub sync_claude_vs_code: bool,
}

#[derive(Clone, Copy)]
pub(in crate::core::profile) enum RestartLaunch {
    CloseOnly,
    ChatGptDesktop,
    Command {
        command: &'static str,
        hidden: bool,
    },
    ExistingProcessPath {
        fallback_command: &'static str,
        hidden: bool,
    },
    MsixPackage {
        package_identities: &'static [&'static str],
    },
}

#[derive(Clone, Copy)]
pub(in crate::core::profile) struct RestartTarget {
    pub label: &'static str,
    pub process_names: &'static [&'static str],
    pub command_markers: &'static [&'static str],
    pub exclude_command_markers: &'static [&'static str],
    pub require_window: bool,
    pub reject_window: bool,
    pub launch: RestartLaunch,
}

pub(in crate::core::profile) fn restart_tool_for_profile(
    profile: &ProfileDraft,
    context: RestartContext,
) -> Result<RestartOutcome, String> {
    let app = canonical_profile_app(&profile.app);
    let targets = restart_targets_for_app(&app, context);
    if targets.is_empty() {
        return Ok(RestartOutcome {
            performed: false,
            message: Some(format!(
                "Tool '{}' does not have a client that needs automatic restart.",
                profile.app
            )),
        });
    }

    let mut messages = Vec::new();
    let mut restarted_any = false;
    let mut stopped_targets = Vec::new();

    for target in targets {
        let result = stop_restart_target_processes(target)?;
        if result.total == 0 {
            continue;
        }
        if result.remaining > 0 {
            return Err(format!(
                "{} is still running; restart was not continued.",
                target.label
            ));
        }

        stopped_targets.push((target, result));
    }

    for (target, result) in stopped_targets {
        launch_restart_target(target, &result.paths)
            .map_err(|err| format!("Failed to restart {}: {err}", target.label))?;
        restarted_any = true;
        messages.push(restart_target_message(target, &result));
    }

    if restarted_any {
        Ok(RestartOutcome {
            performed: true,
            message: Some(messages.join(" ")),
        })
    } else {
        Ok(RestartOutcome {
            performed: false,
            message: Some(format!(
                "{} is not running, so no restart is needed.",
                restart_category_label(&app, context)
            )),
        })
    }
}

pub(in crate::core::profile) fn restart_targets_for_app(
    app: &str,
    context: RestartContext,
) -> Vec<RestartTarget> {
    const CODEX_DESKTOP_NAMES: &[&str] = &["ChatGPT.exe", "ChatGPT", "Codex.exe", "Codex"];
    const CODEX_CLI_NAMES: &[&str] = &["codex.exe", "codex"];
    const CODEX_CLI_MARKERS: &[&str] = &[
        "@openai/codex",
        "@openai\\codex",
        "node_modules/@openai/codex",
        "node_modules\\@openai\\codex",
    ];
    const VSCODE_NAMES: &[&str] = &["Code.exe", "Code", "Code - Insiders.exe", "Code - Insiders"];
    const CODEX_VSCODE_BACKEND_MARKERS: &[&str] = &[
        ".vscode/extensions/openai.chatgpt",
        ".vscode\\extensions\\openai.chatgpt",
        "codex app-server",
        "codex.exe app-server",
    ];
    const CLAUDE_DESKTOP_NAMES: &[&str] = &["Claude.exe", "Claude"];
    const CLAUDE_CLI_NAMES: &[&str] = &["claude.exe", "claude"];
    const CLAUDE_CLI_MARKERS: &[&str] = &[
        "@anthropic-ai/claude-code",
        "@anthropic-ai\\claude-code",
        "node_modules/@anthropic-ai/claude-code",
        "node_modules\\@anthropic-ai\\claude-code",
    ];
    const CLAUDE_VSCODE_BACKEND_MARKERS: &[&str] = &[
        ".vscode/extensions/anthropic.claude-code",
        ".vscode\\extensions\\anthropic.claude-code",
        "resources/native-binary/claude",
        "resources\\native-binary\\claude",
    ];
    const GEMINI_CLI_NAMES: &[&str] = &["gemini.exe", "gemini"];
    const GEMINI_CLI_MARKERS: &[&str] = &["@google/gemini-cli", "@google\\gemini-cli"];
    const OPENCODE_NAMES: &[&str] = &["opencode.exe", "opencode"];
    const OPENCODE_MARKERS: &[&str] = &["opencode-ai"];
    const OPENCLAW_NAMES: &[&str] = &["openclaw.exe", "openclaw"];
    const HERMES_NAMES: &[&str] = &["hermes.exe", "hermes", "Hermes"];
    const GROK_NAMES: &[&str] = &["grok.exe", "grok", "Grok"];
    const PI_NAMES: &[&str] = &["pi.exe", "pi", "Pi"];
    const EMPTY: &[&str] = &[];
    match app {
        "codex" => vec![
            RestartTarget {
                label: "Codex",
                process_names: CODEX_DESKTOP_NAMES,
                command_markers: EMPTY,
                exclude_command_markers: EMPTY,
                require_window: true,
                reject_window: false,
                launch: RestartLaunch::ChatGptDesktop,
            },
            RestartTarget {
                label: "Codex VS Code extension backend",
                process_names: EMPTY,
                command_markers: CODEX_VSCODE_BACKEND_MARKERS,
                exclude_command_markers: EMPTY,
                require_window: false,
                reject_window: false,
                launch: RestartLaunch::CloseOnly,
            },
            RestartTarget {
                label: "Codex CLI",
                process_names: CODEX_CLI_NAMES,
                command_markers: CODEX_CLI_MARKERS,
                exclude_command_markers: CODEX_VSCODE_BACKEND_MARKERS,
                require_window: false,
                reject_window: true,
                launch: RestartLaunch::Command {
                    command: "codex",
                    hidden: true,
                },
            },
        ],
        "claude-desktop" => vec![RestartTarget {
            label: "Claude Desktop",
            process_names: CLAUDE_DESKTOP_NAMES,
            command_markers: EMPTY,
            exclude_command_markers: EMPTY,
            require_window: true,
            reject_window: false,
            launch: if cfg!(target_os = "windows") {
                RestartLaunch::MsixPackage {
                    package_identities: detector::claude_desktop_windows_package_identities(),
                }
            } else {
                RestartLaunch::ExistingProcessPath {
                    fallback_command: "Claude",
                    hidden: false,
                }
            },
        }],
        "claude" => {
            let mut targets = vec![RestartTarget {
                label: "Claude Code",
                process_names: CLAUDE_CLI_NAMES,
                command_markers: CLAUDE_CLI_MARKERS,
                exclude_command_markers: CLAUDE_VSCODE_BACKEND_MARKERS,
                require_window: false,
                reject_window: true,
                launch: RestartLaunch::Command {
                    command: "claude",
                    hidden: true,
                },
            }];
            if context.sync_claude_vs_code {
                targets.push(RestartTarget {
                    label: "Claude VS Code extension backend",
                    process_names: EMPTY,
                    command_markers: CLAUDE_VSCODE_BACKEND_MARKERS,
                    exclude_command_markers: EMPTY,
                    require_window: false,
                    reject_window: false,
                    launch: RestartLaunch::CloseOnly,
                });
            }
            targets
        }
        "gemini" => vec![RestartTarget {
            label: "Gemini CLI",
            process_names: GEMINI_CLI_NAMES,
            command_markers: GEMINI_CLI_MARKERS,
            exclude_command_markers: EMPTY,
            require_window: false,
            reject_window: false,
            launch: RestartLaunch::Command {
                command: "gemini",
                hidden: true,
            },
        }],
        "gemini-code-assist" => vec![RestartTarget {
            label: "Gemini Code Assist",
            process_names: VSCODE_NAMES,
            command_markers: EMPTY,
            exclude_command_markers: EMPTY,
            require_window: true,
            reject_window: false,
            launch: RestartLaunch::ExistingProcessPath {
                fallback_command: "code",
                hidden: false,
            },
        }],
        "opencode" => vec![RestartTarget {
            label: "OpenCode",
            process_names: OPENCODE_NAMES,
            command_markers: OPENCODE_MARKERS,
            exclude_command_markers: EMPTY,
            require_window: false,
            reject_window: false,
            launch: RestartLaunch::Command {
                command: "opencode",
                hidden: true,
            },
        }],
        "openclaw" => vec![RestartTarget {
            label: "OpenClaw",
            process_names: OPENCLAW_NAMES,
            command_markers: EMPTY,
            exclude_command_markers: EMPTY,
            require_window: false,
            reject_window: false,
            launch: RestartLaunch::Command {
                command: "openclaw",
                hidden: true,
            },
        }],
        "hermes" => vec![RestartTarget {
            label: "Hermes",
            process_names: HERMES_NAMES,
            command_markers: EMPTY,
            exclude_command_markers: EMPTY,
            require_window: false,
            reject_window: false,
            launch: RestartLaunch::Command {
                command: "hermes",
                hidden: true,
            },
        }],
        "grok" => vec![RestartTarget {
            label: "Grok",
            process_names: GROK_NAMES,
            command_markers: EMPTY,
            exclude_command_markers: EMPTY,
            require_window: false,
            reject_window: false,
            launch: RestartLaunch::Command {
                command: "grok",
                hidden: true,
            },
        }],
        "pi" => vec![RestartTarget {
            label: "Pi Agent",
            process_names: PI_NAMES,
            command_markers: EMPTY,
            exclude_command_markers: EMPTY,
            require_window: false,
            reject_window: false,
            launch: RestartLaunch::Command {
                command: "pi",
                hidden: true,
            },
        }],
        _ => Vec::new(),
    }
}

fn restart_category_label(app: &str, context: RestartContext) -> &'static str {
    match app {
        "codex" => "Codex, Codex CLI, or Codex VS Code extension backend",
        "claude-desktop" => "Claude Desktop",
        "claude" if context.sync_claude_vs_code => {
            "Claude Code or Claude VS Code extension backend"
        }
        "claude" => "Claude Code",
        "gemini" => "Gemini CLI",
        "gemini-code-assist" => "Gemini Code Assist",
        "opencode" => "OpenCode",
        "openclaw" => "OpenClaw",
        "hermes" => "Hermes",
        "grok" => "Grok",
        "pi" => "Pi Agent",
        _ => "target tool",
    }
}

fn restart_target_message(target: RestartTarget, result: &RestartProcessResult) -> String {
    if matches!(target.launch, RestartLaunch::CloseOnly) {
        if result.forced > 0 {
            return format!(
                "Force-closed {} {} process(es); VS Code will restart the backend when needed.",
                result.forced, target.label
            );
        }
        return format!("Restarted {}.", target.label);
    }

    if result.forced > 0 {
        format!(
            "Force-closed {} {} process(es) and restarted.",
            result.forced, target.label
        )
    } else {
        format!("Restarted {}.", target.label)
    }
}

fn stop_restart_target_processes(target: RestartTarget) -> Result<RestartProcessResult, String> {
    if cfg!(target_os = "macos") {
        return stop_restart_target_processes_macos(target);
    }

    if !cfg!(target_os = "windows") {
        return Ok(RestartProcessResult {
            total: 0,
            forced: 0,
            remaining: 0,
            paths: Vec::new(),
        });
    }

    let script = windows_restart_process_script(target);
    let json = run_powershell(&script).map_err(|err| {
        format!(
            "Failed to inspect and close {} for restart: {err}",
            target.label
        )
    })?;
    #[derive(Deserialize)]
    struct RawRestartProcessResult {
        total: Option<u64>,
        forced: Option<u64>,
        remaining: Option<u64>,
        #[serde(default)]
        paths: Vec<String>,
    }
    let value: RawRestartProcessResult = serde_json::from_str(&json)
        .map_err(|err| format!("Failed to parse {} restart result: {err}", target.label))?;
    Ok(RestartProcessResult {
        total: value.total.unwrap_or(0),
        forced: value.forced.unwrap_or(0),
        remaining: value.remaining.unwrap_or(0),
        paths: value.paths,
    })
}

pub(in crate::core::profile) fn windows_restart_process_script(target: RestartTarget) -> String {
    format!(
        r#"
$ErrorActionPreference = 'Stop'
$Names = {names}
$Markers = {markers}
$ExcludeMarkers = {exclude_markers}
$RequireWindow = ${require_window}
$RejectWindow = ${reject_window}
function Test-TargetProcess($process) {{
  $name = [string]$process.Name
  $nameMatch = $false
  foreach ($candidate in $Names) {{
    if ($name.Equals($candidate, [System.StringComparison]::OrdinalIgnoreCase)) {{
      $nameMatch = $true
      break
    }}
  }}
  $markerMatch = $false
  if ($Markers.Count -gt 0) {{
    $haystack = ((([string]$process.CommandLine) + "`n" + ([string]$process.ExecutablePath))).ToLowerInvariant()
    foreach ($marker in $Markers) {{
      if ($haystack.Contains(([string]$marker).ToLowerInvariant())) {{
        $markerMatch = $true
        break
      }}
    }}
  }}
	  if (-not ($nameMatch -or $markerMatch)) {{ return $false }}
	  if ($ExcludeMarkers.Count -gt 0) {{
	    $haystack = ((([string]$process.CommandLine) + "`n" + ([string]$process.ExecutablePath))).ToLowerInvariant()
	    foreach ($marker in $ExcludeMarkers) {{
	      if ($haystack.Contains(([string]$marker).ToLowerInvariant())) {{
	        return $false
	      }}
	    }}
	  }}
	  if ($RequireWindow) {{
	    try {{
	      $gp = Get-Process -Id $process.ProcessId -ErrorAction Stop
	      if ($gp.MainWindowHandle -eq 0) {{ return $false }}
	    }} catch {{
	      return $false
	    }}
	  }}
	  if ($RejectWindow) {{
	    try {{
	      $gp = Get-Process -Id $process.ProcessId -ErrorAction Stop
	      if ($gp.MainWindowHandle -ne 0) {{ return $false }}
	    }} catch {{}}
	  }}
	  return $true
	}}
function ConvertTo-ProcessSnapshot($process) {{
  $path = ''
  try {{ $path = [string]$process.Path }} catch {{}}
  [pscustomobject]@{{
    ProcessId = [int]$process.Id
    Name = [string]$process.ProcessName
    CommandLine = ''
    ExecutablePath = $path
  }}
}}
$procs = @()
try {{
  $procs = @(Get-CimInstance Win32_Process -ErrorAction Stop | Where-Object {{ Test-TargetProcess $_ }})
}} catch {{
  if ($Names.Count -gt 0 -and $ExcludeMarkers.Count -eq 0) {{
    foreach ($candidate in $Names) {{
      $clean = [System.IO.Path]::GetFileNameWithoutExtension([string]$candidate)
      if (-not $clean) {{ continue }}
      $procs += @(Get-Process -Name $clean -ErrorAction SilentlyContinue | ForEach-Object {{
        ConvertTo-ProcessSnapshot $_
      }})
    }}
    $procs = @($procs | Where-Object {{ Test-TargetProcess $_ }})
  }}
}}
$procs = @($procs | Sort-Object -Property ProcessId -Unique)
$targetIds = @($procs | ForEach-Object {{ [int]$_.ProcessId }})
$paths = @($procs | ForEach-Object {{ [string]$_.ExecutablePath }} | Where-Object {{ $_ }} | Select-Object -Unique)
foreach ($id in $targetIds) {{
  try {{
    $p = Get-Process -Id $id -ErrorAction Stop
    if ($p.MainWindowHandle -ne 0) {{ [void]$p.CloseMainWindow() }}
  }} catch {{}}
}}
$deadline = (Get-Date).AddSeconds(8)
while ((Get-Date) -lt $deadline) {{
  Start-Sleep -Milliseconds 250
  $remaining = @()
  foreach ($id in $targetIds) {{
    $p = Get-Process -Id $id -ErrorAction SilentlyContinue
    if ($null -ne $p) {{ $remaining += $p }}
  }}
  if ($remaining.Count -eq 0) {{ break }}
}}
$remaining = @()
foreach ($id in $targetIds) {{
  $p = Get-Process -Id $id -ErrorAction SilentlyContinue
  if ($null -ne $p) {{ $remaining += $p }}
}}
{force_termination}
[pscustomobject]@{{
  total = [int](@($targetIds).Count)
  forced = [int]$forced
  remaining = [int](@($still).Count)
  paths = @($paths)
}} | ConvertTo-Json -Compress
"#,
        names = ps_array(target.process_names),
        markers = ps_array(target.command_markers),
        exclude_markers = ps_array(target.exclude_command_markers),
        require_window = if target.require_window {
            "true"
        } else {
            "false"
        },
        reject_window = if target.reject_window {
            "true"
        } else {
            "false"
        },
        force_termination = process_control::windows_force_termination_script(),
    )
}

fn launch_restart_target(target: RestartTarget, paths: &[String]) -> Result<(), String> {
    match target.launch {
        RestartLaunch::CloseOnly => Ok(()),
        RestartLaunch::ChatGptDesktop => chatgpt_desktop::launch(),
        RestartLaunch::Command { command, hidden } => launch_process(command, hidden),
        RestartLaunch::ExistingProcessPath {
            fallback_command,
            hidden,
        } => {
            let mut launched = false;
            for path in paths.iter().filter(|path| !path.trim().is_empty()) {
                launch_process(path, hidden)?;
                launched = true;
            }
            if !launched {
                launch_process(fallback_command, hidden)?;
            }
            Ok(())
        }
        RestartLaunch::MsixPackage { package_identities } => {
            let args = Vec::new();
            package::launch_first_msix_package_with_args(package_identities, &args).map(|_| ())
        }
    }
}

fn stop_restart_target_processes_macos(
    target: RestartTarget,
) -> Result<RestartProcessResult, String> {
    let target_ids = collect_macos_restart_target_pids(target)?;
    if target_ids.is_empty() {
        return Ok(RestartProcessResult {
            total: 0,
            forced: 0,
            remaining: 0,
            paths: Vec::new(),
        });
    }

    let paths = if matches!(target.launch, RestartLaunch::ExistingProcessPath { .. }) {
        target_ids
            .iter()
            .filter_map(|pid| macos_restart_process_executable_path(*pid))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };

    for name in target.process_names {
        quit_macos_restart_app_by_name(name);
    }
    wait_for_macos_restart_process_exit(&target_ids, Duration::from_secs(8));

    let remaining_after_quit = target_ids
        .iter()
        .copied()
        .filter(|pid| macos_restart_pid_alive(*pid))
        .collect::<Vec<_>>();
    for pid in &remaining_after_quit {
        let _ = hidden_command_with_args("kill", &["-TERM", &pid.to_string()]).output();
    }
    wait_for_macos_restart_process_exit(&remaining_after_quit, Duration::from_secs(2));

    let remaining_after_term = remaining_after_quit
        .iter()
        .copied()
        .filter(|pid| macos_restart_pid_alive(*pid))
        .collect::<Vec<_>>();
    let mut forced = 0;
    for pid in &remaining_after_term {
        let output = hidden_command_with_args("kill", &["-KILL", &pid.to_string()])
            .output()
            .map_err(|err| format!("Failed to force-close {}: {err}", target.label))?;
        if output.status.success() {
            forced += 1;
        }
    }
    wait_for_macos_restart_process_exit(&remaining_after_term, Duration::from_millis(500));

    let remaining = target_ids
        .iter()
        .copied()
        .filter(|pid| macos_restart_pid_alive(*pid))
        .count() as u64;

    Ok(RestartProcessResult {
        total: target_ids.len() as u64,
        forced,
        remaining,
        paths,
    })
}

fn collect_macos_restart_target_pids(target: RestartTarget) -> Result<Vec<u32>, String> {
    let mut ids = BTreeSet::new();
    for name in target.process_names {
        let clean_name = macos_restart_process_name(name);
        if clean_name.is_empty() {
            continue;
        }
        for pid in pgrep_macos_for_restart(&["-x", clean_name.as_str()])? {
            ids.insert(pid);
        }
    }
    for marker in target.command_markers {
        if marker.trim().is_empty() {
            continue;
        }
        for pid in pgrep_macos_for_restart(&["-f", marker])? {
            ids.insert(pid);
        }
    }

    let current_pid = std::process::id();
    Ok(ids
        .into_iter()
        .filter(|pid| *pid != current_pid)
        .filter(|pid| !macos_restart_process_has_any_marker(*pid, target.exclude_command_markers))
        .collect())
}

fn pgrep_macos_for_restart(args: &[&str]) -> Result<Vec<u32>, String> {
    let output = hidden_command_with_args("pgrep", args)
        .output()
        .map_err(|err| format!("Failed to run pgrep: {err}"))?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .collect())
}

fn macos_restart_process_executable_path(pid: u32) -> Option<String> {
    let pid = pid.to_string();
    let output = hidden_command_with_args("ps", &["-p", &pid, "-o", "comm="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!path.is_empty()).then_some(path)
}

fn macos_restart_process_command_line(pid: u32) -> Option<String> {
    let pid = pid.to_string();
    let output = hidden_command_with_args("ps", &["-p", &pid, "-o", "command="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn macos_restart_process_has_any_marker(pid: u32, markers: &[&str]) -> bool {
    if markers.is_empty() {
        return false;
    }
    let Some(command_line) = macos_restart_process_command_line(pid) else {
        return false;
    };
    let haystack = command_line.to_ascii_lowercase();
    markers
        .iter()
        .map(|marker| marker.to_ascii_lowercase())
        .any(|marker| haystack.contains(&marker))
}

fn quit_macos_restart_app_by_name(name: &str) {
    let clean_name = macos_restart_process_name(name);
    if clean_name.is_empty() {
        return;
    }
    let script = format!("tell application \"{clean_name}\" to quit");
    let _ = hidden_command_with_args("osascript", &["-e", &script]).output();
}

fn wait_for_macos_restart_process_exit(pids: &[u32], timeout: Duration) {
    let started_at = Instant::now();
    while started_at.elapsed() < timeout {
        if pids.iter().all(|pid| !macos_restart_pid_alive(*pid)) {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn macos_restart_pid_alive(pid: u32) -> bool {
    let pid = pid.to_string();
    hidden_command_with_args("kill", &["-0", &pid])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn macos_restart_process_name(name: &str) -> String {
    name.trim()
        .trim_end_matches(".exe")
        .trim_end_matches(".cmd")
        .trim_end_matches(".bat")
        .trim_end_matches(".ps1")
        .to_string()
}

fn launch_process(program: &str, hidden: bool) -> Result<(), String> {
    if cfg!(target_os = "windows") {
        let window_style = if hidden { "Hidden" } else { "Normal" };
        let script = format!(
            "Start-Process -FilePath {program} -WindowStyle {window_style}",
            program = ps_quote(program),
            window_style = window_style
        );
        return run_powershell(&script).map(|_| ());
    }

    if cfg!(target_os = "macos") && !hidden {
        let path = Path::new(program);
        if path.exists() || program == "Claude" {
            let mut command = hidden_command("open");
            if let Some(app_bundle) = path
                .exists()
                .then(|| macos_app_bundle_for_path(path))
                .flatten()
            {
                command.arg(app_bundle);
            } else if path.exists() {
                command.arg(program);
            } else {
                command.args(["-a", program]);
            }
            return command
                .spawn()
                .map(|_| ())
                .map_err(|err| format!("Failed to start {program}: {err}"));
        }
    }

    let resolved = resolve_command(program).unwrap_or_else(|| program.to_string());
    hidden_command(&resolved)
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("Failed to start {program}: {err}"))
}

fn macos_app_bundle_for_path(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|ancestor| {
            ancestor
                .extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| extension.eq_ignore_ascii_case("app"))
                .unwrap_or(false)
        })
        .map(Path::to_path_buf)
}

fn ps_array(values: &[&str]) -> String {
    if values.is_empty() {
        "@()".to_string()
    } else {
        format!(
            "@({})",
            values
                .iter()
                .map(|value| ps_quote(value))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
