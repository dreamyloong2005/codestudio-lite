use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MacosManagedApp {
    #[serde(rename = "codestudio-lite")]
    CodeStudioLite,
    #[serde(rename = "chatgpt-desktop")]
    ChatGptDesktop,
    ClaudeDesktop,
}

#[derive(Debug, Clone, Copy)]
struct ManagedAppIdentity {
    app_names: &'static [&'static str],
    bundle_id: &'static str,
}

impl MacosManagedApp {
    fn identity(self) -> ManagedAppIdentity {
        match self {
            Self::CodeStudioLite => ManagedAppIdentity {
                app_names: &["CodeStudio Lite.app"],
                bundle_id: "com.codestudio.lite",
            },
            Self::ChatGptDesktop => ManagedAppIdentity {
                app_names: &[
                    "ChatGPT.app",
                    "Codex.app",
                    "OpenAI Codex.app",
                    "OpenAI.Codex.app",
                ],
                bundle_id: "com.openai.codex",
            },
            Self::ClaudeDesktop => ManagedAppIdentity {
                app_names: &["Claude.app"],
                bundle_id: "com.anthropic.claudefordesktop",
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MacosInstallScope {
    System,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacosAppResolution {
    pub scope: MacosInstallScope,
    pub system_app: Option<PathBuf>,
    pub user_app: Option<PathBuf>,
    pub preferred_app: Option<PathBuf>,
    pub preferred_destination: PathBuf,
    pub ordered_candidates: Vec<PathBuf>,
    pub duplicate_user_install: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacosApplicationScopeStatus {
    pub app_id: MacosManagedApp,
    pub system_app: Option<PathBuf>,
    pub user_apps: Vec<PathBuf>,
    pub preferred_app: Option<PathBuf>,
    pub preferred_destination: PathBuf,
    pub duplicate_user_install: bool,
    pub running_app: Option<PathBuf>,
    pub running_scope: Option<MacosInstallScope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacosApplicationCleanupResult {
    pub status: MacosApplicationScopeStatus,
    pub moved_to_trash: Vec<PathBuf>,
    pub restart_scheduled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodestudioSelfCleanupMode {
    Direct {
        user_app: PathBuf,
        system_app: PathBuf,
    },
    PostExit {
        user_app: PathBuf,
        system_app: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodestudioSelfCleanupHelperExecutionPlan {
    pub executable: PathBuf,
    pub args: Vec<std::ffi::OsString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodestudioSelfCleanupHelperRequest {
    pub parent_pid: u32,
    pub home: PathBuf,
    pub system_applications: PathBuf,
    pub original_path: PathBuf,
    pub staged_path: PathBuf,
    pub system_app: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodestudioSelfCleanupHelperResult {
    pub moved_to_trash: PathBuf,
    pub launched_app: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodestudioSelfCleanupFailure {
    pub message: String,
    pub restored_user_app: Option<PathBuf>,
    pub system_app: PathBuf,
}

#[derive(Debug)]
pub struct StagedUserBundle {
    home: PathBuf,
    app: MacosManagedApp,
    original_path: PathBuf,
    staged_path: PathBuf,
}

impl StagedUserBundle {
    pub fn original_path(&self) -> &Path {
        &self.original_path
    }

    pub fn staged_path(&self) -> &Path {
        &self.staged_path
    }
}

pub fn resolve(
    home: &Path,
    system_applications: &Path,
    app: MacosManagedApp,
) -> MacosAppResolution {
    let identity = app.identity();

    let user_applications = home.join("Applications");
    let system_candidates =
        valid_bundles_in_root(system_applications, identity.app_names, identity.bundle_id);
    let user_candidates =
        valid_bundles_in_root(&user_applications, identity.app_names, identity.bundle_id);
    let system_app = system_candidates.first().cloned();
    let user_app = user_candidates.first().cloned();
    let duplicate_user_install = system_app.is_some() && user_app.is_some();

    let (scope, preferred_app, preferred_destination) = match (&system_app, &user_app) {
        (Some(system), _) => (
            MacosInstallScope::System,
            Some(system.clone()),
            system.clone(),
        ),
        (None, Some(user)) => (MacosInstallScope::User, Some(user.clone()), user.clone()),
        (None, None) => (
            MacosInstallScope::System,
            None,
            system_applications.join(identity.app_names[0]),
        ),
    };

    let ordered_candidates = system_candidates
        .into_iter()
        .chain(user_candidates)
        .collect();

    MacosAppResolution {
        scope,
        system_app,
        user_app,
        preferred_app,
        preferred_destination,
        ordered_candidates,
        duplicate_user_install,
    }
}

pub fn status_for_roots(
    home: &Path,
    system_applications: &Path,
    app: MacosManagedApp,
    current_executable: Option<&Path>,
) -> MacosApplicationScopeStatus {
    let resolution = resolve(home, system_applications, app);
    let user_root = home.join("Applications");
    let user_apps = resolution
        .ordered_candidates
        .iter()
        .filter(|candidate| candidate.parent() == Some(user_root.as_path()))
        .cloned()
        .collect::<Vec<_>>();
    let running_app = current_executable
        .and_then(macos_bundle_for_executable)
        .filter(|bundle| {
            resolution
                .ordered_candidates
                .iter()
                .any(|candidate| candidate == bundle)
        });
    let running_scope = running_app.as_ref().and_then(|bundle| {
        if resolution.system_app.as_ref() == Some(bundle) {
            Some(MacosInstallScope::System)
        } else if user_apps.iter().any(|candidate| candidate == bundle) {
            Some(MacosInstallScope::User)
        } else {
            None
        }
    });

    MacosApplicationScopeStatus {
        app_id: app,
        system_app: resolution.system_app,
        user_apps,
        preferred_app: resolution.preferred_app,
        preferred_destination: resolution.preferred_destination,
        duplicate_user_install: resolution.duplicate_user_install,
        running_app,
        running_scope,
    }
}

fn macos_bundle_for_executable(executable: &Path) -> Option<PathBuf> {
    executable.ancestors().find_map(|ancestor| {
        (ancestor
            .extension()
            .and_then(|extension| extension.to_str())
            == Some("app"))
        .then(|| ancestor.to_path_buf())
    })
}

pub fn cleanup_managed_user_bundles_for_roots(
    home: &Path,
    system_applications: &Path,
    app: MacosManagedApp,
) -> Result<MacosApplicationCleanupResult, String> {
    if app == MacosManagedApp::CodeStudioLite {
        return Err(
            "CodeStudio Lite self-cleanup requires the application exit helper.".to_string(),
        );
    }

    let before = status_for_roots(home, system_applications, app, None);
    if before.user_apps.is_empty() {
        return Ok(MacosApplicationCleanupResult {
            status: before,
            moved_to_trash: Vec::new(),
            restart_scheduled: false,
        });
    }
    if before.system_app.is_none() {
        return Err(
            "Cleanup was refused because no verified /Applications copy is available.".to_string(),
        );
    }
    let expected_system = before.system_app.clone().expect("checked above");
    let staged = stage_all_user_bundles(home, app, before.user_apps.len())?;
    let moved_to_trash = finalize_staged_user_bundles_for_roots_with(
        home,
        system_applications,
        app,
        &expected_system,
        staged,
        |roots| {
            crate::core::process_control::close_processes_in_macos_bundles(
                "managed macOS application",
                roots,
            )
            .map(|_| ())
        },
    )?;
    Ok(MacosApplicationCleanupResult {
        status: status_for_roots(home, system_applications, app, None),
        moved_to_trash,
        restart_scheduled: false,
    })
}

fn stage_all_user_bundles(
    home: &Path,
    app: MacosManagedApp,
    count: usize,
) -> Result<Vec<StagedUserBundle>, String> {
    let mut staged = Vec::with_capacity(count);
    for _ in 0..count {
        match stage_user_bundle_for_trash(home, app) {
            Ok(bundle) => staged.push(bundle),
            Err(error) => {
                let restore_error = restore_staged_user_bundles(staged).err();
                return Err(match restore_error {
                    Some(restore_error) => {
                        format!("{error} Previously staged bundles could not be restored: {restore_error}")
                    }
                    None => error,
                });
            }
        }
    }
    Ok(staged)
}

fn restore_staged_user_bundles(mut staged: Vec<StagedUserBundle>) -> Result<Vec<PathBuf>, String> {
    staged.reverse();
    let mut restored = Vec::new();
    let mut errors = Vec::new();
    for bundle in staged {
        match restore_staged_user_bundle(bundle) {
            Ok(path) => restored.push(path),
            Err(error) => errors.push(error),
        }
    }
    if errors.is_empty() {
        Ok(restored)
    } else {
        Err(errors.join(" "))
    }
}

fn validate_exact_system_bundle(
    home: &Path,
    system_applications: &Path,
    app: MacosManagedApp,
    expected_system: &Path,
) -> Result<(), String> {
    let current = resolve(home, system_applications, app).system_app;
    if current.as_deref() == Some(expected_system) {
        Ok(())
    } else {
        Err(format!(
            "Cleanup was cancelled because the verified /Applications copy at {} disappeared or changed.",
            expected_system.display()
        ))
    }
}

fn finalize_staged_user_bundles_for_roots_with<FDrain>(
    home: &Path,
    system_applications: &Path,
    app: MacosManagedApp,
    expected_system: &Path,
    staged: Vec<StagedUserBundle>,
    mut drain_processes: FDrain,
) -> Result<Vec<PathBuf>, String>
where
    FDrain: FnMut(&[PathBuf]) -> Result<(), String>,
{
    let roots = staged
        .iter()
        .flat_map(|bundle| {
            [
                bundle.original_path().to_path_buf(),
                bundle.staged_path().to_path_buf(),
            ]
        })
        .collect::<Vec<_>>();
    if let Err(error) = drain_processes(&roots) {
        let restore_error = restore_staged_user_bundles(staged).err();
        return Err(match restore_error {
            Some(restore_error) => {
                format!("{error} The staged user copy could not be restored: {restore_error}")
            }
            None => error,
        });
    }
    if let Err(error) =
        validate_exact_system_bundle(home, system_applications, app, expected_system)
    {
        let restore_error = restore_staged_user_bundles(staged).err();
        return Err(match restore_error {
            Some(restore_error) => {
                format!("{error} The staged user copy could not be restored: {restore_error}")
            }
            None => error,
        });
    }

    staged
        .into_iter()
        .map(finalize_staged_user_bundle_to_trash)
        .collect()
}

pub fn plan_codestudio_self_cleanup_for_roots(
    home: &Path,
    system_applications: &Path,
    current_executable: &Path,
) -> Result<CodestudioSelfCleanupMode, String> {
    let status = status_for_roots(
        home,
        system_applications,
        MacosManagedApp::CodeStudioLite,
        Some(current_executable),
    );
    let system_app = status.system_app.ok_or_else(|| {
        "The verified /Applications CodeStudio Lite copy was not found.".to_string()
    })?;
    let user_app = status
        .user_apps
        .into_iter()
        .next()
        .ok_or_else(|| "The verified user CodeStudio Lite copy was not found.".to_string())?;

    if status.running_scope == Some(MacosInstallScope::User) {
        Ok(CodestudioSelfCleanupMode::PostExit {
            user_app,
            system_app,
        })
    } else {
        Ok(CodestudioSelfCleanupMode::Direct {
            user_app,
            system_app,
        })
    }
}

pub fn stage_user_bundle_for_trash(
    home: &Path,
    app: MacosManagedApp,
) -> Result<StagedUserBundle, String> {
    let identity = app.identity();

    let applications = home.join("Applications");
    let original_path =
        valid_bundles_in_root(&applications, identity.app_names, identity.bundle_id)
            .into_iter()
            .next()
            .ok_or_else(|| "No valid allowlisted user application bundle was found.".to_string())?;
    let staging_candidates = staging_candidate_paths(&applications, &original_path)?;
    let staged_path = move_to_first_available(
        &original_path,
        staging_candidates,
        "application cleanup staging path",
    )?;

    Ok(StagedUserBundle {
        home: home.to_path_buf(),
        app,
        original_path,
        staged_path,
    })
}

pub fn restore_staged_user_bundle(staged: StagedUserBundle) -> Result<PathBuf, String> {
    validate_staged_location(&staged)?;

    atomic_rename_noreplace(&staged.staged_path, &staged.original_path).map_err(|error| {
        format!(
            "Failed to restore {} to {}: {error}",
            staged.staged_path.display(),
            staged.original_path.display()
        )
    })?;
    Ok(staged.original_path)
}

pub fn resume_staged_user_bundle(
    home: &Path,
    app: MacosManagedApp,
    original_path: PathBuf,
    staged_path: PathBuf,
) -> Result<StagedUserBundle, String> {
    let staged = StagedUserBundle {
        home: home.to_path_buf(),
        app,
        original_path,
        staged_path,
    };
    validate_staged_location(&staged)?;
    validate_staged_bundle(&staged)?;
    Ok(staged)
}

pub fn finalize_staged_user_bundle_to_trash(staged: StagedUserBundle) -> Result<PathBuf, String> {
    if let Err(error) = validate_staged_bundle(&staged) {
        return restore_after_finalize_failure(staged, error);
    }

    let trash = staged.home.join(".Trash");
    match fs::symlink_metadata(&trash) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Err(error) = fs::create_dir(&trash) {
                return restore_after_finalize_failure(
                    staged,
                    format!("Failed to create {}: {error}", trash.display()),
                );
            }
        }
        Err(error) => {
            return restore_after_finalize_failure(
                staged,
                format!("Failed to inspect {}: {error}", trash.display()),
            );
        }
    }
    if !is_real_directory(&trash) {
        return restore_after_finalize_failure(
            staged,
            format!(
                "Trash directory is not a real directory: {}",
                trash.display()
            ),
        );
    }

    let trash_candidates = match trash_candidate_paths(&trash, &staged.original_path) {
        Ok(candidates) => candidates,
        Err(error) => return restore_after_finalize_failure(staged, error),
    };
    match move_to_first_available(&staged.staged_path, trash_candidates, "path in Trash") {
        Ok(destination) => Ok(destination),
        Err(error) => restore_after_finalize_failure(staged, error),
    }
}

pub fn move_user_bundle_to_trash(home: &Path, app: MacosManagedApp) -> Result<PathBuf, String> {
    let staged = stage_user_bundle_for_trash(home, app)?;
    finalize_staged_user_bundle_to_trash(staged)
}

const SELF_CLEANUP_HELPER_ARGUMENT: &str = "--codestudio-self-cleanup-helper";

pub fn spawn_codestudio_self_cleanup_helper(
    home: &Path,
    system_applications: &Path,
    current_executable: &Path,
) -> Result<MacosApplicationCleanupResult, String> {
    let mode =
        plan_codestudio_self_cleanup_for_roots(home, system_applications, current_executable)?;
    match mode {
        CodestudioSelfCleanupMode::Direct {
            user_app,
            system_app,
        } => {
            let staged = stage_user_bundle_for_trash(home, MacosManagedApp::CodeStudioLite)?;
            if staged.original_path() != user_app {
                let _ = restore_staged_user_bundle(staged);
                return Err(
                    "The staged CodeStudio Lite bundle did not match the verified user copy."
                        .to_string(),
                );
            }
            let moved_to_trash = finalize_staged_user_bundles_for_roots_with(
                home,
                system_applications,
                MacosManagedApp::CodeStudioLite,
                &system_app,
                vec![staged],
                |roots| {
                    crate::core::process_control::close_processes_in_macos_bundles(
                        "CodeStudio Lite user copy",
                        roots,
                    )
                    .map(|_| ())
                },
            )?;
            Ok(MacosApplicationCleanupResult {
                status: status_for_roots(
                    home,
                    system_applications,
                    MacosManagedApp::CodeStudioLite,
                    Some(current_executable),
                ),
                moved_to_trash,
                restart_scheduled: false,
            })
        }
        CodestudioSelfCleanupMode::PostExit {
            user_app,
            system_app,
        } => {
            let staged = stage_user_bundle_for_trash(home, MacosManagedApp::CodeStudioLite)?;
            if staged.original_path() != user_app {
                let _ = restore_staged_user_bundle(staged);
                return Err(
                    "The staged CodeStudio Lite bundle did not match the verified user copy."
                        .to_string(),
                );
            }
            let execution = codestudio_self_cleanup_helper_execution_plan(
                std::process::id(),
                home,
                system_applications,
                current_executable,
                &system_app,
                &staged,
            )?;
            let spawn = Command::new(&execution.executable)
                .args(&execution.args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            if let Err(error) = spawn {
                let _ = restore_staged_user_bundle(staged);
                return Err(format!(
                    "Failed to start the secured CodeStudio Lite cleanup helper: {error}"
                ));
            }
            Ok(MacosApplicationCleanupResult {
                status: status_for_roots(
                    home,
                    system_applications,
                    MacosManagedApp::CodeStudioLite,
                    Some(current_executable),
                ),
                moved_to_trash: Vec::new(),
                restart_scheduled: true,
            })
        }
    }
}

fn codestudio_self_cleanup_helper_execution_plan(
    parent_pid: u32,
    home: &Path,
    system_applications: &Path,
    current_executable: &Path,
    system_app: &Path,
    staged: &StagedUserBundle,
) -> Result<CodestudioSelfCleanupHelperExecutionPlan, String> {
    let relative_executable = current_executable
        .strip_prefix(staged.original_path())
        .map_err(|_| {
            "The running CodeStudio Lite executable is outside the verified user bundle."
                .to_string()
        })?;
    Ok(CodestudioSelfCleanupHelperExecutionPlan {
        executable: staged.staged_path().join(relative_executable),
        args: vec![
            std::ffi::OsString::from(SELF_CLEANUP_HELPER_ARGUMENT),
            std::ffi::OsString::from(parent_pid.to_string()),
            home.as_os_str().to_os_string(),
            system_applications.as_os_str().to_os_string(),
            staged.original_path().as_os_str().to_os_string(),
            staged.staged_path().as_os_str().to_os_string(),
            system_app.as_os_str().to_os_string(),
        ],
    })
}

pub fn run_codestudio_self_cleanup_helper_from_args() -> bool {
    let args = std::env::args_os().collect::<Vec<_>>();
    if args.get(1).and_then(|arg| arg.to_str()) != Some(SELF_CLEANUP_HELPER_ARGUMENT) {
        return false;
    }
    let helper_args = &args[2..];
    if let Err(error) = run_codestudio_self_cleanup_helper(helper_args) {
        if let Ok(request) = parse_codestudio_self_cleanup_helper_request(helper_args) {
            if !codestudio_self_cleanup_failure_path(&request.home).exists() {
                let _ = persist_codestudio_self_cleanup_failure(
                    &request.home,
                    &CodestudioSelfCleanupFailure {
                        message: error.clone(),
                        restored_user_app: None,
                        system_app: request.system_app,
                    },
                );
            }
        }
        eprintln!("CodeStudio Lite cleanup helper failed: {error}");
    }
    true
}

fn run_codestudio_self_cleanup_helper(args: &[std::ffi::OsString]) -> Result<(), String> {
    let request = parse_codestudio_self_cleanup_helper_request(args)?;
    let started = Instant::now();
    run_codestudio_self_cleanup_helper_request_with(
        request,
        Duration::from_secs(300),
        || started.elapsed(),
        process_is_alive,
        || thread::sleep(Duration::from_millis(200)),
        |roots| {
            crate::core::process_control::close_processes_in_macos_bundles(
                "CodeStudio Lite user copy",
                roots,
            )
            .map(|_| ())
        },
        launch_exact_macos_app,
    )
}

fn run_codestudio_self_cleanup_helper_request_with<FNow, FAlive, FPause, FDrain, FLaunch>(
    request: CodestudioSelfCleanupHelperRequest,
    parent_exit_timeout: Duration,
    mut now: FNow,
    mut parent_alive: FAlive,
    mut pause: FPause,
    drain_processes: FDrain,
    mut launch: FLaunch,
) -> Result<(), String>
where
    FNow: FnMut() -> Duration,
    FAlive: FnMut(u32) -> bool,
    FPause: FnMut(),
    FDrain: FnMut(&[PathBuf]) -> Result<(), String>,
    FLaunch: FnMut(&Path) -> Result<(), String>,
{
    while now() < parent_exit_timeout && parent_alive(request.parent_pid) {
        pause();
    }
    if parent_alive(request.parent_pid) {
        return recover_codestudio_user_copy_after_failure(
            &request,
            None,
            "The running CodeStudio Lite process did not exit in time.".to_string(),
            &mut launch,
        )
        .map(|_| ());
    }

    complete_codestudio_self_cleanup_with(request, drain_processes, launch).map(|_| ())
}

fn parse_codestudio_self_cleanup_helper_request(
    args: &[std::ffi::OsString],
) -> Result<CodestudioSelfCleanupHelperRequest, String> {
    if args.len() != 6 {
        return Err("The CodeStudio Lite cleanup helper arguments are invalid.".to_string());
    }
    Ok(CodestudioSelfCleanupHelperRequest {
        parent_pid: args[0]
            .to_str()
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or_else(|| "The CodeStudio Lite cleanup parent PID is invalid.".to_string())?,
        home: PathBuf::from(&args[1]),
        system_applications: PathBuf::from(&args[2]),
        original_path: PathBuf::from(&args[3]),
        staged_path: PathBuf::from(&args[4]),
        system_app: PathBuf::from(&args[5]),
    })
}

fn launch_exact_macos_app(app: &Path) -> Result<(), String> {
    let output = Command::new("/usr/bin/open")
        .arg(app)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| {
            format!(
                "Failed to execute /usr/bin/open for {}: {error}",
                app.display()
            )
        })?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!(
            "/usr/bin/open failed for {} with status {}: {}",
            app.display(),
            output.status,
            if stderr.is_empty() {
                "no error output"
            } else {
                &stderr
            }
        ))
    }
}

fn complete_codestudio_self_cleanup_with<FDrain, FLaunch>(
    request: CodestudioSelfCleanupHelperRequest,
    drain_processes: FDrain,
    mut launch: FLaunch,
) -> Result<CodestudioSelfCleanupHelperResult, String>
where
    FDrain: FnMut(&[PathBuf]) -> Result<(), String>,
    FLaunch: FnMut(&Path) -> Result<(), String>,
{
    let completion = (|| {
        let staged = resume_staged_user_bundle(
            &request.home,
            MacosManagedApp::CodeStudioLite,
            request.original_path.clone(),
            request.staged_path.clone(),
        )?;
        let mut moved = finalize_staged_user_bundles_for_roots_with(
            &request.home,
            &request.system_applications,
            MacosManagedApp::CodeStudioLite,
            &request.system_app,
            vec![staged],
            drain_processes,
        )?;
        moved.pop().ok_or_else(|| {
            "CodeStudio Lite cleanup did not produce a Trash destination.".to_string()
        })
    })();
    let moved_to_trash = match completion {
        Ok(path) => path,
        Err(error) => {
            return recover_codestudio_user_copy_after_failure(&request, None, error, &mut launch)
        }
    };

    match launch(&request.system_app) {
        Ok(()) => {
            let _ = fs::remove_file(codestudio_self_cleanup_failure_path(&request.home));
            Ok(CodestudioSelfCleanupHelperResult {
                moved_to_trash,
                launched_app: request.system_app,
            })
        }
        Err(system_open_error) => recover_codestudio_user_copy_after_failure(
            &request,
            Some(&moved_to_trash),
            format!("Failed to launch the exact system CodeStudio Lite copy: {system_open_error}"),
            &mut launch,
        ),
    }
}

fn recover_codestudio_user_copy_after_failure<FLaunch>(
    request: &CodestudioSelfCleanupHelperRequest,
    trash_path: Option<&Path>,
    failure: String,
    launch: &mut FLaunch,
) -> Result<CodestudioSelfCleanupHelperResult, String>
where
    FLaunch: FnMut(&Path) -> Result<(), String>,
{
    let identity = MacosManagedApp::CodeStudioLite.identity();
    let restored = if is_valid_bundle(&request.original_path, identity.bundle_id) {
        Ok(request.original_path.clone())
    } else if is_valid_bundle(&request.staged_path, identity.bundle_id) {
        resume_staged_user_bundle(
            &request.home,
            MacosManagedApp::CodeStudioLite,
            request.original_path.clone(),
            request.staged_path.clone(),
        )
        .and_then(restore_staged_user_bundle)
    } else if let Some(trash_path) = trash_path {
        restore_trashed_user_bundle(
            &request.home,
            MacosManagedApp::CodeStudioLite,
            &request.original_path,
            trash_path,
        )
    } else {
        Err(
            "No verified staged, original, or trashed user bundle was available for recovery."
                .to_string(),
        )
    };

    let (restored_user_app, recovery_detail) = match restored {
        Ok(path) => match launch(&path) {
            Ok(()) => (
                Some(path),
                "The verified user copy was restored and relaunched.".to_string(),
            ),
            Err(error) => (
                Some(path.clone()),
                format!(
                    "The verified user copy was restored at {}, but relaunch failed: {error}",
                    path.display()
                ),
            ),
        },
        Err(error) => (None, format!("User-copy recovery failed: {error}")),
    };
    let message = format!("{failure}. {recovery_detail}");
    persist_codestudio_self_cleanup_failure(
        &request.home,
        &CodestudioSelfCleanupFailure {
            message: message.clone(),
            restored_user_app,
            system_app: request.system_app.clone(),
        },
    )?;
    Err(message)
}

fn restore_trashed_user_bundle(
    home: &Path,
    app: MacosManagedApp,
    original_path: &Path,
    trash_path: &Path,
) -> Result<PathBuf, String> {
    let identity = app.identity();
    let applications = home.join("Applications");
    let trash = home.join(".Trash");
    if original_path.parent() != Some(applications.as_path())
        || !original_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| identity.app_names.contains(&name))
    {
        return Err("The rollback destination is not an allowlisted user app path.".to_string());
    }
    if trash_path.parent() != Some(trash.as_path())
        || !is_real_directory(trash_path)
        || !is_valid_bundle(trash_path, identity.bundle_id)
    {
        return Err("The trashed application bundle is not a valid rollback source.".to_string());
    }
    atomic_rename_noreplace(trash_path, original_path).map_err(|error| {
        format!(
            "Failed to restore {} to {}: {error}",
            trash_path.display(),
            original_path.display()
        )
    })?;
    Ok(original_path.to_path_buf())
}

fn codestudio_self_cleanup_failure_path(home: &Path) -> PathBuf {
    home.join(".codestudio-lite")
        .join("macos-self-cleanup-failure.json")
}

fn persist_codestudio_self_cleanup_failure(
    home: &Path,
    failure: &CodestudioSelfCleanupFailure,
) -> Result<(), String> {
    let path = codestudio_self_cleanup_failure_path(home);
    let parent = path
        .parent()
        .ok_or_else(|| "The cleanup failure state path is invalid.".to_string())?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "Failed to create cleanup failure state directory {}: {error}",
            parent.display()
        )
    })?;
    let payload = serde_json::to_vec_pretty(failure)
        .map_err(|error| format!("Failed to serialize cleanup failure state: {error}"))?;
    fs::write(&path, payload).map_err(|error| {
        format!(
            "Failed to persist cleanup failure state {}: {error}",
            path.display()
        )
    })
}

pub fn take_codestudio_self_cleanup_failure(
    home: &Path,
) -> Result<Option<CodestudioSelfCleanupFailure>, String> {
    let path = codestudio_self_cleanup_failure_path(home);
    let payload = match fs::read(&path) {
        Ok(payload) => payload,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Failed to read cleanup failure state {}: {error}",
                path.display()
            ))
        }
    };
    let failure =
        serde_json::from_slice::<CodestudioSelfCleanupFailure>(&payload).map_err(|error| {
            format!(
                "Failed to parse cleanup failure state {}: {error}",
                path.display()
            )
        })?;
    fs::remove_file(&path).map_err(|error| {
        format!(
            "Failed to clear delivered cleanup failure state {}: {error}",
            path.display()
        )
    })?;
    Ok(Some(failure))
}

fn process_is_alive(pid: u32) -> bool {
    Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn valid_bundles_in_root(root: &Path, app_names: &[&str], bundle_id: &str) -> Vec<PathBuf> {
    if !is_real_directory(root) {
        return Vec::new();
    }

    app_names
        .iter()
        .map(|app_name| root.join(app_name))
        .filter(|app| is_valid_bundle(app, bundle_id))
        .collect()
}

fn is_valid_bundle(app: &Path, bundle_id: &str) -> bool {
    if !is_real_directory(app) {
        return false;
    }
    let contents = app.join("Contents");
    if !is_real_directory(&contents) {
        return false;
    }
    let plist = contents.join("Info.plist");
    if !is_real_file(&plist) {
        return false;
    }
    plist::Value::from_file(plist)
        .ok()
        .and_then(|value| {
            value
                .as_dictionary()?
                .get("CFBundleIdentifier")?
                .as_string()
                .map(str::to_owned)
        })
        .as_deref()
        == Some(bundle_id)
}

fn is_real_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| !metadata.file_type().is_symlink() && metadata.is_dir())
        .unwrap_or(false)
}

fn is_real_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| !metadata.file_type().is_symlink() && metadata.is_file())
        .unwrap_or(false)
}

