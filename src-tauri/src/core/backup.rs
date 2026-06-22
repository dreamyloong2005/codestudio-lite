use crate::core::activity_log;
use crate::core::app_paths::{app_paths, display_path, ensure_dirs};
use crate::core::storage;
use crate::core::types::{BackupManifest, RestoreBackupResult, Severity};
use chrono::Utc;
use std::fs;
use std::path::PathBuf;

pub fn backup_files(
    reason: &str,
    profile: Option<&str>,
    files: &[PathBuf],
) -> Result<BackupManifest, String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    ensure_dirs(&paths).map_err(|err| err.to_string())?;

    let id = Utc::now().format("%Y-%m-%dT%H-%M-%S%.3fZ").to_string();

    let mut changed_files = Vec::new();
    for file in files {
        let target_path = display_path(file);
        changed_files.push(target_path.clone());
        if file.exists() {
            let content = fs::read(file).map_err(|err| err.to_string())?;
            storage::save_backup_file(&id, &target_path, Some(&content))?;
        } else {
            storage::save_backup_file(&id, &target_path, None)?;
        }
    }

    let manifest = BackupManifest {
        id: id.clone(),
        reason: reason.to_string(),
        profile: profile.map(ToString::to_string),
        changed_files,
        created_at: Utc::now().to_rfc3339(),
    };
    storage::save_backup_manifest(&manifest)?;

    Ok(manifest)
}

pub fn list_backups() -> Result<Vec<BackupManifest>, String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    ensure_dirs(&paths).map_err(|err| err.to_string())?;
    storage::load_backup_manifests()
}

pub fn restore_backup(backup_id: &str) -> Result<RestoreBackupResult, String> {
    let backup_id = normalize_backup_id(backup_id)?;
    let manifest = storage::load_backup_manifest(&backup_id)?
        .ok_or_else(|| format!("Backup '{backup_id}' does not exist"))?;
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
        let target_display = display_path(&target);
        if let Some(content) = storage::load_backup_file(&backup_id, &target_display)? {
            restore_file_content(&content, &target)?;
        }
    }
    activity_log::append(Severity::Ok, format!("Restored backup '{}'.", manifest.id))?;

    Ok(RestoreBackupResult {
        restored: manifest,
        safety_backup,
    })
}

fn restore_file_content(content: &[u8], target: &std::path::Path) -> Result<(), String> {
    let tmp_path = target.with_extension("restore-tmp");
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(&tmp_path, content).map_err(|err| err.to_string())?;
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
