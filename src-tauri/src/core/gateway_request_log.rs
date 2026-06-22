use crate::core::storage;
use crate::core::types::GatewayRequestLogEntry;

pub fn append(entry: &GatewayRequestLogEntry) -> Result<(), String> {
    storage::append_gateway_request(entry)
}

pub fn load_recent() -> Result<Vec<GatewayRequestLogEntry>, String> {
    storage::load_recent_gateway_requests(50)
}
