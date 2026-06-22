use crate::core::types::{
    ExternalToolLaunchResult, InstallTerminalInputRequest, InstallTerminalOutput,
    InstallTerminalResizeRequest, StartInstallTerminalRequest, StartInstallTerminalResult,
    StopInstallTerminalRequest,
};
use crate::core::{claude_desktop_patch, tool_launch};
use portable_pty::{Child, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tauri::Emitter;
use uuid::Uuid;

const INSTALL_TERMINAL_OUTPUT_EVENT: &str = "install-terminal://output";

struct InstallTerminalSession {
    child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
}

type SessionMap = Arc<Mutex<HashMap<String, InstallTerminalSession>>>;

fn sessions() -> &'static SessionMap {
    static SESSIONS: OnceLock<SessionMap> = OnceLock::new();
    SESSIONS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

#[tauri::command]
pub async fn start_install_terminal(
    app: tauri::AppHandle,
    request: StartInstallTerminalRequest,
) -> Result<StartInstallTerminalResult, String> {
    tauri::async_runtime::spawn_blocking(move || start_terminal_session(app, request))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn write_install_terminal(request: InstallTerminalInputRequest) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut sessions = sessions().lock().map_err(|err| err.to_string())?;
        let session = sessions
            .get_mut(&request.session_id)
            .ok_or_else(|| "Install terminal session was not found.".to_string())?;
        session
            .writer
            .write_all(request.data.as_bytes())
            .map_err(|err| format!("Failed to write to install terminal: {err}"))?;
        session
            .writer
            .flush()
            .map_err(|err| format!("Failed to flush install terminal input: {err}"))
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn resize_install_terminal(request: InstallTerminalResizeRequest) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut sessions = sessions().lock().map_err(|err| err.to_string())?;
        let session = sessions
            .get_mut(&request.session_id)
            .ok_or_else(|| "Install terminal session was not found.".to_string())?;
        session
            .master
            .resize(PtySize {
                rows: request.rows.max(10),
                cols: request.cols.max(20),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|err| format!("Failed to resize install terminal: {err}"))
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn stop_install_terminal(request: StopInstallTerminalRequest) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(mut session) = sessions()
            .lock()
            .map_err(|err| err.to_string())?
            .remove(&request.session_id)
        {
            let _ = session.child.kill();
            let _ = session.child.wait();
        }
        Ok(())
    })
    .await
    .map_err(|err| err.to_string())?
}

fn start_terminal_session(
    app: tauri::AppHandle,
    request: StartInstallTerminalRequest,
) -> Result<StartInstallTerminalResult, String> {
    let session_id = Uuid::new_v4().to_string();
    let localize = request.localize.unwrap_or(false);
    if localize && request.tool_id == "claude-desktop" {
        claude_desktop_patch::ensure_localization_patch()?;
    }
    let launch_command =
        claude_desktop_patch::patched_launch_command(&request.tool_id, &request.command, localize)?;
    let pty_system = portable_pty::native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: request.rows.unwrap_or(28).max(10),
            cols: request.cols.unwrap_or(100).max(20),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|err| format!("Failed to open install terminal PTY: {err}"))?;
    let mut command = tool_launch::shell_command_builder(
        request.shell_id.as_deref(),
        &launch_command,
        request.keep_open.unwrap_or(false),
    );
    for (key, value) in tool_launch::launch_environment_for_profile(request.profile_id.as_deref())?
    {
        command.env(key, value);
    }
    if let Some(working_directory) = normalized_working_directory(&request.working_directory) {
        command.cwd(working_directory);
    }
    let child = pair
        .slave
        .spawn_command(command)
        .map_err(|err| format!("Failed to start install terminal command: {err}"))?;
    drop(pair.slave);

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|err| format!("Failed to read install terminal output: {err}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|err| format!("Failed to open install terminal input: {err}"))?;

    sessions().lock().map_err(|err| err.to_string())?.insert(
        session_id.clone(),
        InstallTerminalSession {
            child,
            master: pair.master,
            writer,
        },
    );

    spawn_output_reader(app.clone(), session_id.clone(), reader);
    if localize && request.tool_id == "claude-desktop" {
        claude_desktop_patch::spawn_localization_injector(app, session_id.clone());
    }

    Ok(StartInstallTerminalResult {
        session_id,
        tool_id: request.tool_id,
        command: launch_command,
        started: true,
    })
}

fn normalized_working_directory(value: &Option<String>) -> Option<PathBuf> {
    let path = value.as_deref()?.trim();
    if path.is_empty() {
        return None;
    }
    Some(PathBuf::from(path))
}

fn spawn_output_reader(
    app: tauri::AppHandle,
    session_id: String,
    mut reader: Box<dyn Read + Send>,
) {
    std::thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(size) => {
                    let _ = app.emit(
                        INSTALL_TERMINAL_OUTPUT_EVENT,
                        InstallTerminalOutput {
                            session_id: session_id.clone(),
                            stream: "output".to_string(),
                            data: String::from_utf8_lossy(&buffer[..size]).into_owned(),
                            done: false,
                            exit_code: None,
                        },
                    );
                }
                Err(err) => {
                    let _ = app.emit(
                        INSTALL_TERMINAL_OUTPUT_EVENT,
                        InstallTerminalOutput {
                            session_id: session_id.clone(),
                            stream: "status".to_string(),
                            data: format!("Failed to read install terminal output: {err}\r\n"),
                            done: true,
                            exit_code: None,
                        },
                    );
                    cleanup_session(&session_id);
                    return;
                }
            }
        }

        let exit_code = cleanup_session(&session_id);
        let _ = app.emit(
            INSTALL_TERMINAL_OUTPUT_EVENT,
            InstallTerminalOutput {
                session_id,
                stream: "status".to_string(),
                data: String::new(),
                done: true,
                exit_code,
            },
        );
    });
}

fn cleanup_session(session_id: &str) -> Option<i32> {
    let Ok(mut sessions) = sessions().lock() else {
        return None;
    };
    let mut session = sessions.remove(session_id)?;
    session
        .child
        .wait()
        .ok()
        .map(|status| status.exit_code() as i32)
}

#[tauri::command]
pub async fn launch_tool_external(
    request: StartInstallTerminalRequest,
) -> Result<ExternalToolLaunchResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let localize = request.localize.unwrap_or(false);
        if localize && request.tool_id == "claude-desktop" {
            claude_desktop_patch::ensure_localization_patch()?;
        }
        let launch_command =
            claude_desktop_patch::patched_launch_command(&request.tool_id, &request.command, localize)?;
        let env = tool_launch::launch_environment_for_profile(request.profile_id.as_deref())?;
        let working_directory = normalized_working_directory(&request.working_directory);
        tool_launch::spawn_external_terminal(
            request.shell_id.as_deref(),
            &launch_command,
            &env,
            working_directory.as_deref(),
        )
        .map_err(|err| format!("Failed to open external terminal: {err}"))?;
        Ok(ExternalToolLaunchResult {
            started: true,
            tool_id: request.tool_id,
            command: launch_command,
        })
    })
    .await
    .map_err(|err| err.to_string())?
}
