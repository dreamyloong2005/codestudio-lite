use crate::core::activity_log;
use crate::core::app_paths::{app_paths, display_path, ensure_dirs};
use crate::core::types::{BackupManifest, RestoreBackupResult, Severity};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};

pub fn backup_files(
    reason: &str,
    profile: Option<&str>,
    files: &[PathBuf],
) -> Result<BackupManifest, String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    ensure_dirs(&paths).map_err(|err| err.to_string())?;

    let id = Utc::now().format("%Y-%m-%dT%H-%M-%S%.3fZ").to_string();
    let backup_dir = paths.backups_dir.join(&id);
    let files_dir = backup_dir.join("files");
    fs::create_dir_all(&files_dir).map_err(|err| err.to_string())?;

    let mut changed_files = Vec::new();
    for file in files {
        changed_files.push(display_path(file));
        if file.exists() {
            let file_name = backup_file_name(file);
            fs::copy(file, files_dir.join(file_name)).map_err(|err| err.to_string())?;
        }
    }

    let manifest = BackupManifest {
        id: id.clone(),
        reason: reason.to_string(),
        profile: profile.map(ToString::to_string),
        changed_files,
        created_at: Utc::now().to_rfc3339(),
    };
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|err| err.to_string())?;
    fs::write(backup_dir.join("manifest.json"), manifest_json).map_err(|err| err.to_string())?;

    Ok(manifest)
}

pub fn list_backups() -> Result<Vec<BackupManifest>, String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    ensure_dirs(&paths).map_err(|err| err.to_string())?;
    let mut backups = Vec::new();

    for entry in fs::read_dir(paths.backups_dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let manifest_path = entry.path().join("manifest.json");
        if !manifest_path.exists() {
            continue;
        }

        let content = fs::read_to_string(manifest_path).map_err(|err| err.to_string())?;
        if let Ok(manifest) = serde_json::from_str::<BackupManifest>(&content) {
            backups.push(manifest);
        }
    }

    backups.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(backups)
}

pub fn restore_backup(backup_id: &str) -> Result<RestoreBackupResult, String> {
    let backup_id = normalize_backup_id(backup_id)?;
    let paths = app_paths().map_err(|err| err.to_string())?;
    ensure_dirs(&paths).map_err(|err| err.to_string())?;

    let backup_dir = paths.backups_dir.join(&backup_id);
    let manifest_path = backup_dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err(format!("Backup '{backup_id}' does not exist"));
    }

    let manifest_content = fs::read_to_string(&manifest_path).map_err(|err| err.to_string())?;
    let manifest: BackupManifest =
        serde_json::from_str(&manifest_content).map_err(|err| err.to_string())?;
    let target_files = restore_targets(&manifest.changed_files)?;
    if target_files.is_empty() {
        return Err("Backup does not contain restorable files".to_string());
    }

    let safety_backup = backup_files(
        "restore-current",
        manifest.profile.as_deref(),
        &target_files,
    )?;
    for target in target_files {
        let source = backup_source_file(&backup_dir.join("files"), &target);
        if source.exists() {
            restore_file(&source, &target)?;
        }
    }
    activity_log::append(Severity::Ok, format!("Restored backup '{}'.", manifest.id))?;

    Ok(RestoreBackupResult {
        restored: manifest,
        safety_backup,
    })
}

fn restore_file(source: &Path, target: &Path) -> Result<(), String> {
    let tmp_path = target.with_extension("restore-tmp");
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::copy(source, &tmp_path).map_err(|err| err.to_string())?;
    if target.exists() {
        fs::remove_file(target).map_err(|err| err.to_string())?;
    }
    fs::rename(tmp_path, target).map_err(|err| err.to_string())
}

fn normalize_backup_id(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("Backup ID is required".to_string());
    }

    if trimmed.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | 'T' | 'Z')
    }) {
        Ok(trimmed.to_string())
    } else {
        Err("Backup ID contains unsupported characters".to_string())
    }
}

fn restore_targets(files: &[String]) -> Result<Vec<PathBuf>, String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    let home_display = display_path(&paths.home_dir).to_lowercase();
    let mut targets = Vec::new();

    for file in files {
        let target = if let Some(relative) = file.strip_prefix("~/") {
            paths.home_dir.join(relative)
        } else {
            PathBuf::from(file)
        };
        let target_display = display_path(&target).to_lowercase();
        if !target_display.starts_with(&home_display) {
            return Err(format!(
                "Backup target '{file}' is outside the home directory"
            ));
        }
        targets.push(target);
    }

    Ok(targets)
}

fn backup_source_file(files_dir: &Path, target: &Path) -> PathBuf {
    let source = files_dir.join(backup_file_name(target));
    if source.exists() {
        source
    } else {
        files_dir.join(legacy_backup_file_name(target))
    }
}

fn backup_file_name(path: &Path) -> String {
    let mut name = path.to_string_lossy().replace(['/', '\\', ':'], "_");
    name.retain(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
    });
    if name.is_empty() {
        "file".to_string()
    } else {
        name
    }
}

fn legacy_backup_file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file")
        .replace(['/', '\\', ':'], "-")
}
