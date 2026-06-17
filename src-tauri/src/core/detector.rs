use crate::core::activity_log;
use crate::core::app_paths::{app_paths, display_path};
use crate::core::codex_client;
use crate::core::env_health;
use crate::core::platform::{hidden_command_with_args, resolve_command};
use crate::core::profile;
use crate::core::tool_registry::{ai_tools, system_tools, ToolDefinition};
use crate::core::types::{
    ConfigState, DetectionSnapshot, DetectionSource, InstallState, Problem, Severity, ToolCategory,
    ToolStatus,
};
use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const VERSION_CHECK_TIMEOUT: Duration = Duration::from_millis(6000);
const UPDATE_CHECK_TIMEOUT: Duration = Duration::from_millis(15000);
const UPDATE_CACHE_TTL: Duration = Duration::from_secs(600);
const NPM_UPDATE_WAIT_BUDGET: Duration = Duration::from_millis(2500);
const WINGET_UPDATE_WAIT_BUDGET: Duration = Duration::from_millis(300);
const BREW_UPDATE_WAIT_BUDGET: Duration = Duration::from_millis(800);
const UPDATE_CACHE_POLL_INTERVAL: Duration = Duration::from_millis(50);
const DETECTION_CACHE_FILE: &str = "detection-cache.json";

#[derive(Debug)]
enum VersionCheck {
    Found(String),
    NotFound(String),
    Failed,
    TimedOut,
}

pub fn load_cached_detection() -> Option<DetectionSnapshot> {
    let path = detection_cache_path()?;
    let text = fs::read_to_string(path).ok()?;
    let mut snapshot = serde_json::from_str::<DetectionSnapshot>(&text).ok()?;
    snapshot.source = DetectionSource::Cached;
    Some(snapshot)
}

pub fn detect_environment() -> Result<DetectionSnapshot, String> {
    profile::ensure_app_dirs()?;
    let paths = app_paths().map_err(|err| err.to_string())?;
    let mut tools = detect_tools(ai_tools_for_environment(resolve_command("code").is_some()));
    tools.push(codex_client::tool_status());
    let mut system = detect_tools(system_tools());
    annotate_update_status(&mut tools, &mut system);
    let profile_summary = profile::load_profile_summary()?;
    let active_profile = profile_summary.active_profile.clone();
    let active_profile_name = profile_summary.active_profile_name.clone();
    let codex_auth = profile_summary.codex_auth.clone();
    let env_conflicts = env_health::claude_env_conflicts_for_active_config(
        &profile_summary.drafts,
        &profile_summary.active_profiles_by_mode.config,
    );
    let mut problems = Vec::new();

    for tool in tools.iter().chain(system.iter()) {
        if tool.install_state == InstallState::Missing {
            problems.push(Problem {
                id: format!("missing-{}", tool.id),
                severity: Severity::Warning,
                title: format!("{} is missing", tool.name),
                detail: tool
                    .install_command
                    .as_ref()
                    .map(|command| format!("Suggested command: {command}"))
                    .unwrap_or_else(|| "Install it before using related workflows.".to_string()),
                action_label: tool.install_command.as_ref().map(|_| "Install".to_string()),
            });
        } else if tool.category == ToolCategory::AiTool
            && tool.config_state == ConfigState::Unconfigured
        {
            problems.push(Problem {
                id: format!("unconfigured-{}", tool.id),
                severity: Severity::Info,
                title: format!("{} is not configured", tool.name),
                detail:
                    "Bootstrap this client to the Local Gateway after creating a Provider Profile."
                        .to_string(),
                action_label: Some("Configure".to_string()),
            });
        }
    }
    for conflict in &env_conflicts {
        problems.push(Problem {
            id: format!(
                "env-conflict-{}-{}-{}",
                conflict.tool_id, conflict.scope, conflict.variable
            ),
            severity: conflict.severity.clone(),
            title: format!("{} 环境变量冲突", conflict.tool_name),
            detail: conflict.message.clone(),
            action_label: Some("清理环境变量".to_string()),
        });
    }

    let _ = activity_log::append(Severity::Ok, "Completed local environment detection.");

    let snapshot = DetectionSnapshot {
        generated_at: Utc::now().to_rfc3339(),
        source: DetectionSource::Live,
        home_dir: display_path(&paths.home_dir),
        app_config_dir: display_path(&paths.config_dir),
        active_profile,
        active_profile_name,
        codex_auth,
        tools,
        system,
        problems,
        env_conflicts,
    };
    store_cached_detection(&snapshot);
    Ok(snapshot)
}

