use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

use toml_edit::{DocumentMut, Item, Table};

const OPENAI_CURATED_REMOTE_MARKETPLACE: &str = "openai-curated-remote";
const OPENAI_CURATED_REMOTE_MARKETPLACE_ZIP: &[u8] =
    include_bytes!("../../resources/plugin-marketplaces/openai-curated-remote.zip");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OfficialRemotePluginCacheResult {
    pub initialized: bool,
    pub configured: bool,
}

pub fn ensure_official_remote_plugin_cache(
    home: &Path,
) -> Result<OfficialRemotePluginCacheResult, String> {
    let mut initialized = false;
    if local_official_remote_marketplace_root(home)?.is_none() {
        install_official_remote_marketplace_zip(home, OPENAI_CURATED_REMOTE_MARKETPLACE_ZIP)?;
        initialized = true;
    }
    let marketplace_root = local_official_remote_marketplace_root(home)?
        .ok_or_else(|| "Official remote plugin cache is invalid after extraction.".to_string())?;
    let configured = ensure_official_remote_marketplace_config(home, &marketplace_root)?;
    Ok(OfficialRemotePluginCacheResult {
        initialized,
        configured,
    })
}

fn official_remote_marketplace_root(home: &Path) -> PathBuf {
    home.join(".tmp").join("plugins-remote")
}

fn local_official_remote_marketplace_root(home: &Path) -> Result<Option<PathBuf>, String> {
    local_official_remote_marketplace_root_from_root(&official_remote_marketplace_root(home))
}

fn local_official_remote_marketplace_root_from_root(
    root: &Path,
) -> Result<Option<PathBuf>, String> {
    let marketplace_path = root
        .join(".agents")
        .join("plugins")
        .join("marketplace.json");
    if !marketplace_path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&marketplace_path)
        .map_err(|err| format!("Failed to read {}: {err}", marketplace_path.display()))?;
    let marketplace: serde_json::Value = serde_json::from_str(&text)
        .map_err(|err| format!("Failed to parse {}: {err}", marketplace_path.display()))?;
    if marketplace.get("name").and_then(serde_json::Value::as_str)
        != Some(OPENAI_CURATED_REMOTE_MARKETPLACE)
    {
        return Ok(None);
    }
    let has_plugins = marketplace
        .get("plugins")
        .and_then(serde_json::Value::as_array)
        .map(|plugins| !plugins.is_empty())
        .unwrap_or(false);
    if !has_plugins || !root.join("plugins").is_dir() {
        return Ok(None);
    }
    Ok(Some(root.to_path_buf()))
}

fn install_official_remote_marketplace_zip(home: &Path, bytes: &[u8]) -> Result<(), String> {
    let destination = official_remote_marketplace_root(home);
    let staging_parent = home.join(".tmp");
    std::fs::create_dir_all(&staging_parent)
        .map_err(|err| format!("Failed to create official remote plugin cache directory: {err}"))?;
    let staging = staging_parent.join(format!(
        "plugins-remote-embedded-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)
            .map_err(|err| format!("Failed to clear stale {}: {err}", staging.display()))?;
    }
    std::fs::create_dir_all(&staging)
        .map_err(|err| format!("Failed to create {}: {err}", staging.display()))?;

    let result = extract_zip_exact(bytes, &staging)
        .and_then(|_| validate_official_remote_marketplace_root(&staging))
        .and_then(|_| {
            replace_directory_with_backup_name(
                &staging,
                &destination,
                "plugins-remote.previous-codestudio-lite",
            )
        });
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

fn extract_zip_exact(bytes: &[u8], destination: &Path) -> Result<(), String> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|err| format!("Failed to read embedded plugin zip: {err}"))?;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|err| format!("Failed to read zip entry {index}: {err}"))?;
        let relative_path = safe_zip_path(file.name())?;
        let output_path = destination.join(relative_path);
        if file.is_dir() {
            std::fs::create_dir_all(&output_path)
                .map_err(|err| format!("Failed to create {}: {err}", output_path.display()))?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("Failed to create {}: {err}", parent.display()))?;
        }
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)
            .map_err(|err| format!("Failed to read zip entry {}: {err}", file.name()))?;
        std::fs::write(&output_path, contents)
            .map_err(|err| format!("Failed to write {}: {err}", output_path.display()))?;
    }
    Ok(())
}

fn safe_zip_path(name: &str) -> Result<PathBuf, String> {
    let path = Path::new(name);
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => relative.push(value),
            Component::CurDir => {}
            _ => return Err(format!("Zip entry escapes destination: {name}")),
        }
    }
    if relative.as_os_str().is_empty() {
        return Err("Zip entry has an empty path.".to_string());
    }
    Ok(relative)
}

fn validate_official_remote_marketplace_root(root: &Path) -> Result<(), String> {
    match local_official_remote_marketplace_root_from_root(root)? {
        Some(marketplace) if marketplace == root => Ok(()),
        _ => Err("Embedded official remote plugin marketplace is invalid.".to_string()),
    }
}

