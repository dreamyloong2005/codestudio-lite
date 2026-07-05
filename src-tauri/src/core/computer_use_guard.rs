#![cfg_attr(not(windows), allow(dead_code))]

use std::path::{Path, PathBuf};

use toml_edit::{Array, DocumentMut, Item, Table};

const BUNDLED_MARKETPLACE: &str = "openai-bundled";
const BUNDLED_MARKETPLACE_PLUGINS: &[&str] = &["browser", "chrome", "computer-use", "latex"];
const COMPUTER_USE_PLUGINS: &[&str] = &[
    "browser@openai-bundled",
    "chrome@openai-bundled",
    "computer-use@openai-bundled",
];
const COMPUTER_USE_EXE: &str = "codex-computer-use.exe";
const COMPUTER_USE_CLIENT_SCRIPT: &str = "computer-use-client.mjs";
const SKY_INTERNAL_COMPUTER_USE_CLIENT_EXPORT: &str =
    "./dist/project/cua/sky_js/src/targets/windows/internal/computer_use_client_base.js";
const SKY_INTERNAL_COMPUTER_USE_CLIENT_IMPORT: &str =
    "@oai/sky/dist/project/cua/sky_js/src/targets/windows/internal/computer_use_client_base.js";
const SKY_PACKAGE_EXPORTS_BACKUP: &str = "package.json.bak-codestudio-runtime-exports";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardResult {
    pub changed: bool,
    pub notify_exe: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardArtifacts {
    pub notify_exe: Option<PathBuf>,
    pub marketplace_path: Option<PathBuf>,
    pub sky_package_json: Option<PathBuf>,
    pub runtime_exports_needed: bool,
}

pub fn resolve_computer_use_guard_artifacts(home: &Path) -> Result<GuardArtifacts, String> {
    #[cfg(windows)]
    {
        let notify_exe = find_computer_use_notify_exe(home);
        let runtime_exports_needed = computer_use_client_needs_sky_internal_export(home)?;
        Ok(GuardArtifacts {
            sky_package_json: find_sky_package_json_for_notify_exe(notify_exe.as_deref())
                .or_else(find_latest_sky_package_json),
            notify_exe,
            marketplace_path: ensure_openai_bundled_marketplace(home)?,
            runtime_exports_needed,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = home;
        Ok(GuardArtifacts {
            notify_exe: None,
            marketplace_path: None,
            sky_package_json: None,
            runtime_exports_needed: false,
        })
    }
}

pub fn ensure_computer_use_config_with_artifacts(
    home: &Path,
    artifacts: &GuardArtifacts,
) -> Result<GuardResult, String> {
    #[cfg(windows)]
    {
        ensure_computer_use_config_with_artifacts_windows(home, artifacts)
    }
    #[cfg(not(windows))]
    {
        let _ = (home, artifacts);
        Ok(GuardResult {
            changed: false,
            notify_exe: None,
        })
    }
}

#[cfg(windows)]
fn ensure_computer_use_config_with_artifacts_windows(
    home: &Path,
    artifacts: &GuardArtifacts,
) -> Result<GuardResult, String> {
    std::fs::create_dir_all(home)
        .map_err(|err| format!("Failed to create Codex config directory: {err}"))?;
    let config_path = home.join("config.toml");
    let existing = match std::fs::read(&config_path) {
        Ok(bytes) => String::from_utf8(bytes)
            .map_err(|err| format!("Codex config.toml is not valid UTF-8: {err}"))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("Failed to read {}: {error}", config_path.display())),
    };
    let updated = if let Some(marketplace_path) = artifacts.marketplace_path.as_deref() {
        guard_config_text_with_marketplace(
            &existing,
            artifacts.notify_exe.as_deref(),
            Some(marketplace_path),
        )?
    } else {
        guard_config_text(&existing, artifacts.notify_exe.as_deref())?
    };
    let config_changed = updated.as_bytes() != existing.as_bytes();
    if config_changed {
        atomic_write_file(&config_path, updated.as_bytes())?;
    }
    let runtime_compat = ensure_computer_use_runtime_exports_compat_windows(
        home,
        artifacts.sky_package_json.as_deref(),
    )?;
    Ok(GuardResult {
        changed: config_changed || runtime_compat.changed,
        notify_exe: artifacts.notify_exe.clone(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCompatResult {
    pub changed: bool,
    pub package_json: Option<PathBuf>,
    pub backup_path: Option<PathBuf>,
}

#[cfg(not(windows))]
pub fn ensure_computer_use_runtime_exports_compat(
    home: &Path,
) -> Result<RuntimeCompatResult, String> {
    let _ = home;
    Ok(RuntimeCompatResult {
        changed: false,
        package_json: None,
        backup_path: None,
    })
}

#[cfg(windows)]
#[allow(dead_code)]
pub fn ensure_computer_use_runtime_exports_compat(
    home: &Path,
) -> Result<RuntimeCompatResult, String> {
    ensure_computer_use_runtime_exports_compat_windows(
        home,
        find_latest_sky_package_json().as_deref(),
    )
}

#[cfg(windows)]
fn ensure_computer_use_runtime_exports_compat_windows(
    home: &Path,
    sky_package_json: Option<&Path>,
) -> Result<RuntimeCompatResult, String> {
    if !computer_use_client_needs_sky_internal_export(home)? {
        return Ok(RuntimeCompatResult {
            changed: false,
            package_json: sky_package_json.map(Path::to_path_buf),
            backup_path: None,
        });
    }
    let Some(package_json) = sky_package_json else {
        return Ok(RuntimeCompatResult {
            changed: false,
            package_json: None,
            backup_path: None,
        });
    };
    if !sky_internal_computer_use_client_file_exists(package_json) {
        return Ok(RuntimeCompatResult {
            changed: false,
            package_json: Some(package_json.to_path_buf()),
            backup_path: None,
        });
    }

    let existing = std::fs::read_to_string(package_json)
        .map_err(|err| format!("Failed to read {}: {err}", package_json.display()))?;
    let Some(updated) = add_sky_internal_computer_use_export(&existing)? else {
        return Ok(RuntimeCompatResult {
            changed: false,
            package_json: Some(package_json.to_path_buf()),
            backup_path: None,
        });
    };

    let backup_path = package_json
        .parent()
        .ok_or_else(|| "Invalid @oai/sky package.json path.".to_string())?
        .join(SKY_PACKAGE_EXPORTS_BACKUP);
    if !backup_path.exists() {
        std::fs::copy(package_json, &backup_path).map_err(|err| {
            format!(
                "Failed to back up {} to {}: {err}",
                package_json.display(),
                backup_path.display()
            )
        })?;
    }
    atomic_write_file(package_json, updated.as_bytes())?;
    Ok(RuntimeCompatResult {
        changed: true,
        package_json: Some(package_json.to_path_buf()),
        backup_path: Some(backup_path),
    })
}

pub(crate) fn guard_config_text(
    config_text: &str,
    notify_exe: Option<&Path>,
) -> Result<String, String> {
    guard_config_text_with_marketplace(config_text, notify_exe, None)
}

pub(crate) fn guard_config_text_with_marketplace(
    config_text: &str,
    notify_exe: Option<&Path>,
    marketplace_path: Option<&Path>,
) -> Result<String, String> {
    let without_bom = config_text.trim_start_matches('\u{feff}');
    let mut doc = parse_toml_document(without_bom)?;

    let features = table_mut_or_insert(&mut doc, "features")?;
    features["js_repl"] = toml_edit::value(true);

    for plugin_id in COMPUTER_USE_PLUGINS {
        ensure_plugin_enabled(&mut doc, plugin_id)?;
    }

    if let Some(notify_exe) = notify_exe {
        let mut notify = Array::default();
        notify.push(notify_exe.to_string_lossy().as_ref());
        notify.push("turn-ended");
        doc["notify"] = toml_edit::value(notify);
    }

    if let Some(marketplace_path) = marketplace_path {
        ensure_openai_bundled_marketplace_config(&mut doc, marketplace_path)?;
    }

    Ok(ensure_trailing_newline(doc.to_string()))
}

pub fn find_computer_use_notify_exe(home: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        find_computer_use_notify_exe_windows(home)
    }
    #[cfg(not(windows))]
    {
        let _ = home;
        None
    }
}

#[cfg(windows)]
fn find_computer_use_notify_exe_windows(home: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        collect_named_files(
            &PathBuf::from(local_app_data)
                .join("OpenAI")
                .join("Codex")
                .join("runtimes")
                .join("cua_node"),
            COMPUTER_USE_EXE,
            12,
            &mut candidates,
        );
    }
    if candidates.is_empty() {
        collect_named_files(
            &home
                .join("plugins")
                .join("cache")
                .join(BUNDLED_MARKETPLACE)
                .join("computer-use"),
            COMPUTER_USE_EXE,
            12,
            &mut candidates,
        );
    }
    candidates.sort_by(|left, right| {
        modified_millis(right)
            .cmp(&modified_millis(left))
            .then_with(|| left.cmp(right))
    });
    candidates.into_iter().next()
}

#[cfg(windows)]
fn collect_named_files(root: &Path, file_name: &str, depth: usize, output: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case(file_name))
            {
                output.push(path);
            }
        } else if path.is_dir() {
            collect_named_files(&path, file_name, depth - 1, output);
        }
    }
}

#[cfg(windows)]
fn modified_millis(path: &Path) -> u128 {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(windows)]
fn computer_use_client_needs_sky_internal_export(home: &Path) -> Result<bool, String> {
    let mut candidates = Vec::new();
    collect_named_files(
        &home
            .join("plugins")
            .join("cache")
            .join(BUNDLED_MARKETPLACE)
            .join("computer-use"),
        COMPUTER_USE_CLIENT_SCRIPT,
        8,
        &mut candidates,
    );
    candidates.sort_by(|left, right| {
        modified_millis(right)
            .cmp(&modified_millis(left))
            .then_with(|| left.cmp(right))
    });
    for candidate in candidates {
        let contents = std::fs::read_to_string(&candidate)
            .map_err(|err| format!("Failed to read {}: {err}", candidate.display()))?;
        if contents.contains(SKY_INTERNAL_COMPUTER_USE_CLIENT_IMPORT) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(windows)]
fn find_sky_package_json_for_notify_exe(notify_exe: Option<&Path>) -> Option<PathBuf> {
    let notify_exe = notify_exe?;
    for ancestor in notify_exe.ancestors() {
        if ancestor
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("sky"))
            && ancestor
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("@oai"))
        {
            let package_json = ancestor.join("package.json");
            if package_json.is_file() {
                return Some(package_json);
            }
        }
    }
    None
}