pub fn invalidate_update_cache() {
    {
        let mut cache = npm_update_cache().lock().unwrap();
        cache.packages.clear();
        cache.checked_at = None;
    }
    {
        let mut cache = winget_update_cache().lock().unwrap();
        cache.packages.clear();
        cache.checked_at = None;
    }
    {
        let mut cache = brew_update_cache().lock().unwrap();
        cache.packages.clear();
        cache.checked_at = None;
    }
}

fn detect_tools(definitions: Vec<ToolDefinition>) -> Vec<ToolStatus> {
    definitions
        .into_iter()
        .map(|definition| thread::spawn(move || detect_tool(&definition)))
        .collect::<Vec<_>>()
        .into_iter()
        .filter_map(|handle| handle.join().ok())
        .collect()
}

fn ai_tools_for_environment(vscode_available: bool) -> Vec<ToolDefinition> {
    ai_tools()
        .into_iter()
        .filter(|tool| vscode_available || !is_vscode_extension_tool(tool.id))
        .collect()
}

fn is_vscode_extension_tool(tool_id: &str) -> bool {
    matches!(
        tool_id,
        "codex-vscode" | "claude-vscode" | "gemini-code-assist"
    )
}

fn detection_cache_path() -> Option<PathBuf> {
    app_paths()
        .ok()
        .map(|paths| paths.config_dir.join(DETECTION_CACHE_FILE))
}

fn store_cached_detection(snapshot: &DetectionSnapshot) {
    let Some(path) = detection_cache_path() else {
        return;
    };
    let Ok(text) = serde_json::to_string_pretty(snapshot) else {
        return;
    };
    let _ = fs::write(path, text);
}

fn detect_tool(definition: &ToolDefinition) -> ToolStatus {
    let resolved_command = resolve_command(definition.command);
    let npm_package_version = npm_package_for_tool(definition.id)
        .and_then(read_npm_global_package_version)
        .map(|version| VersionCheck::Found(version));
    let version_check = match (resolved_command.as_ref(), npm_package_version) {
        (Some(_), Some(version_check)) => Some(version_check),
        (Some(_), None) if definition.version_args.is_empty() => {
            Some(VersionCheck::Found("installed".to_string()))
        }
        (Some(command), None) => run_version(
            command,
            definition.version_args,
            definition.version_output_contains,
        ),
        _ => None,
    };
    let version = match &version_check {
        Some(VersionCheck::Found(version)) => Some(version.clone()),
        _ => None,
    };
    let install_state = match (&resolved_command, &version_check) {
        (Some(_), Some(VersionCheck::Found(_))) => InstallState::Installed,
        (Some(_), Some(VersionCheck::NotFound(_))) => InstallState::Missing,
        (Some(_), Some(VersionCheck::Failed | VersionCheck::TimedOut)) | (Some(_), None) => {
            InstallState::Unknown
        }
        _ => InstallState::Missing,
    };
    let config_path = definition
        .config_relative_path
        .and_then(|relative| app_paths().ok().map(|paths| paths.home_dir.join(relative)));
    let config_state = match (&definition.category, &config_path) {
        (ToolCategory::System, _) => ConfigState::NotApplicable,
        (_, Some(path)) if path.exists() => ConfigState::Configured,
        (_, Some(_)) => ConfigState::Unconfigured,
        _ => ConfigState::Unknown,
    };
    let details = match (&resolved_command, &version_check) {
        (Some(command), Some(VersionCheck::Found(_))) => Some(format!("Resolved: {command}")),
        (Some(command), Some(VersionCheck::TimedOut)) => Some(format!(
            "Version check timed out after {}ms: {command}",
            VERSION_CHECK_TIMEOUT.as_millis()
        )),
        (_, Some(VersionCheck::NotFound(detail))) => Some(detail.clone()),
        (Some(command), Some(VersionCheck::Failed)) | (Some(command), None) => {
            Some(format!("Version check failed: {command}"))
        }
        _ => Some("Command not found".to_string()),
    };

    ToolStatus {
        id: definition.id.to_string(),
        name: definition.name.to_string(),
        category: definition.category.clone(),
        command: definition.command.to_string(),
        path_repair: env_health::path_repair_hint(definition),
        version,
        latest_version: None,
        update_available: false,
        update_command: update_command_for_tool(definition.id),
        install_state,
        config_state,
        config_path: config_path.as_deref().map(display_path),
        install_command: definition.install_command.map(ToString::to_string),
        details,
    }
}

