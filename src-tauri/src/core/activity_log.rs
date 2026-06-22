use crate::core::storage;
use crate::core::types::{ActivityEvent, Severity};
use chrono::Utc;

pub fn append(level: Severity, message: impl Into<String>) -> Result<(), String> {
    let event = ActivityEvent {
        id: uuid::Uuid::new_v4().to_string(),
        level,
        message: message.into(),
        created_at: Utc::now().to_rfc3339(),
    };

    storage::append_activity_event(&event)
}

pub fn load_recent() -> Result<Vec<ActivityEvent>, String> {
    let mut events = storage::load_recent_activity(20)?;
    if events.is_empty() {
        append(Severity::Info, "Initialized activity log.")?;
        events = storage::load_recent_activity(20)?;
    }
    Ok(events)
}