#[cfg(windows)]
fn find_latest_sky_package_json() -> Option<PathBuf> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")?;
    let runtimes = PathBuf::from(local_app_data)
        .join("OpenAI")
        .join("Codex")
        .join("runtimes")
        .join("cua_node");
    let Ok(entries) = std::fs::read_dir(runtimes) else {
        return None;
    };
    let mut candidates: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| {
            entry
                .path()
                .join("bin")
                .join("node_modules")
                .join("@oai")
                .join("sky")
                .join("package.json")
        })
        .filter(|path| path.is_file())
        .collect();
    candidates.sort_by(|left, right| {
        modified_millis(right)
            .cmp(&modified_millis(left))
            .then_with(|| left.cmp(right))
    });
    candidates.into_iter().next()
}

#[cfg(windows)]
fn sky_internal_computer_use_client_file_exists(package_json: &Path) -> bool {
    let Some(package_root) = package_json.parent() else {
        return false;
    };
    package_root
        .join(SKY_INTERNAL_COMPUTER_USE_CLIENT_EXPORT.trim_start_matches("./"))
        .is_file()
}

fn add_sky_internal_computer_use_export(contents: &str) -> Result<Option<String>, String> {
    let mut package: serde_json::Value = serde_json::from_str(contents)
        .map_err(|err| format!("@oai/sky package.json parse failed: {err}"))?;
    let Some(exports) = package
        .get_mut("exports")
        .and_then(|value| value.as_object_mut())
    else {
        return Ok(None);
    };
    if exports.contains_key(SKY_INTERNAL_COMPUTER_USE_CLIENT_EXPORT) {
        return Ok(None);
    }
    exports.insert(
        SKY_INTERNAL_COMPUTER_USE_CLIENT_EXPORT.to_string(),
        serde_json::Value::String(SKY_INTERNAL_COMPUTER_USE_CLIENT_EXPORT.to_string()),
    );
    let mut updated = serde_json::to_string_pretty(&package).map_err(|err| err.to_string())?;
    updated.push('\n');
    Ok(Some(updated))
}