fn staging_candidate_paths(
    applications: &Path,
    original_path: &Path,
) -> Result<Vec<PathBuf>, String> {
    let file_name = original_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Allowlisted application has an invalid file name.".to_string())?;
    let stem = original_path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Allowlisted application has an invalid file stem.".to_string())?;
    let mut candidates = Vec::with_capacity(10_000);
    candidates.push(applications.join(format!(".codestudio-lite-trash-staging-{file_name}")));
    candidates.extend((2..=10_000).map(|suffix| {
        applications.join(format!(
            ".codestudio-lite-trash-staging-{stem} {suffix}.app"
        ))
    }));
    Ok(candidates)
}

fn trash_candidate_paths(trash: &Path, original_path: &Path) -> Result<Vec<PathBuf>, String> {
    let file_name = original_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Allowlisted application has an invalid file name.".to_string())?;
    let stem = original_path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Allowlisted application has an invalid file stem.".to_string())?;
    let mut candidates = Vec::with_capacity(10_000);
    candidates.push(trash.join(file_name));
    candidates.extend((2..=10_000).map(|suffix| trash.join(format!("{stem} {suffix}.app"))));
    Ok(candidates)
}

fn move_to_first_available(
    source: &Path,
    candidates: Vec<PathBuf>,
    destination_description: &str,
) -> Result<PathBuf, String> {
    for candidate in candidates {
        match atomic_rename_noreplace(source, &candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Failed to move {} to {}: {error}",
                    source.display(),
                    candidate.display()
                ));
            }
        }
    }
    Err(format!(
        "Failed to allocate a unique {destination_description}."
    ))
}

