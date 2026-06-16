use crate::core::backup;
use crate::core::types::{BackupManifest, RestoreBackupRequest, RestoreBackupResult};

#[tauri::command]
pub fn list_backups() -> Result<Vec<BackupManifest>, String> {
    backup::list_backups().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn restore_backup(request: RestoreBackupRequest) -> Result<RestoreBackupResult, String> {
    backup::restore_backup(&request.backup_id).map_err(|err| err.to_string())
}