#[cfg(windows)]
pub fn ensure_openai_bundled_marketplace(home: &Path) -> Result<Option<PathBuf>, String> {
    let active = home
        .join(".tmp")
        .join("bundled-marketplaces")
        .join(BUNDLED_MARKETPLACE);
    if is_complete_openai_bundled_marketplace(&active) {
        return Ok(Some(active));
    }
    if let Some(configured) = configured_openai_bundled_marketplace(home) {
        if is_complete_openai_bundled_marketplace(&configured) {
            return Ok(Some(configured));
        }
    }

    let parent = active
        .parent()
        .ok_or_else(|| "Invalid bundled marketplace path.".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|err| format!("Failed to create bundled marketplace directory: {err}"))?;

    let staging = parent.join(format!(
        "{BUNDLED_MARKETPLACE}.guard-staging-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)
            .map_err(|err| format!("Failed to clear stale marketplace staging dir: {err}"))?;
    }

    if let Some(source) = find_complete_openai_bundled_marketplace(parent, &active) {
        copy_dir_recursive(&source, &staging)?;
    } else if can_build_marketplace_from_cache(home) {
        build_marketplace_from_cache(home, &staging)?;
    } else {
        return Ok(None);
    }

    match replace_active_marketplace(&active, &staging) {
        Ok(()) => Ok(Some(active)),
        Err(_) if is_complete_openai_bundled_marketplace(&staging) => Ok(Some(staging)),
        Err(error) => Err(format!(
            "Failed to replace active bundled marketplace at {}: {error}",
            active.display()
        )),
    }
}

#[cfg(windows)]
fn configured_openai_bundled_marketplace(home: &Path) -> Option<PathBuf> {
    let config = std::fs::read_to_string(home.join("config.toml")).ok()?;
    let without_bom = config.trim_start_matches('\u{feff}');
    let doc = parse_toml_document(without_bom).ok()?;
    let source = doc
        .get("marketplaces")?
        .as_table()?
        .get(BUNDLED_MARKETPLACE)?
        .as_table()?
        .get("source")?
        .as_str()?;
    Some(path_from_configured_marketplace_source(source))
}

#[cfg(windows)]
fn path_from_configured_marketplace_source(source: &str) -> PathBuf {
    source
        .strip_prefix(r"\\?\")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(source))
}

#[cfg(windows)]
fn is_complete_openai_bundled_marketplace(path: &Path) -> bool {
    if !path
        .join(".agents")
        .join("plugins")
        .join("marketplace.json")
        .is_file()
    {
        return false;
    }
    BUNDLED_MARKETPLACE_PLUGINS.iter().all(|plugin| {
        path.join("plugins")
            .join(plugin)
            .join(".codex-plugin")
            .join("plugin.json")
            .is_file()
    })
}

#[cfg(windows)]
fn find_complete_openai_bundled_marketplace(parent: &Path, active: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    let Ok(entries) = std::fs::read_dir(parent) else {
        return None;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == active || !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.starts_with(BUNDLED_MARKETPLACE) && is_complete_openai_bundled_marketplace(&path) {
            candidates.push(path);
        }
    }
    candidates.sort_by(|left, right| {
        modified_millis(right)
            .cmp(&modified_millis(left))
            .then_with(|| left.cmp(right))
    });
    candidates.into_iter().next()
}

#[cfg(windows)]
fn cache_plugin_root(home: &Path, plugin: &str) -> PathBuf {
    home.join("plugins")
        .join("cache")
        .join(BUNDLED_MARKETPLACE)
        .join(plugin)
}

#[cfg(windows)]
fn can_build_marketplace_from_cache(home: &Path) -> bool {
    BUNDLED_MARKETPLACE_PLUGINS
        .iter()
        .all(|plugin| latest_cache_plugin_version(home, plugin).is_some())
}

#[cfg(windows)]
fn latest_cache_plugin_version(home: &Path, plugin: &str) -> Option<PathBuf> {
    let root = cache_plugin_root(home, plugin);
    let mut candidates = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return None;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.join(".codex-plugin").join("plugin.json").is_file() {
            candidates.push(path);
        }
    }
    candidates.sort_by(|left, right| {
        let left_name = left
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let right_name = right
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        right_name
            .cmp(left_name)
            .then_with(|| modified_millis(right).cmp(&modified_millis(left)))
    });
    candidates.into_iter().next()
}

#[cfg(windows)]
fn build_marketplace_from_cache(home: &Path, staging: &Path) -> Result<(), String> {
    let plugins_dir = staging.join("plugins");
    std::fs::create_dir_all(staging.join(".agents").join("plugins"))
        .map_err(|err| format!("Failed to create marketplace metadata dir: {err}"))?;
    std::fs::create_dir_all(&plugins_dir)
        .map_err(|err| format!("Failed to create marketplace plugins dir: {err}"))?;
    std::fs::write(
        staging
            .join(".agents")
            .join("plugins")
            .join("marketplace.json"),
        bundled_marketplace_json().as_bytes(),
    )
    .map_err(|err| format!("Failed to write bundled marketplace metadata: {err}"))?;
    for plugin in BUNDLED_MARKETPLACE_PLUGINS {
        let Some(source) = latest_cache_plugin_version(home, plugin) else {
            return Err(format!(
                "Missing cached {plugin} plugin for openai-bundled marketplace."
            ));
        };
        copy_dir_recursive(&source, &plugins_dir.join(plugin))?;
    }
    Ok(())
}

#[cfg(windows)]
fn bundled_marketplace_json() -> String {
    let plugins = [
        ("browser", "Engineering"),
        ("chrome", "Productivity"),
        ("computer-use", "Productivity"),
        ("latex", "Research"),
    ]
    .into_iter()
    .map(|(name, category)| {
        serde_json::json!({
            "name": name,
            "source": {
                "source": "local",
                "path": format!("./plugins/{name}")
            },
            "policy": {
                "installation": "AVAILABLE",
                "authentication": "ON_INSTALL"
            },
            "category": category
        })
    })
    .collect::<Vec<_>>();
    serde_json::to_string_pretty(&serde_json::json!({
        "name": BUNDLED_MARKETPLACE,
        "interface": {
            "displayName": "OpenAI Bundled"
        },
        "plugins": plugins
    }))
    .expect("bundled marketplace JSON should serialize")
}

#[cfg(windows)]
fn replace_active_marketplace(active: &Path, staging: &Path) -> Result<(), String> {
    if active.exists() {
        let backup = active.with_file_name(format!(
            "{BUNDLED_MARKETPLACE}.bak-guard-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));
        std::fs::rename(active, backup)
            .map_err(|err| format!("Failed to back up active bundled marketplace: {err}"))?;
    }
    std::fs::rename(staging, active)
        .map_err(|err| format!("Failed to activate bundled marketplace: {err}"))
}

#[cfg(windows)]
fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    std::fs::create_dir_all(destination)
        .map_err(|err| format!("Failed to create {}: {err}", destination.display()))?;
    for entry in std::fs::read_dir(source)
        .map_err(|err| format!("Failed to read {}: {err}", source.display()))?
    {
        let entry = entry.map_err(|err| err.to_string())?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &destination_path)?;
        } else {
            std::fs::copy(&source_path, &destination_path).map_err(|err| {
                format!(
                    "Failed to copy {} to {}: {err}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn ensure_openai_bundled_marketplace_config(
    doc: &mut DocumentMut,
    marketplace_path: &Path,
) -> Result<(), String> {
    let marketplaces = table_mut_or_insert(doc, "marketplaces")?;
    if marketplaces
        .get(BUNDLED_MARKETPLACE)
        .and_then(Item::as_table)
        .is_none()
    {
        marketplaces[BUNDLED_MARKETPLACE] = toml_edit::table();
    }
    marketplaces[BUNDLED_MARKETPLACE]["source_type"] = toml_edit::value("local");
    marketplaces[BUNDLED_MARKETPLACE]["source"] =
        toml_edit::value(windows_extended_path(marketplace_path));
    Ok(())
}

fn windows_extended_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if value.starts_with(r"\\?\") {
        value.into_owned()
    } else {
        format!(r"\\?\{value}")
    }
}

fn parse_toml_document(contents: &str) -> Result<DocumentMut, String> {
    if contents.trim().is_empty() {
        Ok(DocumentMut::new())
    } else {
        contents
            .parse::<DocumentMut>()
            .map_err(|err| format!("config.toml TOML parse failed: {err}"))
    }
}

fn table_mut_or_insert<'a>(doc: &'a mut DocumentMut, key: &str) -> Result<&'a mut Table, String> {
    if !doc.as_table().contains_key(key) {
        doc[key] = toml_edit::table();
    }
    if doc.get(key).and_then(Item::as_table).is_none() {
        doc[key] = toml_edit::table();
    }
    doc.get_mut(key)
        .and_then(Item::as_table_mut)
        .ok_or_else(|| format!("{key} must be a TOML table"))
}

fn ensure_plugin_enabled(doc: &mut DocumentMut, plugin_id: &str) -> Result<(), String> {
    let plugins = table_mut_or_insert(doc, "plugins")?;
    if !plugins.contains_key(plugin_id) {
        plugins[plugin_id] = toml_edit::table();
    }
    if plugins.get(plugin_id).and_then(Item::as_table).is_none() {
        plugins[plugin_id] = toml_edit::table();
    }
    plugins[plugin_id]["enabled"] = toml_edit::value(true);
    Ok(())
}

fn ensure_trailing_newline(mut contents: String) -> String {
    if !contents.ends_with('\n') {
        contents.push('\n');
    }
    contents
}

fn atomic_write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Invalid file path: {}", path.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|err| format!("Failed to create {}: {err}", parent.display()))?;
    let temp = parent.join(format!(
        ".{}.codestudio-tmp",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("config")
    ));
    std::fs::write(&temp, bytes)
        .map_err(|err| format!("Failed to write {}: {err}", temp.display()))?;
    match std::fs::rename(&temp, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(&temp);
            Err(format!("Failed to replace {}: {error}", path.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_config_text_repairs_computer_use_settings() {
        let updated = guard_config_text(
            "\u{feff}notify = [\"old.exe\", \"turn-ended\"]\n\n[features]\njs_repl = false\n\n[plugins.\"computer-use@openai-bundled\"]\nenabled = false\n",
            Some(Path::new(r"C:\tools\codex-computer-use.exe")),
        )
        .unwrap();

        assert!(!updated.as_bytes().starts_with(&[0xef, 0xbb, 0xbf]));
        assert!(updated.contains("js_repl = true"));
        assert!(updated.contains("[plugins.\"browser@openai-bundled\"]"));
        assert!(updated.contains("[plugins.\"chrome@openai-bundled\"]"));
        assert!(updated.contains("[plugins.\"computer-use@openai-bundled\"]"));
        assert!(updated.contains("enabled = true"));
        let parsed = updated.parse::<DocumentMut>().unwrap();
        let notify = parsed["notify"].as_array().unwrap();
        assert_eq!(
            notify.get(0).and_then(|value| value.as_str()),
            Some(r"C:\tools\codex-computer-use.exe")
        );
        assert_eq!(
            notify.get(1).and_then(|value| value.as_str()),
            Some("turn-ended")
        );
        assert!(!updated.contains("old.exe"));
    }

    #[test]
    fn guard_config_text_creates_missing_sections() {
        let updated = guard_config_text("model = \"gpt-5\"\n", None).unwrap();

        assert!(updated.contains("[features]"));
        assert!(updated.contains("js_repl = true"));
        for plugin_id in COMPUTER_USE_PLUGINS {
            assert!(updated.contains(&format!("[plugins.\"{plugin_id}\"]")));
        }
        assert!(!updated.contains("notify ="));
    }

    #[test]
    fn guard_config_text_writes_openai_bundled_marketplace_source() {
        let updated = guard_config_text_with_marketplace(
            "model = \"gpt-5\"\n\n[marketplaces.openai-bundled]\nsource_type = \"remote\"\nsource = \"old\"\n",
            None,
            Some(Path::new(r"C:\Users\me\.codex\.tmp\bundled-marketplaces\openai-bundled")),
        )
        .unwrap();
        let parsed = updated.parse::<DocumentMut>().unwrap();

        assert_eq!(
            parsed["marketplaces"]["openai-bundled"]["source_type"].as_str(),
            Some("local")
        );
        assert_eq!(
            parsed["marketplaces"]["openai-bundled"]["source"].as_str(),
            Some(r"\\?\C:\Users\me\.codex\.tmp\bundled-marketplaces\openai-bundled")
        );
    }

    #[test]
    fn add_sky_internal_computer_use_export_adds_exact_subpath() {
        let updated = add_sky_internal_computer_use_export(
            r#"{
  "name": "@oai/sky",
  "exports": {
    ".": "./dist/project/cua/sky_js/src/index.js"
  }
}"#,
        )
        .unwrap()
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&updated).unwrap();

        assert_eq!(
            parsed["exports"][SKY_INTERNAL_COMPUTER_USE_CLIENT_EXPORT].as_str(),
            Some(SKY_INTERNAL_COMPUTER_USE_CLIENT_EXPORT)
        );
        assert!(updated.ends_with('\n'));
    }

    #[test]
    fn add_sky_internal_computer_use_export_is_idempotent() {
        let updated = add_sky_internal_computer_use_export(&format!(
            r#"{{
  "name": "@oai/sky",
  "exports": {{
    ".": "./dist/project/cua/sky_js/src/index.js",
    "{SKY_INTERNAL_COMPUTER_USE_CLIENT_EXPORT}": "{SKY_INTERNAL_COMPUTER_USE_CLIENT_EXPORT}"
  }}
}}"#
        ))
        .unwrap();

        assert!(updated.is_none());
    }
}
