use crate::core::app_paths::{app_paths, ensure_dirs};
use crate::core::types::GatewayRequestLogEntry;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};

pub fn append(entry: &GatewayRequestLogEntry) -> Result<(), String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    ensure_dirs(&paths).map_err(|err| err.to_string())?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.gateway_request_log_file)
        .map_err(|err| err.to_string())?;
    let line = serde_json::to_string(entry).map_err(|err| err.to_string())?;
    writeln!(file, "{line}").map_err(|err| err.to_string())
}

pub fn load_recent() -> Result<Vec<GatewayRequestLogEntry>, String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    ensure_dirs(&paths).map_err(|err| err.to_string())?;

    if !paths.gateway_request_log_file.exists() {
        return Ok(Vec::new());
    }

    let file =
        std::fs::File::open(paths.gateway_request_log_file).map_err(|err| err.to_string())?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines().map_while(Result::ok) {
        if let Ok(entry) = serde_json::from_str::<GatewayRequestLogEntry>(&line) {
            entries.push(entry);
        }
    }

    entries.reverse();
    entries.truncate(50);
    Ok(entries)
}