#[derive(Debug, Clone)]
struct NpmOutdatedPackage {
    latest: String,
}

#[derive(Debug, Default)]
struct NpmUpdateCache {
    packages: HashMap<String, NpmOutdatedPackage>,
    checked_at: Option<Instant>,
    in_progress: bool,
}

#[derive(Debug, Default)]
struct WingetUpdateCache {
    packages: HashMap<String, String>,
    checked_at: Option<Instant>,
    in_progress: bool,
}

#[derive(Debug, Clone)]
struct BrewOutdatedPackage {
    latest: String,
}

#[derive(Debug, Default)]
struct BrewUpdateCache {
    packages: HashMap<String, BrewOutdatedPackage>,
    checked_at: Option<Instant>,
    in_progress: bool,
}

static NPM_UPDATE_CACHE: OnceLock<Mutex<NpmUpdateCache>> = OnceLock::new();
static WINGET_UPDATE_CACHE: OnceLock<Mutex<WingetUpdateCache>> = OnceLock::new();
static BREW_UPDATE_CACHE: OnceLock<Mutex<BrewUpdateCache>> = OnceLock::new();

fn annotate_update_status(tools: &mut [ToolStatus], system: &mut [ToolStatus]) {
    let npm_outdated = cached_npm_global_outdated(NPM_UPDATE_WAIT_BUDGET);
    let winget_outdated = cached_winget_outdated(WINGET_UPDATE_WAIT_BUDGET);
    let brew_outdated = cached_brew_outdated(BREW_UPDATE_WAIT_BUDGET);
    for tool in tools.iter_mut().chain(system.iter_mut()) {
        tool.update_command = update_command_for_tool(&tool.id);
        if tool.install_state != InstallState::Installed {
            continue;
        }

        if let Some(package) = npm_package_for_tool(&tool.id) {
            if let Some(outdated) = npm_outdated.get(package) {
                tool.latest_version = Some(outdated.latest.clone());
                tool.update_available = true;
            }
        }
        if let Some(package_id) = winget_package_for_tool(&tool.id) {
            if let Some(latest) = winget_outdated.get(package_id) {
                tool.latest_version = Some(latest.clone());
                tool.update_available = true;
            }
        }
        if let Some(package) = brew_package_for_tool(&tool.id) {
            if let Some(outdated) = brew_outdated.get(package) {
                tool.latest_version = Some(outdated.latest.clone());
                tool.update_available = true;
            }
        }
    }
}

fn npm_update_cache() -> &'static Mutex<NpmUpdateCache> {
    NPM_UPDATE_CACHE.get_or_init(|| Mutex::new(NpmUpdateCache::default()))
}

fn winget_update_cache() -> &'static Mutex<WingetUpdateCache> {
    WINGET_UPDATE_CACHE.get_or_init(|| Mutex::new(WingetUpdateCache::default()))
}

fn brew_update_cache() -> &'static Mutex<BrewUpdateCache> {
    BREW_UPDATE_CACHE.get_or_init(|| Mutex::new(BrewUpdateCache::default()))
}

