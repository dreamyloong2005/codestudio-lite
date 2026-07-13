use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use std::thread::JoinHandle;

#[derive(Default)]
struct GatewayRuntime {
    shutdown: Option<Sender<()>>,
    handle: Option<JoinHandle<()>>,
    started_at: Option<String>,
    last_error: Option<String>,
}

pub(in crate::core::gateway) struct Snapshot {
    pub(in crate::core::gateway) running: bool,
    pub(in crate::core::gateway) started_at: Option<String>,
    pub(in crate::core::gateway) last_error: Option<String>,
}

static RUNTIME: OnceLock<Mutex<GatewayRuntime>> = OnceLock::new();

fn state() -> &'static Mutex<GatewayRuntime> {
    RUNTIME.get_or_init(|| Mutex::new(GatewayRuntime::default()))
}

pub(in crate::core::gateway) fn is_running() -> Result<bool, String> {
    state()
        .lock()
        .map(|guard| guard.shutdown.is_some())
        .map_err(|err| err.to_string())
}

pub(in crate::core::gateway) fn mark_started(
    shutdown: Sender<()>,
    handle: JoinHandle<()>,
    started_at: String,
) -> Result<(), String> {
    let mut guard = state().lock().map_err(|err| err.to_string())?;
    guard.shutdown = Some(shutdown);
    guard.handle = Some(handle);
    guard.started_at = Some(started_at);
    guard.last_error = None;
    Ok(())
}

pub(in crate::core::gateway) fn set_last_error(message: Option<String>) {
    if let Ok(mut guard) = state().lock() {
        guard.last_error = message;
    }
}

pub(in crate::core::gateway) fn stop() -> Result<bool, String> {
    let handle = {
        let mut guard = state().lock().map_err(|err| err.to_string())?;
        if let Some(shutdown) = guard.shutdown.take() {
            let _ = shutdown.send(());
        }
        guard.started_at = None;
        guard.handle.take()
    };
    if let Some(handle) = handle {
        let _ = handle.join();
        return Ok(true);
    }
    Ok(false)
}

pub(in crate::core::gateway) fn snapshot() -> Result<Snapshot, String> {
    let guard = state().lock().map_err(|err| err.to_string())?;
    Ok(Snapshot {
        running: guard.shutdown.is_some(),
        started_at: guard.started_at.clone(),
        last_error: guard.last_error.clone(),
    })
}