#[cfg(any(target_vendor = "apple", target_os = "linux", target_os = "redox"))]
fn atomic_rename_noreplace(source: &Path, destination: &Path) -> io::Result<()> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        source,
        rustix::fs::CWD,
        destination,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(Into::into)
}

#[cfg(windows)]
fn atomic_rename_noreplace(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(not(any(
    target_vendor = "apple",
    target_os = "linux",
    target_os = "redox",
    windows
)))]
fn atomic_rename_noreplace(_source: &Path, _destination: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "atomic no-replace rename is unsupported on this platform",
    ))
}

fn validate_staged_location(staged: &StagedUserBundle) -> Result<(), String> {
    let identity = staged.app.identity();
    let applications = staged.home.join("Applications");
    if !is_real_directory(&applications) {
        return Err(format!(
            "User Applications directory is not a real directory: {}",
            applications.display()
        ));
    }
    if staged.original_path.parent() != Some(applications.as_path())
        || !staged
            .original_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| identity.app_names.contains(&name))
    {
        return Err("Staged application does not have an allowlisted original path.".to_string());
    }
    if staged.staged_path.parent() != Some(applications.as_path())
        || !staged
            .staged_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".codestudio-lite-trash-staging-"))
    {
        return Err("Staged application is outside the managed staging location.".to_string());
    }
    if !is_real_directory(&staged.staged_path) {
        return Err("Staged application is not a real directory.".to_string());
    }
    Ok(())
}

