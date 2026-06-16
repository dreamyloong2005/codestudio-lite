use crate::core::gateway_request_log;
use crate::core::types::GatewayRequestLogEntry;

#[tauri::command]
pub fn load_gateway_request_log() -> Result<Vec<GatewayRequestLogEntry>, String> {
    gateway_request_log::load_recent()
}
