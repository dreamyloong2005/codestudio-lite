use crate::core::doctor;
use crate::core::types::DoctorReport;

#[tauri::command]
pub async fn run_doctor() -> Result<DoctorReport, String> {
    tauri::async_runtime::spawn_blocking(|| doctor::run_doctor().map_err(|err| err.to_string()))
        .await
        .map_err(|err| err.to_string())?
}