fn validate_staged_bundle(staged: &StagedUserBundle) -> Result<(), String> {
    validate_staged_location(staged)?;
    let identity = staged.app.identity();
    if !is_valid_bundle(&staged.staged_path, identity.bundle_id) {
        return Err("Staged application bundle is no longer valid.".to_string());
    }
    Ok(())
}

fn restore_after_finalize_failure(
    staged: StagedUserBundle,
    finalize_error: String,
) -> Result<PathBuf, String> {
    match restore_staged_user_bundle(staged) {
        Ok(_) => Err(finalize_error),
        Err(restore_error) => Err(format!(
            "{finalize_error} The staged bundle could not be restored: {restore_error}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        atomic_rename_noreplace, cleanup_managed_user_bundles_for_roots,
        codestudio_self_cleanup_helper_execution_plan, complete_codestudio_self_cleanup_with,
        finalize_staged_user_bundle_to_trash, finalize_staged_user_bundles_for_roots_with,
        move_user_bundle_to_trash, persist_codestudio_self_cleanup_failure,
        plan_codestudio_self_cleanup_for_roots, resolve, restore_staged_user_bundle,
        run_codestudio_self_cleanup_helper_request_with, stage_user_bundle_for_trash,
        status_for_roots, take_codestudio_self_cleanup_failure, CodestudioSelfCleanupFailure,
        CodestudioSelfCleanupHelperRequest, CodestudioSelfCleanupMode, MacosInstallScope,
        MacosManagedApp,
    };
    use plist::{Dictionary, Value};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    const BUNDLE_ID: &str = "com.openai.codex";

    struct TestRoot {
        path: PathBuf,
    }

    impl TestRoot {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "codestudio-lite-macos-app-scope-{name}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("system clock should be after the Unix epoch")
                    .as_nanos()
            ));
            fs::create_dir_all(&path).expect("test root should be created");
            Self { path }
        }

        fn home(&self) -> PathBuf {
            self.path.join("home")
        }

        fn system_applications(&self) -> PathBuf {
            self.path.join("system-applications")
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn write_app(applications: &Path, app_name: &str, bundle_id: &str) -> PathBuf {
        let app = applications.join(app_name);
        let contents = app.join("Contents");
        fs::create_dir_all(&contents).expect("app Contents should be created");
        fs::write(
            contents.join("Info.plist"),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>{bundle_id}</string>
</dict>
</plist>
"#
            ),
        )
        .expect("Info.plist should be written");
        app
    }

    fn write_binary_app(applications: &Path, app_name: &str, bundle_id: &str) -> PathBuf {
        let app = applications.join(app_name);
        let contents = app.join("Contents");
        fs::create_dir_all(&contents).expect("app Contents should be created");
        let mut dictionary = Dictionary::new();
        dictionary.insert(
            "CFBundleIdentifier".to_string(),
            Value::String(bundle_id.to_string()),
        );
        Value::Dictionary(dictionary)
            .to_file_binary(contents.join("Info.plist"))
            .expect("binary Info.plist should be written");
        app
    }

    #[test]
    fn system_only_selects_the_system_bundle() {
        let root = TestRoot::new("system-only");
        let system_app = write_app(&root.system_applications(), "ChatGPT.app", BUNDLE_ID);

        let resolution = resolve(
            &root.home(),
            &root.system_applications(),
            MacosManagedApp::ChatGptDesktop,
        );

        assert_eq!(resolution.scope, MacosInstallScope::System);
        assert_eq!(resolution.system_app.as_deref(), Some(system_app.as_path()));
        assert_eq!(resolution.user_app, None);
        assert_eq!(
            resolution.preferred_app.as_deref(),
            Some(system_app.as_path())
        );
        assert_eq!(resolution.preferred_destination, system_app);
        assert_eq!(resolution.ordered_candidates, vec![system_app]);
        assert!(!resolution.duplicate_user_install);
    }

    #[test]
    fn user_only_selects_the_user_bundle_and_destination() {
        let root = TestRoot::new("user-only");
        let user_app = write_app(&root.home().join("Applications"), "ChatGPT.app", BUNDLE_ID);

        let resolution = resolve(
            &root.home(),
            &root.system_applications(),
            MacosManagedApp::ChatGptDesktop,
        );

        assert_eq!(resolution.scope, MacosInstallScope::User);
        assert_eq!(resolution.system_app, None);
        assert_eq!(resolution.user_app.as_deref(), Some(user_app.as_path()));
        assert_eq!(
            resolution.preferred_app.as_deref(),
            Some(user_app.as_path())
        );
        assert_eq!(resolution.preferred_destination, user_app);
        assert_eq!(resolution.ordered_candidates, vec![user_app]);
        assert!(!resolution.duplicate_user_install);
    }

    #[test]
    fn duplicate_install_prefers_system_and_reports_the_user_copy() {
        let root = TestRoot::new("both");
        let system_app = write_app(&root.system_applications(), "ChatGPT.app", BUNDLE_ID);
        let user_app = write_app(&root.home().join("Applications"), "Codex.app", BUNDLE_ID);

        let resolution = resolve(
            &root.home(),
            &root.system_applications(),
            MacosManagedApp::ChatGptDesktop,
        );

        assert_eq!(resolution.scope, MacosInstallScope::System);
        assert_eq!(
            resolution.preferred_app.as_deref(),
            Some(system_app.as_path())
        );
        assert_eq!(resolution.preferred_destination, system_app);
        assert_eq!(resolution.ordered_candidates, vec![system_app, user_app]);
        assert!(resolution.duplicate_user_install);
    }

    #[test]
    fn missing_install_uses_the_primary_system_destination() {
        let root = TestRoot::new("neither");

        let resolution = resolve(
            &root.home(),
            &root.system_applications(),
            MacosManagedApp::ChatGptDesktop,
        );

        assert_eq!(resolution.scope, MacosInstallScope::System);
        assert_eq!(resolution.system_app, None);
        assert_eq!(resolution.user_app, None);
        assert_eq!(resolution.preferred_app, None);
        assert_eq!(
            resolution.preferred_destination,
            root.system_applications().join("ChatGPT.app")
        );
        assert!(resolution.ordered_candidates.is_empty());
        assert!(!resolution.duplicate_user_install);
    }

    #[test]
    fn aliases_are_checked_in_declared_order() {
        let root = TestRoot::new("aliases");
        let chatgpt_app = write_app(&root.system_applications(), "ChatGPT.app", BUNDLE_ID);
        let codex_app = write_app(&root.system_applications(), "Codex.app", BUNDLE_ID);
        let openai_codex_app =
            write_app(&root.system_applications(), "OpenAI Codex.app", BUNDLE_ID);
        let dotted_openai_codex_app =
            write_app(&root.system_applications(), "OpenAI.Codex.app", BUNDLE_ID);

        let resolution = resolve(
            &root.home(),
            &root.system_applications(),
            MacosManagedApp::ChatGptDesktop,
        );

        assert_eq!(
            resolution.system_app.as_deref(),
            Some(chatgpt_app.as_path())
        );
        assert_eq!(
            resolution.ordered_candidates,
            vec![
                chatgpt_app,
                codex_app,
                openai_codex_app,
                dotted_openai_codex_app
            ]
        );
    }

    #[test]
    fn mismatched_bundle_identifiers_are_not_valid_installations() {
        let root = TestRoot::new("bundle-id");
        write_app(
            &root.system_applications(),
            "ChatGPT.app",
            "com.example.impostor",
        );
        let valid_alias = write_app(&root.system_applications(), "Codex.app", BUNDLE_ID);
        write_app(
            &root.home().join("Applications"),
            "ChatGPT.app",
            "com.example.impostor",
        );

        let resolution = resolve(
            &root.home(),
            &root.system_applications(),
            MacosManagedApp::ChatGptDesktop,
        );

        assert_eq!(
            resolution.system_app.as_deref(),
            Some(valid_alias.as_path())
        );
        assert_eq!(resolution.user_app, None);
        assert_eq!(resolution.ordered_candidates, vec![valid_alias]);
        assert!(!resolution.duplicate_user_install);
    }

    #[test]
    fn managed_apps_accept_only_their_fixed_names_and_bundle_identifiers() {
        let root = TestRoot::new("managed-identities");
        let codestudio = write_app(
            &root.system_applications(),
            "CodeStudio Lite.app",
            "com.codestudio.lite",
        );
        let claude = write_app(
            &root.home().join("Applications"),
            "Claude.app",
            "com.anthropic.claudefordesktop",
        );

        let codestudio_resolution = resolve(
            &root.home(),
            &root.system_applications(),
            MacosManagedApp::CodeStudioLite,
        );
        let claude_resolution = resolve(
            &root.home(),
            &root.system_applications(),
            MacosManagedApp::ClaudeDesktop,
        );
        let chatgpt_resolution = resolve(
            &root.home(),
            &root.system_applications(),
            MacosManagedApp::ChatGptDesktop,
        );

        assert_eq!(
            codestudio_resolution.preferred_app.as_deref(),
            Some(codestudio.as_path())
        );
        assert_eq!(
            claude_resolution.preferred_app.as_deref(),
            Some(claude.as_path())
        );
        assert_eq!(chatgpt_resolution.preferred_app, None);
    }

    #[test]
    fn managed_app_ids_use_the_closed_api_values() {
        assert_eq!(
            serde_json::to_string(&MacosManagedApp::CodeStudioLite).unwrap(),
            r#""codestudio-lite""#
        );
        assert_eq!(
            serde_json::to_string(&MacosManagedApp::ChatGptDesktop).unwrap(),
            r#""chatgpt-desktop""#
        );
        assert_eq!(
            serde_json::to_string(&MacosManagedApp::ClaudeDesktop).unwrap(),
            r#""claude-desktop""#
        );
        assert!(serde_json::from_str::<MacosManagedApp>(r#""other-app""#).is_err());
    }

    #[test]
    fn scope_status_reports_both_copies_and_the_running_user_bundle() {
        let root = TestRoot::new("scope-status");
        let system_app = write_app(
            &root.system_applications(),
            "CodeStudio Lite.app",
            "com.codestudio.lite",
        );
        let user_app = write_app(
            &root.home().join("Applications"),
            "CodeStudio Lite.app",
            "com.codestudio.lite",
        );
        let current_executable = user_app.join("Contents/MacOS/codestudio-lite");

        let status = status_for_roots(
            &root.home(),
            &root.system_applications(),
            MacosManagedApp::CodeStudioLite,
            Some(&current_executable),
        );

        assert_eq!(status.app_id, MacosManagedApp::CodeStudioLite);
        assert_eq!(status.system_app.as_deref(), Some(system_app.as_path()));
        assert_eq!(status.user_apps, vec![user_app.clone()]);
        assert_eq!(status.preferred_app.as_deref(), Some(system_app.as_path()));
        assert!(status.duplicate_user_install);
        assert_eq!(status.running_app.as_deref(), Some(user_app.as_path()));
        assert_eq!(status.running_scope, Some(MacosInstallScope::User));
    }

    #[test]
    fn managed_cleanup_moves_every_verified_user_alias_to_trash_and_refreshes_status() {
        let root = TestRoot::new("managed-cleanup");
        let system_app = write_app(
            &root.system_applications(),
            "ChatGPT.app",
            "com.openai.codex",
        );
        let user_chatgpt = write_app(
            &root.home().join("Applications"),
            "ChatGPT.app",
            "com.openai.codex",
        );
        let user_codex = write_app(
            &root.home().join("Applications"),
            "OpenAI Codex.app",
            "com.openai.codex",
        );

        let result = cleanup_managed_user_bundles_for_roots(
            &root.home(),
            &root.system_applications(),
            MacosManagedApp::ChatGptDesktop,
        )
        .unwrap();

        assert_eq!(result.moved_to_trash.len(), 2);
        assert!(result
            .moved_to_trash
            .iter()
            .all(|path| path.starts_with(root.home().join(".Trash"))));
        assert!(!user_chatgpt.exists());
        assert!(!user_codex.exists());
        assert_eq!(
            result.status.preferred_app.as_deref(),
            Some(system_app.as_path())
        );
        assert!(!result.status.duplicate_user_install);
        assert!(result.status.user_apps.is_empty());
    }

    #[test]
    fn managed_cleanup_refuses_to_delete_the_only_valid_user_copy() {
        let root = TestRoot::new("managed-cleanup-user-only");
        let user_app = write_app(
            &root.home().join("Applications"),
            "Claude.app",
            "com.anthropic.claudefordesktop",
        );

        let error = cleanup_managed_user_bundles_for_roots(
            &root.home(),
            &root.system_applications(),
            MacosManagedApp::ClaudeDesktop,
        )
        .unwrap_err();

        assert!(error.contains("/Applications"));
        assert!(user_app.exists());
    }

    #[test]
    fn staged_cleanup_restores_user_bundle_when_exact_system_copy_disappears() {
        let root = TestRoot::new("system-disappears-after-stage");
        let system_app = write_app(
            &root.system_applications(),
            "Claude.app",
            "com.anthropic.claudefordesktop",
        );
        let user_app = write_app(
            &root.home().join("Applications"),
            "Claude.app",
            "com.anthropic.claudefordesktop",
        );
        let staged =
            stage_user_bundle_for_trash(&root.home(), MacosManagedApp::ClaudeDesktop).unwrap();
        fs::remove_dir_all(&system_app).unwrap();

        let error = finalize_staged_user_bundles_for_roots_with(
            &root.home(),
            &root.system_applications(),
            MacosManagedApp::ClaudeDesktop,
            &system_app,
            vec![staged],
            |_| Ok(()),
        )
        .unwrap_err();

        assert!(error.contains("/Applications"));
        assert!(user_app.exists());
        assert!(root
            .home()
            .join("Applications")
            .read_dir()
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".codestudio-lite-trash-staging-")));
    }

    #[test]
    fn self_cleanup_helper_plan_uses_the_exact_staged_executable_and_arguments() {
        let root = TestRoot::new("helper-plan");
        let system_app = write_app(
            &root.system_applications(),
            "CodeStudio Lite.app",
            "com.codestudio.lite",
        );
        let user_app = write_app(
            &root.home().join("Applications"),
            "CodeStudio Lite.app",
            "com.codestudio.lite",
        );
        let current_executable = user_app.join("Contents/MacOS/codestudio-lite");
        let staged =
            stage_user_bundle_for_trash(&root.home(), MacosManagedApp::CodeStudioLite).unwrap();

        let plan = codestudio_self_cleanup_helper_execution_plan(
            4242,
            &root.home(),
            &root.system_applications(),
            &current_executable,
            &system_app,
            &staged,
        )
        .unwrap();

        assert_eq!(
            plan.executable,
            staged.staged_path().join("Contents/MacOS/codestudio-lite")
        );
        assert_eq!(
            plan.args,
            vec![
                std::ffi::OsString::from("--codestudio-self-cleanup-helper"),
                std::ffi::OsString::from("4242"),
                root.home().into_os_string(),
                root.system_applications().into_os_string(),
                staged.original_path().as_os_str().to_os_string(),
                staged.staged_path().as_os_str().to_os_string(),
                system_app.into_os_string(),
            ]
        );
        restore_staged_user_bundle(staged).unwrap();
    }

    #[test]
    fn self_cleanup_open_failure_restores_and_relaunches_exact_user_bundle_with_failure_state() {
        let root = TestRoot::new("helper-open-rollback");
        let system_app = write_app(
            &root.system_applications(),
            "CodeStudio Lite.app",
            "com.codestudio.lite",
        );
        let user_app = write_app(
            &root.home().join("Applications"),
            "CodeStudio Lite.app",
            "com.codestudio.lite",
        );
        let staged =
            stage_user_bundle_for_trash(&root.home(), MacosManagedApp::CodeStudioLite).unwrap();
        let request = CodestudioSelfCleanupHelperRequest {
            parent_pid: 4242,
            home: root.home(),
            system_applications: root.system_applications(),
            original_path: staged.original_path().to_path_buf(),
            staged_path: staged.staged_path().to_path_buf(),
            system_app: system_app.clone(),
        };
        let mut launched = Vec::new();

        let error = complete_codestudio_self_cleanup_with(
            request,
            |_| Ok(()),
            |path| {
                launched.push(path.to_path_buf());
                if path == system_app {
                    Err("open system failed".to_string())
                } else {
                    Ok(())
                }
            },
        )
        .unwrap_err();

        assert!(error.contains("open system failed"));
        assert_eq!(launched, vec![system_app, user_app.clone()]);
        assert!(user_app.exists());
        let failure = fs::read_to_string(
            root.home()
                .join(".codestudio-lite/macos-self-cleanup-failure.json"),
        )
        .unwrap();
        assert!(failure.contains("open system failed"));
        assert!(failure.contains(&user_app.to_string_lossy().to_string()));
    }

    #[test]
    fn self_cleanup_system_disappearance_restores_relaunches_and_persists_user_path() {
        let root = TestRoot::new("helper-system-disappears");
        let system_app = write_app(
            &root.system_applications(),
            "CodeStudio Lite.app",
            "com.codestudio.lite",
        );
        let user_app = write_app(
            &root.home().join("Applications"),
            "CodeStudio Lite.app",
            "com.codestudio.lite",
        );
        let staged =
            stage_user_bundle_for_trash(&root.home(), MacosManagedApp::CodeStudioLite).unwrap();
        let request = CodestudioSelfCleanupHelperRequest {
            parent_pid: 4242,
            home: root.home(),
            system_applications: root.system_applications(),
            original_path: staged.original_path().to_path_buf(),
            staged_path: staged.staged_path().to_path_buf(),
            system_app: system_app.clone(),
        };
        fs::remove_dir_all(system_app).unwrap();
        let mut launched = Vec::new();

        let error = complete_codestudio_self_cleanup_with(
            request,
            |_| Ok(()),
            |path| {
                launched.push(path.to_path_buf());
                Ok(())
            },
        )
        .unwrap_err();

        assert!(error.contains("disappeared or changed"));
        assert!(user_app.exists());
        assert_eq!(launched, vec![user_app.clone()]);
        let failure = take_codestudio_self_cleanup_failure(&root.home())
            .unwrap()
            .unwrap();
        assert_eq!(
            failure.restored_user_app.as_deref(),
            Some(user_app.as_path())
        );
    }

    #[test]
    fn self_cleanup_drain_failure_restores_relaunches_and_persists_user_path() {
        let root = TestRoot::new("helper-drain-fails");
        let system_app = write_app(
            &root.system_applications(),
            "CodeStudio Lite.app",
            "com.codestudio.lite",
        );
        let user_app = write_app(
            &root.home().join("Applications"),
            "CodeStudio Lite.app",
            "com.codestudio.lite",
        );
        let staged =
            stage_user_bundle_for_trash(&root.home(), MacosManagedApp::CodeStudioLite).unwrap();
        let request = CodestudioSelfCleanupHelperRequest {
            parent_pid: 4242,
            home: root.home(),
            system_applications: root.system_applications(),
            original_path: staged.original_path().to_path_buf(),
            staged_path: staged.staged_path().to_path_buf(),
            system_app,
        };
        let mut launched = Vec::new();

        let error = complete_codestudio_self_cleanup_with(
            request,
            |_| Err("exact process drain failed".to_string()),
            |path| {
                launched.push(path.to_path_buf());
                Ok(())
            },
        )
        .unwrap_err();

        assert!(error.contains("exact process drain failed"));
        assert!(user_app.exists());
        assert_eq!(launched, vec![user_app.clone()]);
        let failure = take_codestudio_self_cleanup_failure(&root.home())
            .unwrap()
            .unwrap();
        assert_eq!(
            failure.restored_user_app.as_deref(),
            Some(user_app.as_path())
        );
    }

    #[test]
    fn self_cleanup_finalize_failure_restores_relaunches_and_persists_user_path() {
        let root = TestRoot::new("helper-finalize-fails");
        let system_app = write_app(
            &root.system_applications(),
            "CodeStudio Lite.app",
            "com.codestudio.lite",
        );
        let user_app = write_app(
            &root.home().join("Applications"),
            "CodeStudio Lite.app",
            "com.codestudio.lite",
        );
        let staged =
            stage_user_bundle_for_trash(&root.home(), MacosManagedApp::CodeStudioLite).unwrap();
        fs::write(root.home().join(".Trash"), b"not-a-directory").unwrap();
        let request = CodestudioSelfCleanupHelperRequest {
            parent_pid: 4242,
            home: root.home(),
            system_applications: root.system_applications(),
            original_path: staged.original_path().to_path_buf(),
            staged_path: staged.staged_path().to_path_buf(),
            system_app,
        };
        let mut launched = Vec::new();

        let error = complete_codestudio_self_cleanup_with(
            request,
            |_| Ok(()),
            |path| {
                launched.push(path.to_path_buf());
                Ok(())
            },
        )
        .unwrap_err();

        assert!(error.contains("Trash directory is not a real directory"));
        assert!(user_app.exists());
        assert_eq!(launched, vec![user_app.clone()]);
        let failure = take_codestudio_self_cleanup_failure(&root.home())
            .unwrap()
            .unwrap();
        assert_eq!(
            failure.restored_user_app.as_deref(),
            Some(user_app.as_path())
        );
    }

    #[test]
    fn self_cleanup_parent_timeout_restores_relaunches_and_persists_user_path_without_sleeping() {
        use std::cell::Cell;

        let root = TestRoot::new("helper-parent-timeout");
        let system_app = write_app(
            &root.system_applications(),
            "CodeStudio Lite.app",
            "com.codestudio.lite",
        );
        let user_app = write_app(
            &root.home().join("Applications"),
            "CodeStudio Lite.app",
            "com.codestudio.lite",
        );
        let staged =
            stage_user_bundle_for_trash(&root.home(), MacosManagedApp::CodeStudioLite).unwrap();
        let request = CodestudioSelfCleanupHelperRequest {
            parent_pid: 4242,
            home: root.home(),
            system_applications: root.system_applications(),
            original_path: staged.original_path().to_path_buf(),
            staged_path: staged.staged_path().to_path_buf(),
            system_app,
        };
        let now = Cell::new(Duration::ZERO);
        let mut launched = Vec::new();

        let error = run_codestudio_self_cleanup_helper_request_with(
            request,
            Duration::from_secs(3),
            || now.get(),
            |_| true,
            || now.set(now.get() + Duration::from_secs(1)),
            |_| panic!("process drain must not run after parent timeout"),
            |path| {
                launched.push(path.to_path_buf());
                Ok(())
            },
        )
        .unwrap_err();

        assert!(error.contains("did not exit in time"));
        assert!(user_app.exists());
        assert_eq!(launched, vec![user_app.clone()]);
        let failure = take_codestudio_self_cleanup_failure(&root.home())
            .unwrap()
            .unwrap();
        assert_eq!(
            failure.restored_user_app.as_deref(),
            Some(user_app.as_path())
        );
    }

    #[test]
    fn persisted_self_cleanup_failure_is_delivered_once_and_cleared_after_valid_read() {
        let root = TestRoot::new("take-helper-failure");
        let expected = CodestudioSelfCleanupFailure {
            message: "system open failed".to_string(),
            restored_user_app: Some(root.home().join("Applications").join("CodeStudio Lite.app")),
            system_app: root.system_applications().join("CodeStudio Lite.app"),
        };
        persist_codestudio_self_cleanup_failure(&root.home(), &expected).unwrap();

        assert_eq!(
            take_codestudio_self_cleanup_failure(&root.home()).unwrap(),
            Some(expected)
        );
        assert_eq!(
            take_codestudio_self_cleanup_failure(&root.home()).unwrap(),
            None
        );
    }

    #[test]
    fn malformed_self_cleanup_failure_is_not_cleared() {
        let root = TestRoot::new("malformed-helper-failure");
        let path = root
            .home()
            .join(".codestudio-lite/macos-self-cleanup-failure.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"{not-json").unwrap();

        assert!(take_codestudio_self_cleanup_failure(&root.home()).is_err());
        assert!(path.exists());
    }

    #[test]
    fn self_cleanup_uses_post_exit_mode_only_when_running_from_the_user_copy() {
        let root = TestRoot::new("self-cleanup-plan");
        let system_app = write_app(
            &root.system_applications(),
            "CodeStudio Lite.app",
            "com.codestudio.lite",
        );
        let user_app = write_app(
            &root.home().join("Applications"),
            "CodeStudio Lite.app",
            "com.codestudio.lite",
        );

        assert_eq!(
            plan_codestudio_self_cleanup_for_roots(
                &root.home(),
                &root.system_applications(),
                &user_app.join("Contents/MacOS/codestudio-lite"),
            )
            .unwrap(),
            CodestudioSelfCleanupMode::PostExit {
                user_app: user_app.clone(),
                system_app: system_app.clone(),
            }
        );
        assert_eq!(
            plan_codestudio_self_cleanup_for_roots(
                &root.home(),
                &root.system_applications(),
                &system_app.join("Contents/MacOS/codestudio-lite"),
            )
            .unwrap(),
            CodestudioSelfCleanupMode::Direct {
                user_app,
                system_app,
            }
        );
    }

    #[test]
    fn binary_info_plist_bundle_identifier_is_supported() {
        let root = TestRoot::new("binary-plist");
        let app = write_binary_app(&root.system_applications(), "ChatGPT.app", BUNDLE_ID);

        let resolution = resolve(
            &root.home(),
            &root.system_applications(),
            MacosManagedApp::ChatGptDesktop,
        );

        assert_eq!(resolution.preferred_app.as_deref(), Some(app.as_path()));
    }

    #[test]
    fn non_string_bundle_identifier_cannot_match_a_later_string() {
        let root = TestRoot::new("non-string-bundle-id");
        let app = root
            .system_applications()
            .join("ChatGPT.app")
            .join("Contents");
        fs::create_dir_all(&app).expect("app Contents should be created");
        fs::write(
            app.join("Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <integer>1</integer>
  <key>Unrelated</key>
  <string>com.openai.codex</string>
</dict>
</plist>
"#,
        )
        .expect("malformed identity plist should be written");

        let resolution = resolve(
            &root.home(),
            &root.system_applications(),
            MacosManagedApp::ChatGptDesktop,
        );

        assert_eq!(resolution.preferred_app, None);
        assert!(resolution.ordered_candidates.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_user_applications_parent_is_omitted_from_candidates() {
        use std::os::unix::fs::symlink;

        let root = TestRoot::new("parent-symlink");
        let real_applications = root.path.join("real-user-applications");
        let linked_applications = root.home().join("Applications");
        fs::create_dir_all(root.home()).expect("home should be created");
        write_app(&real_applications, "ChatGPT.app", BUNDLE_ID);
        symlink(&real_applications, &linked_applications)
            .expect("user Applications symlink should be created");

        let resolution = resolve(
            &root.home(),
            &root.system_applications(),
            MacosManagedApp::ChatGptDesktop,
        );

        assert_eq!(resolution.user_app, None);
        assert!(resolution.ordered_candidates.is_empty());
        assert_eq!(resolution.scope, MacosInstallScope::System);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_app_bundle_is_not_a_valid_installation() {
        use std::os::unix::fs::symlink;

        let root = TestRoot::new("app-symlink");
        let real_app = write_app(&root.path.join("elsewhere"), "ChatGPT.app", BUNDLE_ID);
        fs::create_dir_all(root.home().join("Applications"))
            .expect("user Applications should be created");
        symlink(
            &real_app,
            root.home().join("Applications").join("ChatGPT.app"),
        )
        .expect("app bundle symlink should be created");

        let resolution = resolve(
            &root.home(),
            &root.system_applications(),
            MacosManagedApp::ChatGptDesktop,
        );

        assert_eq!(resolution.user_app, None);
        assert!(resolution.ordered_candidates.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_info_plist_is_not_a_valid_installation() {
        use std::os::unix::fs::symlink;

        let root = TestRoot::new("plist-symlink");
        let app = root
            .home()
            .join("Applications")
            .join("ChatGPT.app")
            .join("Contents");
        fs::create_dir_all(&app).expect("app Contents should be created");
        let external_plist = root.path.join("external.plist");
        fs::write(
            &external_plist,
            "<key>CFBundleIdentifier</key><string>com.openai.chat</string>",
        )
        .expect("external plist should be written");
        symlink(&external_plist, app.join("Info.plist"))
            .expect("Info.plist symlink should be created");

        let resolution = resolve(
            &root.home(),
            &root.system_applications(),
            MacosManagedApp::ChatGptDesktop,
        );

        assert_eq!(resolution.user_app, None);
        assert!(resolution.ordered_candidates.is_empty());
    }

    #[test]
    fn staged_user_bundle_can_be_restored_to_its_allowlisted_location() {
        let root = TestRoot::new("restore");
        let user_app = write_app(&root.home().join("Applications"), "ChatGPT.app", BUNDLE_ID);

        let staged = stage_user_bundle_for_trash(&root.home(), MacosManagedApp::ChatGptDesktop)
            .expect("valid user bundle should stage");
        assert!(!user_app.exists());
        assert!(staged.staged_path().exists());

        restore_staged_user_bundle(staged).expect("staged bundle should restore");

        assert!(user_app.exists());
    }

    #[test]
    fn staged_user_bundle_can_be_finalized_into_home_trash() {
        let root = TestRoot::new("finalize-trash");
        let user_app = write_app(&root.home().join("Applications"), "ChatGPT.app", BUNDLE_ID);

        let staged = stage_user_bundle_for_trash(&root.home(), MacosManagedApp::ChatGptDesktop)
            .expect("valid user bundle should stage");
        let trash_path = finalize_staged_user_bundle_to_trash(staged)
            .expect("staged bundle should move to Trash");

        assert!(!user_app.exists());
        assert_eq!(
            trash_path.parent(),
            Some(root.home().join(".Trash").as_path())
        );
        assert!(trash_path.exists());
        assert!(trash_path.join("Contents").join("Info.plist").is_file());
    }

    #[test]
    fn finalize_validation_failure_restores_the_staged_bundle() {
        let root = TestRoot::new("validation-restore");
        let user_app = write_app(&root.home().join("Applications"), "ChatGPT.app", BUNDLE_ID);
        let staged = stage_user_bundle_for_trash(&root.home(), MacosManagedApp::ChatGptDesktop)
            .expect("valid user bundle should stage");
        let staged_path = staged.staged_path().to_path_buf();
        fs::write(
            staged_path.join("Contents").join("Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>com.example.changed</string>
</dict>
</plist>
"#,
        )
        .expect("staged Info.plist should be changed");

        let error = finalize_staged_user_bundle_to_trash(staged)
            .expect_err("changed bundle identifier must fail validation");

        assert!(error.contains("no longer valid"));
        assert!(user_app.exists());
        assert!(!staged_path.exists());
    }

    #[test]
    fn atomic_noreplace_rename_never_overwrites_an_existing_entry() {
        let root = TestRoot::new("atomic-noreplace");
        let source = root.path.join("source");
        let destination = root.path.join("destination");
        fs::create_dir(&source).expect("source should be created");
        fs::write(source.join("source-marker"), "source").expect("source marker should be written");
        fs::create_dir(&destination).expect("destination should be created");
        fs::write(destination.join("destination-marker"), "destination")
            .expect("destination marker should be written");

        let error = atomic_rename_noreplace(&source, &destination)
            .expect_err("occupied destination must not be replaced");

        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert!(source.join("source-marker").is_file());
        assert!(destination.join("destination-marker").is_file());
    }

    #[test]
    fn occupied_staging_name_is_preserved_and_the_next_name_is_used() {
        let root = TestRoot::new("staging-collision");
        let applications = root.home().join("Applications");
        let user_app = write_app(&applications, "ChatGPT.app", BUNDLE_ID);
        let occupied = applications.join(".codestudio-lite-trash-staging-ChatGPT.app");
        fs::create_dir(&occupied).expect("occupied staging entry should be created");
        fs::write(occupied.join("marker"), "keep")
            .expect("occupied staging marker should be written");

        let staged = stage_user_bundle_for_trash(&root.home(), MacosManagedApp::ChatGptDesktop)
            .expect("valid user bundle should stage without overwriting");

        assert_eq!(
            staged.staged_path(),
            applications.join(".codestudio-lite-trash-staging-ChatGPT 2.app")
        );
        assert_eq!(
            fs::read_to_string(occupied.join("marker"))
                .expect("occupied staging marker should remain"),
            "keep"
        );
        assert!(!user_app.exists());
    }

    #[test]
    fn occupied_trash_name_is_preserved_and_the_next_name_is_used() {
        let root = TestRoot::new("trash-collision");
        write_app(&root.home().join("Applications"), "ChatGPT.app", BUNDLE_ID);
        let trash = root.home().join(".Trash");
        let occupied = trash.join("ChatGPT.app");
        fs::create_dir_all(&occupied).expect("occupied Trash entry should be created");
        fs::write(occupied.join("marker"), "keep")
            .expect("occupied Trash marker should be written");
        let staged = stage_user_bundle_for_trash(&root.home(), MacosManagedApp::ChatGptDesktop)
            .expect("valid user bundle should stage");

        let trash_path = finalize_staged_user_bundle_to_trash(staged)
            .expect("staged bundle should use the next Trash name");

        assert_eq!(trash_path, trash.join("ChatGPT 2.app"));
        assert_eq!(
            fs::read_to_string(occupied.join("marker"))
                .expect("occupied Trash marker should remain"),
            "keep"
        );
        assert!(trash_path.exists());
    }

    #[test]
    fn convenience_cleanup_moves_only_the_valid_allowlisted_user_bundle() {
        let root = TestRoot::new("move-trash");
        let user_app = write_app(&root.home().join("Applications"), "Codex.app", BUNDLE_ID);
        let unrelated_app = write_app(
            &root.home().join("Applications"),
            "Unrelated.app",
            "com.example.unrelated",
        );

        let trash_path = move_user_bundle_to_trash(&root.home(), MacosManagedApp::ChatGptDesktop)
            .expect("valid allowlisted user bundle should move to Trash");

        assert!(!user_app.exists());
        assert!(trash_path.exists());
        assert!(unrelated_app.exists());
    }

    #[cfg(unix)]
    #[test]
    fn failed_trash_preparation_restores_the_staged_user_bundle() {
        use std::os::unix::fs::symlink;

        let root = TestRoot::new("trash-restore");
        let user_app = write_app(&root.home().join("Applications"), "ChatGPT.app", BUNDLE_ID);
        symlink(
            root.path.join("missing-trash-target"),
            root.home().join(".Trash"),
        )
        .expect("dangling Trash symlink should be created");
        let staged = stage_user_bundle_for_trash(&root.home(), MacosManagedApp::ChatGptDesktop)
            .expect("valid user bundle should stage");
        let staged_path = staged.staged_path().to_path_buf();

        let error = finalize_staged_user_bundle_to_trash(staged)
            .expect_err("unsafe Trash path must be rejected");

        assert!(error.contains("Trash"));
        assert!(user_app.exists());
        assert!(!staged_path.exists());
    }
}