fn replace_directory_with_backup_name(
    source: &Path,
    destination: &Path,
    backup_name: &str,
) -> Result<(), String> {
    let backup = destination.with_file_name(backup_name);
    if backup.exists() {
        std::fs::remove_dir_all(&backup)
            .map_err(|err| format!("Failed to remove {}: {err}", backup.display()))?;
    }
    if destination.exists() {
        std::fs::rename(destination, &backup).map_err(|err| {
            format!(
                "Failed to move {} to {}: {err}",
                destination.display(),
                backup.display()
            )
        })?;
    }
    match std::fs::rename(source, destination) {
        Ok(()) => {
            if backup.exists() {
                let _ = std::fs::remove_dir_all(&backup);
            }
            Ok(())
        }
        Err(error) => {
            if backup.exists() {
                let _ = std::fs::rename(&backup, destination);
            }
            Err(format!(
                "Failed to move {} to {}: {error}",
                source.display(),
                destination.display()
            ))
        }
    }
}

fn ensure_official_remote_marketplace_config(
    home: &Path,
    marketplace_root: &Path,
) -> Result<bool, String> {
    let config_path = home.join("config.toml");
    let existing = match std::fs::read(&config_path) {
        Ok(bytes) => String::from_utf8(bytes)
            .map_err(|err| format!("Codex config.toml is not valid UTF-8: {err}"))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("Failed to read {}: {error}", config_path.display())),
    };
    let without_bom = existing.trim_start_matches('\u{feff}');
    let updated = official_remote_marketplace_config_text(without_bom, marketplace_root)?;
    if updated.as_bytes() == without_bom.as_bytes() {
        return Ok(false);
    }
    atomic_write_file(&config_path, updated.as_bytes())?;
    Ok(true)
}

fn official_remote_marketplace_config_text(
    config_text: &str,
    marketplace_root: &Path,
) -> Result<String, String> {
    let mut doc = parse_toml_document(config_text)?;
    let marketplaces = table_mut_or_insert(&mut doc, "marketplaces")?;
    if marketplaces
        .get(OPENAI_CURATED_REMOTE_MARKETPLACE)
        .and_then(Item::as_table)
        .is_none()
    {
        marketplaces[OPENAI_CURATED_REMOTE_MARKETPLACE] = toml_edit::table();
    }
    marketplaces[OPENAI_CURATED_REMOTE_MARKETPLACE]["source_type"] = toml_edit::value("local");
    marketplaces[OPENAI_CURATED_REMOTE_MARKETPLACE]["source"] =
        toml_edit::value(config_source_path(marketplace_root));
    Ok(ensure_trailing_newline(doc.to_string()))
}

fn config_source_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if cfg!(windows) {
        if value.starts_with(r"\\?\") {
            value.into_owned()
        } else {
            format!(r"\\?\{value}")
        }
    } else {
        value.into_owned()
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

    fn unique_temp_home(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "codestudio-lite-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn config_text_registers_official_remote_marketplace() {
        let root = PathBuf::from(if cfg!(windows) {
            r"C:\Users\me\.codex\.tmp\plugins-remote"
        } else {
            "/Users/me/.codex/.tmp/plugins-remote"
        });
        let updated =
            official_remote_marketplace_config_text("model = \"gpt-5\"\n", &root).unwrap();
        let parsed = updated.parse::<DocumentMut>().unwrap();

        assert_eq!(
            parsed["marketplaces"]["openai-curated-remote"]["source_type"].as_str(),
            Some("local")
        );
        assert_eq!(
            parsed["marketplaces"]["openai-curated-remote"]["source"].as_str(),
            Some(config_source_path(&root).as_str())
        );
        assert!(updated.ends_with('\n'));
    }

    #[test]
    fn ensure_official_remote_plugin_cache_installs_embedded_snapshot() {
        let home = unique_temp_home("remote-plugin-cache");
        let result = ensure_official_remote_plugin_cache(&home).unwrap();

        assert!(result.initialized);
        assert!(result.configured);
        let root = home.join(".tmp").join("plugins-remote");
        assert!(root
            .join(".agents")
            .join("plugins")
            .join("marketplace.json")
            .is_file());
        assert!(root
            .join("plugins")
            .join("product-design")
            .join(".codex-plugin")
            .join("plugin.json")
            .is_file());
        let config = std::fs::read_to_string(home.join("config.toml")).unwrap();
        let parsed = config.parse::<DocumentMut>().unwrap();
        assert_eq!(
            parsed["marketplaces"]["openai-curated-remote"]["source_type"].as_str(),
            Some("local")
        );
        cleanup(&home);
    }

    #[test]
    fn safe_zip_path_rejects_escape_entries() {
        assert!(safe_zip_path("plugins/product-design/file.txt").is_ok());
        assert!(safe_zip_path("../evil.txt").is_err());
        assert!(safe_zip_path("/evil.txt").is_err());
    }
}