fn cached_npm_global_outdated(wait_budget: Duration) -> HashMap<String, NpmOutdatedPackage> {
    let should_start = {
        let mut cache = npm_update_cache().lock().unwrap();
        if cache
            .checked_at
            .map(|checked_at| checked_at.elapsed() < UPDATE_CACHE_TTL)
            .unwrap_or(false)
        {
            return cache.packages.clone();
        }

        if cache.in_progress {
            false
        } else {
            cache.in_progress = true;
            true
        }
    };

    if should_start {
        thread::spawn(|| {
            let packages = read_npm_global_outdated();
            let mut cache = npm_update_cache().lock().unwrap();
            cache.packages = packages;
            cache.checked_at = Some(Instant::now());
            cache.in_progress = false;
        });
    }

    wait_for_npm_update_cache(wait_budget)
}

fn cached_winget_outdated(wait_budget: Duration) -> HashMap<String, String> {
    if !cfg!(target_os = "windows") {
        return HashMap::new();
    }
    let should_start = {
        let mut cache = winget_update_cache().lock().unwrap();
        if cache
            .checked_at
            .map(|checked_at| checked_at.elapsed() < UPDATE_CACHE_TTL)
            .unwrap_or(false)
        {
            return cache.packages.clone();
        }

        if cache.in_progress {
            false
        } else {
            cache.in_progress = true;
            true
        }
    };

    if should_start {
        thread::spawn(|| {
            let packages = read_winget_outdated();
            let mut cache = winget_update_cache().lock().unwrap();
            cache.packages = packages;
            cache.checked_at = Some(Instant::now());
            cache.in_progress = false;
        });
    }

    wait_for_winget_update_cache(wait_budget)
}

fn cached_brew_outdated(wait_budget: Duration) -> HashMap<String, BrewOutdatedPackage> {
    if !cfg!(target_os = "macos") {
        return HashMap::new();
    }
    let should_start = {
        let mut cache = brew_update_cache().lock().unwrap();
        if cache
            .checked_at
            .map(|checked_at| checked_at.elapsed() < UPDATE_CACHE_TTL)
            .unwrap_or(false)
        {
            return cache.packages.clone();
        }

        if cache.in_progress {
            false
        } else {
            cache.in_progress = true;
            true
        }
    };

    if should_start {
        thread::spawn(|| {
            let packages = read_brew_outdated();
            let mut cache = brew_update_cache().lock().unwrap();
            cache.packages = packages;
            cache.checked_at = Some(Instant::now());
            cache.in_progress = false;
        });
    }

    wait_for_brew_update_cache(wait_budget)
}

fn wait_for_npm_update_cache(wait_budget: Duration) -> HashMap<String, NpmOutdatedPackage> {
    let started_at = Instant::now();
    loop {
        {
            let cache = npm_update_cache().lock().unwrap();
            if !cache.in_progress
                || cache
                    .checked_at
                    .map(|checked_at| checked_at.elapsed() < UPDATE_CACHE_TTL)
                    .unwrap_or(false)
            {
                return cache.packages.clone();
            }
            if started_at.elapsed() >= wait_budget {
                return cache.packages.clone();
            }
        }
        thread::sleep(UPDATE_CACHE_POLL_INTERVAL);
    }
}

fn wait_for_winget_update_cache(wait_budget: Duration) -> HashMap<String, String> {
    let started_at = Instant::now();
    loop {
        {
            let cache = winget_update_cache().lock().unwrap();
            if !cache.in_progress
                || cache
                    .checked_at
                    .map(|checked_at| checked_at.elapsed() < UPDATE_CACHE_TTL)
                    .unwrap_or(false)
            {
                return cache.packages.clone();
            }
            if started_at.elapsed() >= wait_budget {
                return cache.packages.clone();
            }
        }
        thread::sleep(UPDATE_CACHE_POLL_INTERVAL);
    }
}

fn wait_for_brew_update_cache(wait_budget: Duration) -> HashMap<String, BrewOutdatedPackage> {
    let started_at = Instant::now();
    loop {
        {
            let cache = brew_update_cache().lock().unwrap();
            if !cache.in_progress
                || cache
                    .checked_at
                    .map(|checked_at| checked_at.elapsed() < UPDATE_CACHE_TTL)
                    .unwrap_or(false)
            {
                return cache.packages.clone();
            }
            if started_at.elapsed() >= wait_budget {
                return cache.packages.clone();
            }
        }
        thread::sleep(UPDATE_CACHE_POLL_INTERVAL);
    }
}

