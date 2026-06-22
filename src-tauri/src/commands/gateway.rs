use crate::core::gateway;
use crate::core::types::{GatewayControlResult, GatewayStatus, UpdateGatewaySettingsRequest};

#[tauri::command]
pub fn load_gateway_status() -> Result<GatewayStatus, String> {
    gateway::status_gateway()
}

#[tauri::command]
pub fn start_gateway() -> Result<GatewayControlResult, String> {
    gateway::start_gateway()
}

#[tauri::command]
pub fn stop_gateway() -> Result<GatewayControlResult, String> {
    gateway::stop_gateway()
}

#[tauri::command]
pub fn restart_gateway() -> Result<GatewayControlResult, String> {
    gateway::restart_gateway()
}

#[tauri::command]
pub fn update_gateway_settings(
    request: UpdateGatewaySettingsRequest,
) -> Result<GatewayControlResult, String> {
    gateway::update_gateway_settings(request)
}
