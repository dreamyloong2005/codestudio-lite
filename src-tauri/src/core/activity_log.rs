use crate::core::app_paths::{app_paths, ensure_dirs};
use crate::core::types::{ActivityEvent, Severity};
use chrono::Utc;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};

pub fn append(level: Severity, message: impl Into<String>) -> Result<(), String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    ensure_dirs(&paths).map_err(|err| err.to_string())?;

    let event = ActivityEvent {
        id: uuid::Uuid::new_v4().to_string(),
        level,
        message: message.into(),
        created_at: Utc::now().to_rfc3339(),
    };

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.activity_log_file)
        .map_err(|err| err.to_string())?;
    let line = serde_json::to_string(&event).map_err(|err| err.to_string())?;
    writeln!(file, "{line}").map_err(|err| err.to_string())
}

pub fn load_recent() -> Result<Vec<ActivityEvent>, String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    ensure_dirs(&paths).map_err(|err| err.to_string())?;

    if !paths.activity_log_file.exists() {
        append(Severity::Info, "Initialized activity log.")?;
    }

    let file = std::fs::File::open(paths.activity_log_file).map_err(|err| err.to_string())?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for line in reader.lines().map_while(Result::ok) {
        if let Ok(event) = serde_json::from_str::<ActivityEvent>(&line) {
            events.push(event);
        }
    }

    events.reverse();
    events.truncate(20);
    Ok(events)
}