fn npm_package_for_tool(tool_id: &str) -> Option<&'static str> {
    match tool_id {
        "codex" => Some("@openai/codex"),
        "claude" => Some("@anthropic-ai/claude-code"),
        "gemini" => Some("@google/gemini-cli"),
        "opencode" => Some("opencode-ai"),
        "openclaw" => Some("openclaw"),
        "pnpm" => Some("pnpm"),
        "npm" => Some("npm"),
        _ => None,
    }
}

fn winget_package_for_tool(tool_id: &str) -> Option<&'static str> {
    if !cfg!(target_os = "windows") {
        return None;
    }
    match tool_id {
        "claude-desktop" => Some("Anthropic.Claude"),
        "node" => Some("OpenJS.NodeJS.LTS"),
        "git" => Some("Git.Git"),
        "bun" => Some("Oven-sh.Bun"),
        _ => None,
    }
}

fn brew_package_for_tool(tool_id: &str) -> Option<&'static str> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    match tool_id {
        "claude-desktop" => Some("claude"),
        "hermes" => Some("hermes-agent"),
        "node" => Some("node"),
        "git" => Some("git"),
        "bun" => Some("bun"),
        _ => None,
    }
}

fn update_command_for_tool(tool_id: &str) -> Option<String> {
    match tool_id {
        "codex" => Some("npm install -g @openai/codex@latest".to_string()),
        "codex-vscode" => Some("code --install-extension openai.chatgpt --force".to_string()),
        "claude" => Some("npm install -g @anthropic-ai/claude-code@latest".to_string()),
        "claude-desktop" if cfg!(target_os = "macos") => {
            Some("brew upgrade --cask claude".to_string())
        }
        "claude-desktop" => Some(
            "winget upgrade --id Anthropic.Claude --exact --accept-source-agreements --accept-package-agreements --disable-interactivity"
                .to_string(),
        ),
        "claude-vscode" => {
            Some("code --install-extension anthropic.claude-code --force".to_string())
        }
        "gemini" => Some("npm install -g @google/gemini-cli@latest".to_string()),
        "gemini-code-assist" => {
            Some("code --install-extension Google.geminicodeassist --force".to_string())
        }
        "opencode" => Some("npm install -g opencode-ai@latest".to_string()),
        "openclaw" => Some("npm install -g openclaw@latest".to_string()),
        "hermes" if cfg!(target_os = "macos") => {
            Some("brew upgrade hermes-agent".to_string())
        }
        "hermes" => Some(
            "powershell -NoProfile -ExecutionPolicy Bypass -Command \"iex (irm https://hermes-agent.nousresearch.com/install.ps1)\""
                .to_string(),
        ),
        "node" if cfg!(target_os = "macos") => Some("brew upgrade node".to_string()),
        "node" => Some(
            "winget upgrade --id OpenJS.NodeJS.LTS --exact --accept-source-agreements --accept-package-agreements --disable-interactivity"
                .to_string(),
        ),
        "git" if cfg!(target_os = "macos") => Some("brew upgrade git".to_string()),
        "git" => Some(
            "winget upgrade --id Git.Git --exact --accept-source-agreements --accept-package-agreements --disable-interactivity"
                .to_string(),
        ),
        "pnpm" => Some("npm install -g pnpm@latest".to_string()),
        "bun" if cfg!(target_os = "macos") => Some("brew upgrade bun".to_string()),
        "bun" => Some(
            "winget upgrade --id Oven-sh.Bun --exact --accept-source-agreements --accept-package-agreements --disable-interactivity"
                .to_string(),
        ),
        _ => None,
    }
}

fn read_npm_global_outdated() -> HashMap<String, NpmOutdatedPackage> {
    let Some(npm) = resolve_command("npm") else {
        return HashMap::new();
    };
    let Some(output) = run_command_with_timeout(
        &npm,
        &["outdated", "-g", "--json", "--depth=0"],
        UPDATE_CHECK_TIMEOUT,
    ) else {
        return HashMap::new();
    };
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        return HashMap::new();
    }

    let Ok(Value::Object(packages)) = serde_json::from_str::<Value>(&stdout) else {
        return HashMap::new();
    };

    packages
        .into_iter()
        .filter_map(|(package, value)| {
            let latest = value
                .get("latest")
                .or_else(|| value.get("wanted"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|latest| !latest.is_empty())?
                .to_string();
            Some((package, NpmOutdatedPackage { latest }))
        })
        .collect()
}

fn read_npm_global_package_version(package: &str) -> Option<String> {
    npm_global_package_roots()
        .into_iter()
        .filter_map(|root| read_npm_package_version(&root, package))
        .next()
}

fn read_npm_package_version(root: &PathBuf, package: &str) -> Option<String> {
    let manifest = package
        .split('/')
        .fold(root.clone(), |path, segment| path.join(segment))
        .join("package.json");
    let text = fs::read_to_string(manifest).ok()?;
    let value = serde_json::from_str::<Value>(&text).ok()?;
    value
        .get("version")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .map(ToString::to_string)
}

fn npm_global_package_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if cfg!(windows) {
        if let Some(app_data) = env::var_os("APPDATA") {
            roots.push(PathBuf::from(app_data).join("npm").join("node_modules"));
        }
        if let Ok(paths) = app_paths() {
            roots.push(
                paths
                    .home_dir
                    .join("AppData")
                    .join("Roaming")
                    .join("npm")
                    .join("node_modules"),
            );
        }
    }
    if let Some(prefix) = env::var_os("NPM_CONFIG_PREFIX") {
        roots.push(PathBuf::from(prefix).join("node_modules"));
    }
    if let Some(npm) = resolve_command("npm") {
        if let Some(root) = npm_global_root_from_command(&npm) {
            roots.push(root);
        }
        let npm_path = PathBuf::from(npm);
        if let Some(parent) = npm_path.parent() {
            roots.push(parent.join("node_modules"));
        }
    }
    if cfg!(target_os = "macos") {
        roots.push(PathBuf::from("/opt/homebrew/lib/node_modules"));
        roots.push(PathBuf::from("/usr/local/lib/node_modules"));
        if let Ok(paths) = app_paths() {
            roots.push(
                paths
                    .home_dir
                    .join(".npm-global")
                    .join("lib")
                    .join("node_modules"),
            );
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

fn npm_global_root_from_command(npm: &str) -> Option<PathBuf> {
    let output = run_command_with_timeout(npm, &["root", "-g"], Duration::from_millis(1200))?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        None
    } else {
        Some(PathBuf::from(stdout))
    }
}

fn read_winget_outdated() -> HashMap<String, String> {
    let Some(winget) = resolve_command("winget") else {
        return HashMap::new();
    };
    let Some(output) = run_command_with_timeout(
        &winget,
        &["upgrade", "--source", "winget", "--disable-interactivity"],
        UPDATE_CHECK_TIMEOUT,
    ) else {
        return HashMap::new();
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let package_ids = [
        "Anthropic.Claude",
        "OpenJS.NodeJS.LTS",
        "Git.Git",
        "Oven-sh.Bun",
    ];

    stdout
        .lines()
        .filter_map(|line| {
            let tokens = line.split_whitespace().collect::<Vec<_>>();
            let package_id = package_ids
                .iter()
                .find(|package_id| tokens.iter().any(|token| *token == **package_id))?;
            let index = tokens.iter().position(|token| *token == *package_id)?;
            let latest = tokens.get(index + 2)?;
            Some(((*package_id).to_string(), (*latest).to_string()))
        })
        .collect()
}

fn read_brew_outdated() -> HashMap<String, BrewOutdatedPackage> {
    let Some(brew) = resolve_command("brew") else {
        return HashMap::new();
    };
    let Some(output) =
        run_command_with_timeout(&brew, &["outdated", "--json=v2"], UPDATE_CHECK_TIMEOUT)
    else {
        return HashMap::new();
    };
    if !output.status.success() {
        return HashMap::new();
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        return HashMap::new();
    }
    let Ok(value) = serde_json::from_str::<Value>(&stdout) else {
        return HashMap::new();
    };

    let mut packages = HashMap::new();
    collect_brew_outdated_items(value.get("formulae"), &mut packages);
    collect_brew_outdated_items(value.get("casks"), &mut packages);
    packages
}

fn collect_brew_outdated_items(
    items: Option<&Value>,
    packages: &mut HashMap<String, BrewOutdatedPackage>,
) {
    let Some(Value::Array(items)) = items else {
        return;
    };
    for item in items {
        let Some(name) = item.get("name").and_then(Value::as_str) else {
            continue;
        };
        let latest = item
            .get("current_version")
            .and_then(Value::as_str)
            .or_else(|| {
                item.get("current_versions")
                    .and_then(Value::as_array)
                    .and_then(|versions| versions.first())
                    .and_then(Value::as_str)
            })
            .map(str::trim)
            .filter(|latest| !latest.is_empty())
            .unwrap_or("latest")
            .to_string();
        packages.insert(name.to_string(), BrewOutdatedPackage { latest });
    }
}

fn run_command_with_timeout(
    command: &str,
    args: &[&str],
    timeout: Duration,
) -> Option<std::process::Output> {
    let mut command_builder = hidden_command_with_args(command, args);
    let mut child = command_builder.spawn().ok()?;
    let started_at = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) if started_at.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

fn run_version(
    command: &str,
    args: &[&str],
    output_contains: Option<&str>,
) -> Option<VersionCheck> {
    let mut command_builder = hidden_command_with_args(command, args);
    let mut child = command_builder.spawn().ok()?;
    let started_at = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                let output = child.wait_with_output().ok()?;
                if !output.status.success() {
                    return Some(VersionCheck::Failed);
                }

                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let output_text = if !stdout.is_empty() { &stdout } else { &stderr };

                if let Some(needle) = output_contains {
                    let needle_lower = needle.to_ascii_lowercase();
                    return Some(
                        output_text
                            .lines()
                            .find(|line| line.to_ascii_lowercase().contains(&needle_lower))
                            .map(|line| VersionCheck::Found(line.trim().to_string()))
                            .unwrap_or_else(|| {
                                VersionCheck::NotFound(format!(
                                    "Required marker not found in command output: {needle}"
                                ))
                            }),
                    );
                }

                return Some(if !stdout.is_empty() {
                    VersionCheck::Found(stdout.lines().next().unwrap_or_default().to_string())
                } else if !stderr.is_empty() {
                    VersionCheck::Found(stderr.lines().next().unwrap_or_default().to_string())
                } else {
                    VersionCheck::Found("installed".to_string())
                });
            }
            Ok(None) if started_at.elapsed() >= VERSION_CHECK_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Some(VersionCheck::TimedOut);
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Some(VersionCheck::Failed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vscode_extension_tools_are_hidden_without_vscode() {
        let tools = ai_tools_for_environment(false);
        assert!(!tools.iter().any(|tool| tool.id == "codex-vscode"));
        assert!(!tools.iter().any(|tool| tool.id == "claude-vscode"));
        assert!(!tools.iter().any(|tool| tool.id == "gemini-code-assist"));
        assert!(tools.iter().any(|tool| tool.id == "codex"));
        assert!(tools.iter().any(|tool| tool.id == "claude"));
    }

    #[test]
    fn vscode_extension_tools_are_visible_with_vscode() {
        let tools = ai_tools_for_environment(true);
        assert!(tools.iter().any(|tool| tool.id == "codex-vscode"));
        assert!(tools.iter().any(|tool| tool.id == "claude-vscode"));
        assert!(tools.iter().any(|tool| tool.id == "gemini-code-assist"));
    }

    #[test]
    fn parses_homebrew_outdated_formulae_and_casks() {
        let value: Value = serde_json::from_str(
            r#"{
              "formulae": [{ "name": "node", "current_version": "24.1.0" }],
              "casks": [{ "name": "claude", "current_version": "0.12.0" }]
            }"#,
        )
        .expect("json");
        let mut packages = HashMap::new();

        collect_brew_outdated_items(value.get("formulae"), &mut packages);
        collect_brew_outdated_items(value.get("casks"), &mut packages);

        assert_eq!(
            packages.get("node").map(|item| item.latest.as_str()),
            Some("24.1.0")
        );
        assert_eq!(
            packages.get("claude").map(|item| item.latest.as_str()),
            Some("0.12.0")
        );
    }
}
