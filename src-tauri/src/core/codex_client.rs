use crate::core::activity_log;
use crate::core::app_paths::{app_paths, display_path, ensure_dirs};
use crate::core::platform::{hidden_command, package, run_powershell};
use crate::core::process_control;
use crate::core::types::{ConfigState, InstallState, Severity, ToolCategory, ToolStatus};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use zip::ZipArchive;

const DEFAULT_MIRROR_BASE: &str = "https://codexapp.agentsmirror.com";
const OFFICIAL_MACOS_ARM64_URL: &str = "https://persistent.oaistatic.com/codex-app-prod/Codex.dmg";
const OFFICIAL_MACOS_X64_URL: &str =
    "https://persistent.oaistatic.com/codex-app-prod/Codex-latest-x64.dmg";
const PACKAGE_IDENTITY: &str = "OpenAI.Codex";
const CODEX_DISPLAY_NAME: &str = "Codex";
const CODEX_PUBLISHER: &str = "OpenAI";
const CODEX_EXE_NAME: &str = "Codex.exe";
const CODEX_MACOS_APP_NAME: &str = "Codex.app";
const CODEX_SHORTCUT_NAME: &str = "Codex.lnk";
const CODEX_UNINSTALL_KEY: &str =
    r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Codex";
const CODEX_MACOS_BUNDLE_ID: &str = "com.openai.codex";
pub const CODEX_CLIENT_PROGRESS_EVENT: &str = "codex-client://progress";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexClientSettings {
    pub source: String,
    pub custom_url: String,
    pub auto_check: bool,
    pub ask_before: bool,
    pub signed_only: bool,
    pub windows_install_mode: String,
    pub install_root: String,
    pub keep_user_data_on_uninstall: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCodexClientSettingsRequest {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub custom_url: Option<String>,
    #[serde(default)]
    pub auto_check: Option<bool>,
    #[serde(default)]
    pub ask_before: Option<bool>,
    #[serde(default)]
    pub windows_install_mode: Option<String>,
    #[serde(default)]
    pub install_root: Option<String>,
    #[serde(default)]
    pub keep_user_data_on_uninstall: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledCodexClient {
    pub path: String,
    pub version: String,
    pub arch: Option<String>,
    pub source: String,
    pub package_family_name: Option<String>,
    pub installed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexClientRelease {
    pub version: String,
    pub package_moniker: String,
    pub architecture: Option<String>,
    pub package_kind: String,
    pub package_source: String,
    pub content_length: Option<u64>,
    pub etag: Option<String>,
    pub package_identity: Option<String>,
    pub package_url: String,
    pub checksums_url: String,
    pub manifest_url: String,
    pub sha256: String,
    pub macos_arm64_version: Option<String>,
    pub macos_x64_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexClientCapability {
    pub id: String,
    pub label: String,
    pub status: Severity,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexClientPlan {
    pub up_to_date: bool,
    pub current_version: Option<String>,
    pub latest_version: String,
    pub route: String,
    pub package_url: String,
    pub download_size: Option<u64>,
    pub sha256: String,
    pub staged_path: Option<String>,
    pub install_root: Option<String>,
    pub warnings: Vec<String>,
    pub capabilities: Vec<CodexClientCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexClientState {
    pub generated_at: String,
    pub platform: String,
    pub settings: CodexClientSettings,
    pub installed: Option<InstalledCodexClient>,
    pub install_class: String,
    pub release: Option<CodexClientRelease>,
    pub plan: Option<CodexClientPlan>,
    pub staging_dir: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexClientStageReport {
    pub up_to_date: bool,
    pub staged_path: Option<String>,
    pub package_moniker: String,
    pub download_size: u64,
    pub sha256: String,
    pub hash_verified: bool,
    pub route: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexClientProgress {
    pub phase: String,
    pub message: String,
    pub downloaded: Option<u64>,
    pub total: Option<u64>,
    pub percent: Option<f64>,
    pub step: Option<u64>,
    pub step_total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexClientOperationResult {
    pub success: bool,
    pub action: String,
    pub message: String,
    pub installed: Option<InstalledCodexClient>,
    pub stage: Option<CodexClientStageReport>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexClientUninstallRequest {
    pub confirm: bool,
    #[serde(default)]
    pub purge_user_data: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexClientInstallRequest {
    pub confirm: bool,
    #[serde(default)]
    pub expected_current_version: Option<String>,
    #[serde(default)]
    pub expected_latest_version: Option<String>,
    #[serde(default)]
    pub expected_route: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedInstallMarker {
    source: String,
    install_root: Option<String>,
    package_family_name: Option<String>,
    version: Option<String>,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MirrorManifest {
    schema_version: u64,
    sources: ManifestSources,
}

#[derive(Debug, Deserialize)]
struct ManifestSources {
    windows: WindowsSource,
    #[serde(default)]
    macos: Option<MacosSources>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowsSource {
    version: String,
    package_moniker: String,
    architecture: Option<String>,
    content_length: Option<u64>,
    etag: Option<String>,
    product_id: Option<String>,
    update_manifest: Option<WindowsUpdateManifest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowsUpdateManifest {
    package_identity: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MacosSources {
    #[serde(default)]
    arm64: Option<MacosSource>,
    #[serde(default)]
    x64: Option<MacosSource>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacosSource {
    url: Option<String>,
    content_length: Option<u64>,
    etag: Option<String>,
    sha256: Option<String>,
    bundle_short_version: Option<String>,
    bundle_version: Option<String>,
    bundle_identifier: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MsixIdentity {
    name: String,
    publisher: String,
    version: String,
    processor_architecture: String,
}

impl Default for CodexClientSettings {
    fn default() -> Self {
        Self {
            source: "mirror".to_string(),
            custom_url: String::new(),
            auto_check: true,
            ask_before: true,
            signed_only: true,
            windows_install_mode: "msix".to_string(),
            install_root: default_install_root(),
            keep_user_data_on_uninstall: true,
        }
    }
}

pub fn inspect_state(include_network: bool) -> Result<CodexClientState, String> {
    let settings = load_settings()?;
    let installed = detect_installed(&settings);
    let release = if include_network {
        Some(load_release(&settings)?)
    } else {
        None
    };
    let plan = release
        .as_ref()
        .map(|release| build_plan(&settings, installed.as_ref(), release))
        .transpose()?;
    let install_class = install_class(installed.as_ref());
    let mut notes = vec![
        "Codex 客户端管理复刻 Codex-App-Manager 的安装、更新、卸载、启动和镜像源流程。".to_string(),
        "不会修改 Codex 安装包内容；下载后先做 SHA-256 校验，再进入安装步骤。".to_string(),
    ];
    if cfg!(target_os = "macos") {
        notes.push("macOS 会使用 DMG 安装包并复制 Codex.app 到目标应用目录。".to_string());
    } else if !cfg!(target_os = "windows") {
        notes.push("当前平台暂未提供 Codex 桌面客户端安装执行链路。".to_string());
    }

    Ok(CodexClientState {
        generated_at: Utc::now().to_rfc3339(),
        platform: platform_label(),
        settings,
        installed,
        install_class,
        release,
        plan,
        staging_dir: display_path(&staging_dir()?),
        notes,
    })
}

pub fn plan_update() -> Result<CodexClientState, String> {
    inspect_state(true)
}

pub fn stage_update() -> Result<CodexClientStageReport, String> {
    stage_update_with_progress(|_| {})
}

pub fn stage_update_with_progress<F>(on_progress: F) -> Result<CodexClientStageReport, String>
where
    F: Fn(CodexClientProgress),
{
    emit_step_progress(
        &on_progress,
        "preparing",
        "正在读取镜像 manifest 与 checksums...",
        None,
        None,
        Some(1),
        Some(4),
    );
    let settings = load_settings()?;
    let release = load_release(&settings)?;
    let installed = detect_installed(&settings);
    let plan = build_plan(&settings, installed.as_ref(), &release)?;
    stage_from_plan(&release, &plan, &on_progress)
}

pub fn install_or_update(
    request: CodexClientInstallRequest,
) -> Result<CodexClientOperationResult, String> {
    install_or_update_with_progress(request, |_| {})
}

pub fn install_or_update_with_progress<F>(
    request: CodexClientInstallRequest,
    on_progress: F,
) -> Result<CodexClientOperationResult, String>
where
    F: Fn(CodexClientProgress),
{
    if !request.confirm {
        return Err("拒绝执行：安装或更新 Codex 客户端必须显式确认。".to_string());
    }

    emit_step_progress(
        &on_progress,
        "preparing",
        "正在确认安装状态与更新计划...",
        None,
        None,
        Some(1),
        Some(7),
    );
    let settings = load_settings()?;
    validate_install_target(&settings)?;
    let release = load_release(&settings)?;
    let installed_before = detect_installed(&settings);
    let plan = build_plan(&settings, installed_before.as_ref(), &release)?;

    if let Some(expected) = request.expected_current_version.as_deref() {
        let actual = installed_before.as_ref().map(|item| item.version.as_str());
        if actual != Some(expected) && !(expected.is_empty() && actual.is_none()) {
            return Err(format!(
                "Codex 客户端状态已变化：确认时版本为 {expected}，当前为 {}。请刷新后重试。",
                actual.unwrap_or("未安装")
            ));
        }
    }
    if let Some(expected) = request.expected_latest_version.as_deref() {
        if expected != release.version {
            return Err(format!(
                "镜像最新版本已变化：确认时为 {expected}，当前为 {}。请刷新后重试。",
                release.version
            ));
        }
    }
    if let Some(expected) = request.expected_route.as_deref() {
        if expected != plan.route {
            return Err(format!(
                "安装方式已变化：确认时为 {expected}，当前为 {}。请刷新后重试。",
                plan.route
            ));
        }
    }

    if plan.up_to_date {
        emit_step_progress(
            &on_progress,
            "done",
            "Codex 客户端已经是最新版本。",
            Some(1),
            Some(1),
            Some(7),
            Some(7),
        );
        return Ok(CodexClientOperationResult {
            success: true,
            action: "none".to_string(),
            message: "Codex 客户端已经是最新版本。".to_string(),
            installed: installed_before,
            stage: None,
            notes: Vec::new(),
        });
    }

    let mut stage = stage_from_plan(&release, &plan, &on_progress)?;
    let staged_path = stage
        .staged_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| "没有可安装的暂存文件。".to_string())?;
    let mut notes = stage.notes.clone();
    if plan.route == "unsupported" {
        return Err("当前平台暂未提供 Codex 桌面客户端安装执行链路。".to_string());
    }

    let action = plan.route.clone();
    if let Some(installed) = installed_before.as_ref() {
        if cfg!(target_os = "windows") {
            let mut termination = if installed.source == "msix" {
                process_control::close_appx_package_for_update("Codex 客户端", PACKAGE_IDENTITY)?
            } else {
                process_control::ProcessTerminationReport::default()
            };
            let fallback = process_control::close_processes_for_update(
                "Codex 客户端",
                &["Codex"],
                Some(Path::new(&installed.path)),
            )?;
            termination.total += fallback.total;
            termination.forced += fallback.forced;
            termination.remaining += fallback.remaining;
            if let Some(note) = termination.note("Codex 客户端") {
                notes.push(note);
            }
        } else if cfg!(target_os = "macos") {
            if let Err(err) = package::quit_macos_app(CODEX_DISPLAY_NAME) {
                notes.push(format!("关闭 Codex 客户端失败：{err}"));
            }
        }
    }
    let preserve_existing_msix = installed_before
        .as_ref()
        .map(|item| item.source == "msix")
        .unwrap_or(false);

    let installed = if action == "portable-fallback" {
        emit_step_progress(
            &on_progress,
            "installing",
            "正在安装便携版 Codex 客户端...",
            None,
            None,
            Some(4),
            Some(7),
        );
        let report = install_portable(
            &staged_path,
            &expand_env_path(&settings.install_root)?,
            &on_progress,
        )?;
        notes.extend(report.notes);
        report.installed
    } else if action == "macos-dmg" {
        emit_step_progress(
            &on_progress,
            "installing",
            "正在安装 macOS Codex 客户端...",
            None,
            None,
            Some(4),
            Some(7),
        );
        let report = package::install_macos_dmg(
            &staged_path,
            CODEX_MACOS_APP_NAME,
            &expand_env_path(&settings.install_root)?,
            Some(CODEX_MACOS_BUNDLE_ID),
        )?;
        notes.extend(report.notes);
        report.installed.map(installed_from_macos_app)
    } else {
        emit_step_progress(
            &on_progress,
            "msix-installing",
            "正在执行 MSIX 安装...",
            None,
            None,
            Some(4),
            Some(7),
        );
        match package::install_msix_package(&staged_path, PACKAGE_IDENTITY) {
            Ok(report) if report.success => report
                .installed
                .map(installed_from_msix)
                .or_else(|| detect_installed(&settings)),
            Ok(report) => {
                notes.push(format!("MSIX 安装失败：{}", report.message));
                if preserve_existing_msix {
                    return Err(format!("MSIX 更新失败：{}。", report.message));
                }
                notes.push("已自动切换到便携版安装。".to_string());
                emit_step_progress(
                    &on_progress,
                    "portable-fallback",
                    "MSIX 不可用，正在切换到便携版安装...",
                    None,
                    None,
                    Some(5),
                    Some(7),
                );
                let portable = install_portable(
                    &staged_path,
                    &expand_env_path(&settings.install_root)?,
                    &on_progress,
                )?;
                notes.extend(portable.notes);
                portable.installed
            }
            Err(err) => {
                notes.push(format!("MSIX 安装执行失败：{err}"));
                if preserve_existing_msix {
                    return Err(format!("MSIX 更新执行失败：{err}。"));
                }
                notes.push("已自动切换到便携版安装。".to_string());
                emit_step_progress(
                    &on_progress,
                    "portable-fallback",
                    "MSIX 执行失败，正在切换到便携版安装...",
                    None,
                    None,
                    Some(5),
                    Some(7),
                );
                let portable = install_portable(
                    &staged_path,
                    &expand_env_path(&settings.install_root)?,
                    &on_progress,
                )?;
                notes.extend(portable.notes);
                portable.installed
            }
        }
    };

    let installed = installed.or_else(|| detect_installed(&settings));
    if installed.is_some() {
        cleanup_staged_package(&mut stage, &mut notes);
    }
    save_marker(&ManagedInstallMarker {
        source: installed
            .as_ref()
            .map(|item| item.source.clone())
            .unwrap_or_else(|| action.clone()),
        install_root: Some(
            expand_env_path(&settings.install_root)?
                .to_string_lossy()
                .to_string(),
        ),
        package_family_name: installed
            .as_ref()
            .and_then(|item| item.package_family_name.clone()),
        version: installed.as_ref().map(|item| item.version.clone()),
        updated_at: Utc::now().to_rfc3339(),
    })?;
    let _ = activity_log::append(
        Severity::Ok,
        format!(
            "Installed or updated Codex Client to {} via {}.",
            release.version, action
        ),
    );

    emit_step_progress(
        &on_progress,
        "done",
        "Codex 客户端安装流程已完成。",
        Some(1),
        Some(1),
        Some(7),
        Some(7),
    );

    Ok(CodexClientOperationResult {
        success: installed.is_some(),
        action,
        message: installed
            .as_ref()
            .map(|item| format!("Codex 客户端已就绪：{} ({})", item.version, item.source))
            .unwrap_or_else(|| "安装流程结束，但未能重新检测到 Codex 客户端。".to_string()),
        installed,
        stage: Some(stage),
        notes,
    })
}

pub fn uninstall(
    request: CodexClientUninstallRequest,
) -> Result<CodexClientOperationResult, String> {
    if !request.confirm {
        return Err("拒绝执行：卸载 Codex 客户端必须显式确认。".to_string());
    }
    if !cfg!(target_os = "windows") && !cfg!(target_os = "macos") {
        return Err("当前平台暂未提供 Codex 桌面客户端卸载执行链路。".to_string());
    }

    let settings = load_settings()?;
    let installed = detect_installed(&settings);
    let Some(installed_before) = installed else {
        return Ok(CodexClientOperationResult {
            success: true,
            action: "none".to_string(),
            message: "没有检测到可卸载的 Codex 客户端。".to_string(),
            installed: None,
            stage: None,
            notes: Vec::new(),
        });
    };

    let mut notes = Vec::new();
    if cfg!(target_os = "windows") {
        terminate_codex_process_for_uninstall(Some(Path::new(&installed_before.path)), &mut notes)?;
    } else if cfg!(target_os = "macos") {
        if let Err(err) = package::quit_macos_app(CODEX_DISPLAY_NAME) {
            notes.push(format!("关闭 Codex 客户端失败：{err}"));
        }
    }
    let action = if installed_before.source == "portable" {
        if Path::new(&installed_before.path).exists() {
            fs::remove_dir_all(&installed_before.path)
                .map_err(|err| format!("移除便携版目录失败：{err}"))?;
        }
        if let Err(err) = package::remove_portable_start_menu_shortcut(CODEX_SHORTCUT_NAME) {
            notes.push(format!("开始菜单快捷方式清理失败：{err}"));
        }
        if let Err(err) = package::remove_portable_uninstall_entry(CODEX_UNINSTALL_KEY) {
            notes.push(format!("卸载项清理失败：{err}"));
        }
        "remove-portable"
    } else if installed_before.source == "macos" {
        let app_path = Path::new(&installed_before.path);
        if app_path.exists() {
            fs::remove_dir_all(app_path).map_err(|err| format!("移除 macOS 应用失败：{err}"))?;
        }
        "remove-macos"
    } else if installed_before.source == "msix" {
        let report = package::remove_msix_package(PACKAGE_IDENTITY)?;
        if !report.success {
            return Err(report.message);
        }
        notes.extend(report.notes);
        "remove-msix"
    } else {
        return Err(format!(
            "不支持卸载当前 Codex 客户端安装类型：{}。",
            installed_before.source
        ));
    };

    if request.purge_user_data {
        if purge_user_data()? {
            notes.push("已删除 ~/.codex 用户数据。".to_string());
        } else {
            notes.push("未发现 ~/.codex 用户数据目录。".to_string());
        }
    } else {
        notes.push("已保留 ~/.codex 用户数据。".to_string());
    }

    let _ = fs::remove_file(marker_file()?);
    let _ = activity_log::append(Severity::Ok, "Uninstalled Codex Client.");

    Ok(CodexClientOperationResult {
        success: true,
        action: action.to_string(),
        message: "Codex 客户端卸载完成。".to_string(),
        installed: None,
        stage: None,
        notes,
    })
}

pub fn launch() -> Result<(), String> {
    let settings = load_settings()?;
    let installed =
        detect_installed(&settings).ok_or_else(|| "未检测到 Codex 客户端。".to_string())?;
    if installed.source == "portable" {
        let exe = Path::new(&installed.path).join("Codex.exe");
        hidden_command(exe)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("启动 Codex 客户端失败：{err}"))?;
    } else if cfg!(target_os = "windows") {
        launch_msix()?;
    } else if cfg!(target_os = "macos") {
        package::launch_macos_app(Path::new(&installed.path))
            .map_err(|err| format!("启动 Codex 客户端失败：{err}"))?;
    } else {
        return Err("当前平台暂不支持启动 Codex 客户端。".to_string());
    }
    let _ = activity_log::append(Severity::Info, "Launched Codex Client.");
    Ok(())
}

pub fn restart() -> Result<String, String> {
    let settings = load_settings()?;
    let _installed =
        detect_installed(&settings).ok_or_else(|| "未检测到 Codex 客户端。".to_string())?;
    let mut notes = Vec::new();
    terminate_codex_process_for_restart(None, &mut notes)?;
    launch()?;
    let message = if notes.is_empty() {
        "已启动 Codex 客户端。".to_string()
    } else {
        format!("{} 已重新启动 Codex 客户端。", notes.join(" "))
    };
    let _ = activity_log::append(
        Severity::Info,
        "Restarted Codex Client after profile apply.",
    );
    Ok(message)
}

pub fn update_settings(
    request: UpdateCodexClientSettingsRequest,
) -> Result<CodexClientSettings, String> {
    let mut settings = load_settings()?;
    if let Some(source) = request.source {
        settings.source = normalize_source(&source);
    } else {
        settings.source = normalize_source(&settings.source);
    }
    settings.custom_url = String::new();
    if let Some(auto_check) = request.auto_check {
        settings.auto_check = auto_check;
    }
    if let Some(ask_before) = request.ask_before {
        settings.ask_before = ask_before;
    }
    if let Some(mode) = request.windows_install_mode {
        settings.windows_install_mode = if mode == "portable" {
            "portable"
        } else {
            "msix"
        }
        .to_string();
    }
    if let Some(root) = request.install_root {
        let expanded = expand_env_path(&root)?;
        validate_install_path_for_platform(&expanded)?;
        settings.install_root = expanded.to_string_lossy().to_string();
    }
    if let Some(keep) = request.keep_user_data_on_uninstall {
        settings.keep_user_data_on_uninstall = keep;
    }
    settings.signed_only = true;
    save_settings(&settings)?;
    Ok(settings)
}

pub fn open_path(kind: String) -> Result<(), String> {
    let settings = load_settings()?;
    let target = match kind.as_str() {
        "install" => detect_installed(&settings)
            .map(|installed| PathBuf::from(installed.path))
            .unwrap_or(expand_env_path(&settings.install_root)?),
        "staging" => staging_dir()?,
        "config" => app_paths()
            .map_err(|err| err.to_string())?
            .home_dir
            .join(".codex"),
        _ => return Err("未知路径类型。".to_string()),
    };
    open_folder(&target)
}

pub fn tool_status() -> ToolStatus {
    let settings = load_settings().unwrap_or_default();
    let installed = detect_installed(&settings);
    let config_path = app_paths().ok().map(|paths| paths.home_dir.join(".codex"));
    ToolStatus {
        id: "codex-app".to_string(),
        name: "Codex 客户端".to_string(),
        category: ToolCategory::AiTool,
        command: if cfg!(target_os = "windows") {
            "Codex.exe".to_string()
        } else {
            "Codex.app".to_string()
        },
        path_repair: None,
        version: installed.as_ref().map(|item| item.version.clone()),
        latest_version: None,
        update_available: false,
        update_command: None,
        install_state: if installed.is_some() {
            InstallState::Installed
        } else {
            InstallState::Missing
        },
        config_state: match &config_path {
            Some(path) if path.exists() => ConfigState::Configured,
            Some(_) => ConfigState::Unconfigured,
            None => ConfigState::Unknown,
        },
        config_path: config_path.as_deref().map(display_path),
        install_command: Some("在 Codex 客户端页面中安装或更新".to_string()),
        details: installed
            .as_ref()
            .map(|item| format!("{} / {}", item.source, item.path))
            .or_else(|| Some("未检测到官方 Codex 桌面客户端".to_string())),
    }
}

fn build_plan(
    settings: &CodexClientSettings,
    installed: Option<&InstalledCodexClient>,
    release: &CodexClientRelease,
) -> Result<CodexClientPlan, String> {
    let capabilities = probe_capabilities();
    let current_version = installed.map(|item| item.version.clone());
    let up_to_date = current_version
        .as_deref()
        .map(|version| compare_versions(version, &release.version) != Ordering::Less)
        .unwrap_or(false);
    let portable_recommended = capabilities.iter().any(|cap| {
        cap.status == Severity::Error
            && ["add-appx", "appx-service", "msix-runtime"].contains(&cap.id.as_str())
    });
    let existing_source = installed.map(|item| item.source.as_str());
    let route = select_install_route(settings, installed, portable_recommended).to_string();
    let mut warnings = Vec::new();
    if settings.source == "official" && cfg!(target_os = "windows") {
        warnings.push(
            "Windows 官方源暂未提供与镜像一致的 manifest/checksum 合约，已使用镜像源计划。"
                .to_string(),
        );
    }
    if route == "unsupported" {
        warnings.push("当前平台暂未提供 Codex 桌面客户端安装执行链路。".to_string());
    } else if route == "macos-dmg" {
        if settings.source == "official" {
            warnings.push(
                "macOS 官方源使用官网稳定 DMG 下载地址；版本与 SHA-256 仍以镜像 manifest 为准。"
                    .to_string(),
            );
        }
        if capabilities
            .iter()
            .any(|capability| capability.status == Severity::Error)
        {
            warnings.push("macOS DMG 安装依赖不可用，安装前需要恢复 hdiutil/ditto。".to_string());
        }
    } else if route == "portable-fallback" {
        warnings.push("当前计划会安装便携版，并在开始菜单与卸载项中登记。".to_string());
        if portable_recommended {
            warnings.push(package::msix_runtime_unavailable_message(None));
        }
    } else if existing_source == Some("msix") && portable_recommended {
        warnings.push(
            "已检测到现有 MSIX 安装，本次更新会优先覆盖原 MSIX；即使能力探测建议便携版，也不会在更新时自动改变安装类型。"
                .to_string(),
        );
    }

    Ok(CodexClientPlan {
        up_to_date,
        current_version,
        latest_version: release.version.clone(),
        route,
        package_url: release.package_url.clone(),
        download_size: release.content_length,
        sha256: release.sha256.clone(),
        staged_path: staged_package_path(release)
            .ok()
            .filter(|path| path.exists())
            .map(|path| display_path(&path)),
        install_root: Some(
            expand_env_path(&settings.install_root)?
                .to_string_lossy()
                .to_string(),
        ),
        warnings,
        capabilities,
    })
}

fn select_install_route(
    settings: &CodexClientSettings,
    installed: Option<&InstalledCodexClient>,
    portable_recommended: bool,
) -> &'static str {
    if cfg!(target_os = "macos") {
        return "macos-dmg";
    }
    if !cfg!(target_os = "windows") {
        return "unsupported";
    }
    let existing_source = installed.map(|item| item.source.as_str());
    if existing_source == Some("msix") {
        "msix-sideload"
    } else if existing_source == Some("portable")
        || settings.windows_install_mode == "portable"
        || portable_recommended
    {
        "portable-fallback"
    } else {
        "msix-sideload"
    }
}

fn stage_from_plan<F>(
    release: &CodexClientRelease,
    plan: &CodexClientPlan,
    on_progress: &F,
) -> Result<CodexClientStageReport, String>
where
    F: Fn(CodexClientProgress),
{
    if plan.up_to_date {
        emit_step_progress(
            on_progress,
            "done",
            "Codex 客户端已经是最新版本，无需下载。",
            Some(1),
            Some(1),
            Some(4),
            Some(4),
        );
        return Ok(CodexClientStageReport {
            up_to_date: true,
            staged_path: None,
            package_moniker: release.package_moniker.clone(),
            download_size: 0,
            sha256: release.sha256.clone(),
            hash_verified: true,
            route: plan.route.clone(),
            notes: vec!["Codex 客户端已经是最新版本，无需下载。".to_string()],
        });
    }

    let path = staged_package_path(release)?;
    if !path.exists() || sha256_file(&path).ok().as_deref() != Some(release.sha256.as_str()) {
        if path.exists() {
            let _ = fs::remove_file(&path);
        }
        download_to_file(
            &release.package_url,
            &path,
            release.content_length,
            on_progress,
        )?;
    } else {
        let size = fs::metadata(&path).map_err(|err| err.to_string())?.len();
        emit_step_progress(
            on_progress,
            "verifying",
            "已找到暂存安装包，正在校验 SHA-256...",
            Some(size),
            Some(size),
            Some(3),
            Some(4),
        );
    }

    emit_step_progress(
        on_progress,
        "verifying",
        "正在校验安装包 SHA-256...",
        None,
        None,
        Some(3),
        Some(4),
    );
    let actual = sha256_file(&path)?;
    if !actual.eq_ignore_ascii_case(&release.sha256) {
        let _ = fs::remove_file(&path);
        return Err(format!(
            "SHA-256 校验失败：期望 {}，实际 {}。",
            release.sha256, actual
        ));
    }
    let size = fs::metadata(&path).map_err(|err| err.to_string())?.len();
    let _ = activity_log::append(
        Severity::Ok,
        format!("Staged Codex Client package {}.", release.package_moniker),
    );
    emit_step_progress(
        on_progress,
        "done",
        "安装包已下载并通过 SHA-256 校验。",
        Some(size),
        Some(size),
        Some(4),
        Some(4),
    );

    Ok(CodexClientStageReport {
        up_to_date: false,
        staged_path: Some(display_path(&path)),
        package_moniker: release.package_moniker.clone(),
        download_size: size,
        sha256: release.sha256.clone(),
        hash_verified: true,
        route: plan.route.clone(),
        notes: vec!["安装包已下载并通过 SHA-256 校验。".to_string()],
    })
}

fn cleanup_staged_package(stage: &mut CodexClientStageReport, notes: &mut Vec<String>) {
    let Some(staged_path) = stage.staged_path.as_deref() else {
        return;
    };
    let path = PathBuf::from(staged_path);
    if !path.exists() {
        stage.staged_path = None;
        return;
    }
    match fs::remove_file(&path) {
        Ok(()) => {
            stage.staged_path = None;
            notes.push("已清理本次使用的暂存安装包。".to_string());
        }
        Err(err) => {
            notes.push(format!(
                "暂存安装包清理失败：{}，可稍后手动删除 {}。",
                err,
                display_path(&path)
            ));
        }
    }
}

fn load_release(settings: &CodexClientSettings) -> Result<CodexClientRelease, String> {
    let base = manifest_base(settings);
    let manifest_url = format!("{base}/latest/manifest");
    let checksums_url = format!("{base}/latest/checksums");
    let manifest_text = fetch_text(&manifest_url)?;
    let checksums_text = fetch_text(&checksums_url)?;
    let manifest: MirrorManifest = serde_json::from_str(&manifest_text)
        .map_err(|err| format!("解析 Codex 镜像 manifest 失败：{err}"))?;
    if manifest.schema_version < 2 {
        return Err(format!(
            "不支持的 Codex 镜像 manifest schemaVersion：{}",
            manifest.schema_version
        ));
    }

    let macos_arm64_version = manifest
        .sources
        .macos
        .as_ref()
        .and_then(|macos| macos.arm64.as_ref())
        .and_then(|source| source.bundle_short_version.clone());
    let macos_x64_version = manifest
        .sources
        .macos
        .as_ref()
        .and_then(|macos| macos.x64.as_ref())
        .and_then(|source| source.bundle_short_version.clone());

    if cfg!(target_os = "macos") {
        let macos = manifest
            .sources
            .macos
            .as_ref()
            .ok_or_else(|| "Codex 镜像 manifest 没有 macOS 安装包信息。".to_string())?;
        let (source, arch) = current_macos_source(macos)?;
        let source_url = source
            .url
            .clone()
            .ok_or_else(|| format!("Codex 镜像 manifest 没有 macOS {arch} 下载地址。"))?;
        let package_url = if settings.source == "official" {
            official_macos_url(arch).to_string()
        } else {
            source_url
        };
        let checksum_name = format!("Codex-mac-{arch}.dmg");
        let package_moniker =
            package_filename(&package_url).unwrap_or_else(|| checksum_name.clone());
        let sha256 = source
            .sha256
            .clone()
            .or_else(|| checksum_for_name(&checksums_text, &checksum_name))
            .or_else(|| checksum_for_name(&checksums_text, &package_moniker))
            .ok_or_else(|| format!("checksums 中没有找到 macOS {arch} DMG 的 SHA-256。"))?;
        let version = source
            .bundle_short_version
            .clone()
            .or_else(|| source.bundle_version.clone())
            .ok_or_else(|| format!("Codex 镜像 manifest 没有 macOS {arch} 版本号。"))?;

        return Ok(CodexClientRelease {
            version,
            package_moniker,
            architecture: Some(arch.to_string()),
            package_kind: "dmg".to_string(),
            package_source: settings.source.clone(),
            content_length: source.content_length,
            etag: source.etag.clone(),
            package_identity: source
                .bundle_identifier
                .clone()
                .or_else(|| Some(CODEX_MACOS_BUNDLE_ID.to_string())),
            package_url,
            checksums_url,
            manifest_url,
            sha256,
            macos_arm64_version,
            macos_x64_version,
        });
    }

    let windows = manifest.sources.windows;
    let package_url = format!("{base}/latest/win");
    let sha256 =
        checksum_for_windows(&checksums_text, &windows.package_moniker).ok_or_else(|| {
            format!(
                "checksums 中没有找到 {} 的 SHA-256。",
                windows.package_moniker
            )
        })?;

    Ok(CodexClientRelease {
        version: windows.version,
        package_moniker: windows.package_moniker,
        architecture: windows.architecture,
        package_kind: "msix".to_string(),
        package_source: "mirror".to_string(),
        content_length: windows.content_length,
        etag: windows.etag,
        package_identity: windows
            .update_manifest
            .as_ref()
            .and_then(|item| item.package_identity.clone())
            .or(windows.product_id)
            .or_else(|| Some(PACKAGE_IDENTITY.to_string())),
        package_url,
        checksums_url,
        manifest_url,
        sha256,
        macos_arm64_version,
        macos_x64_version,
    })
}

fn detect_installed(settings: &CodexClientSettings) -> Option<InstalledCodexClient> {
    if cfg!(target_os = "windows") {
        package::detect_msix_package(PACKAGE_IDENTITY)
            .map(installed_from_msix)
            .or_else(|| {
                expand_env_path(&settings.install_root)
                    .ok()
                    .and_then(|root| detect_portable_install(&root))
            })
    } else if cfg!(target_os = "macos") {
        package::detect_macos_app(&macos_app_candidates(), Some(CODEX_MACOS_BUNDLE_ID))
            .map(installed_from_macos_app)
    } else {
        None
    }
}

fn installed_from_msix(package: package::InstalledMsixPackage) -> InstalledCodexClient {
    InstalledCodexClient {
        installed_at: path_mtime(&PathBuf::from(&package.path)),
        path: package.path,
        version: package.version,
        arch: package.arch,
        source: "msix".to_string(),
        package_family_name: package.package_family_name,
    }
}

fn installed_from_macos_app(app: package::InstalledMacosApp) -> InstalledCodexClient {
    InstalledCodexClient {
        installed_at: path_mtime(&PathBuf::from(&app.path)),
        path: app.path,
        version: app.version,
        arch: None,
        source: "macos".to_string(),
        package_family_name: app.bundle_identifier,
    }
}

fn detect_portable_install(root: &Path) -> Option<InstalledCodexClient> {
    let exe = root.join("Codex.exe");
    if !exe.is_file() {
        return None;
    }
    let identity = fs::read_to_string(root.join("AppxManifest.xml"))
        .ok()
        .and_then(|xml| parse_msix_identity(&xml).ok());
    Some(InstalledCodexClient {
        path: root.to_string_lossy().to_string(),
        version: identity
            .as_ref()
            .map(|item| item.version.clone())
            .unwrap_or_else(|| "0.0.0.0".to_string()),
        arch: identity
            .as_ref()
            .map(|item| item.processor_architecture.clone()),
        source: "portable".to_string(),
        package_family_name: None,
        installed_at: path_mtime(&exe),
    })
}

fn macos_app_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from("/Applications").join(CODEX_MACOS_APP_NAME)];
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join("Applications").join(CODEX_MACOS_APP_NAME));
    }
    candidates
}

struct PortableInstallReport {
    installed: Option<InstalledCodexClient>,
    notes: Vec<String>,
}

fn install_portable<F>(
    msix_path: &Path,
    install_root: &Path,
    on_progress: &F,
) -> Result<PortableInstallReport, String>
where
    F: Fn(CodexClientProgress),
{
    emit_step_progress(
        on_progress,
        "installing",
        "正在准备便携版安装目录...",
        None,
        None,
        Some(4),
        Some(7),
    );
    validate_install_root(install_root)?;
    let mut notes = Vec::new();
    let termination = process_control::close_processes_for_update(
        "Codex 客户端",
        &["Codex"],
        Some(install_root),
    )?;
    if let Some(note) = termination.note("Codex 客户端") {
        notes.push(note);
    }
    let parent = install_root
        .parent()
        .ok_or_else(|| "安装目录无父级目录。".to_string())?;
    fs::create_dir_all(parent).map_err(|err| format!("创建安装父目录失败：{err}"))?;
    let work = parent
        .join(".codestudio-codex-client-staging")
        .join(format!("portable-{}", std::process::id()));
    let extracted = work.join("extracted");
    let payload = work.join("payload");
    if work.exists() {
        fs::remove_dir_all(&work).map_err(|err| format!("清理旧暂存目录失败：{err}"))?;
    }
    fs::create_dir_all(&extracted).map_err(|err| format!("创建暂存目录失败：{err}"))?;

    let manifest_xml = extract_msix(msix_path, &extracted, on_progress)?;
    let identity = parse_msix_identity(&manifest_xml)?;
    if identity.name != PACKAGE_IDENTITY {
        notes.push(format!(
            "MSIX Identity 是 {}，不是预期的 {}。",
            identity.name, PACKAGE_IDENTITY
        ));
    }
    if !identity.publisher.to_ascii_lowercase().contains("openai") {
        notes.push(format!(
            "MSIX Publisher 未显示为 OpenAI：{}。",
            identity.publisher
        ));
    }
    let exe = find_codex_exe(&extracted)?;
    let exe_dir = exe
        .parent()
        .ok_or_else(|| "Codex.exe 无父级目录。".to_string())?;
    emit_step_progress(
        on_progress,
        "copying",
        "正在复制便携版文件...",
        None,
        None,
        Some(5),
        Some(7),
    );
    copy_dir_all(exe_dir, &payload).map_err(|err| format!("复制便携版文件失败：{err}"))?;
    fs::write(payload.join("AppxManifest.xml"), manifest_xml)
        .map_err(|err| format!("写入 AppxManifest.xml 失败：{err}"))?;

    emit_step_progress(
        on_progress,
        "writing",
        "正在写入安装目录...",
        None,
        None,
        Some(6),
        Some(7),
    );
    let rollback = parent.join("Codex.rollback");
    if rollback.exists() {
        fs::remove_dir_all(&rollback).map_err(|err| format!("清理旧回滚目录失败：{err}"))?;
    }
    let had_previous = install_root.exists();
    if had_previous {
        fs::rename(install_root, &rollback).map_err(|err| format!("创建回滚备份失败：{err}"))?;
    }
    if let Err(err) = fs::rename(&payload, install_root) {
        if had_previous && rollback.exists() {
            let _ = fs::rename(&rollback, install_root);
        }
        return Err(format!("写入便携版安装目录失败，已尝试回滚：{err}"));
    }

    emit_step_progress(
        on_progress,
        "finalizing",
        "正在创建快捷方式与卸载项...",
        None,
        None,
        Some(6),
        Some(7),
    );
    let registration = portable_registration(install_root, &identity.version);
    if let Err(err) = package::create_portable_start_menu_shortcut(&registration) {
        notes.push(format!("开始菜单快捷方式创建失败：{err}"));
    }
    if let Err(err) = package::create_portable_uninstall_entry(&registration) {
        notes.push(format!("卸载项登记失败：{err}"));
    }
    if had_previous && rollback.exists() {
        if let Err(err) = fs::remove_dir_all(&rollback) {
            notes.push(format!("回滚备份清理失败：{err}"));
        }
    }
    let _ = fs::remove_dir_all(&work);
    emit_step_progress(
        on_progress,
        "finalizing",
        "便携版安装已写入。",
        Some(1),
        Some(1),
        Some(6),
        Some(7),
    );

    Ok(PortableInstallReport {
        installed: Some(InstalledCodexClient {
            path: install_root.to_string_lossy().to_string(),
            version: identity.version,
            arch: Some(identity.processor_architecture),
            source: "portable".to_string(),
            package_family_name: None,
            installed_at: path_mtime(&install_root.join("Codex.exe")),
        }),
        notes,
    })
}

fn extract_msix<F>(msix_path: &Path, dest: &Path, on_progress: &F) -> Result<String, String>
where
    F: Fn(CodexClientProgress),
{
    let file = File::open(msix_path).map_err(|err| format!("打开 MSIX 失败：{err}"))?;
    let mut zip = ZipArchive::new(file).map_err(|err| format!("读取 MSIX ZIP 结构失败：{err}"))?;
    let mut manifest_xml = None;
    let total_entries = zip.len();
    let total = total_entries as u64;
    emit_step_progress(
        on_progress,
        "extracting",
        "正在解包 MSIX 安装包...",
        Some(0),
        Some(total),
        Some(4),
        Some(7),
    );

    for index in 0..total_entries {
        let mut entry = zip
            .by_index(index)
            .map_err(|err| format!("读取 MSIX 条目失败：{err}"))?;
        let Some(enclosed_name) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
            continue;
        };
        let out_path = dest.join(&enclosed_name);
        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(|err| format!("创建解包目录失败：{err}"))?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|err| format!("创建解包父目录失败：{err}"))?;
        }
        let mut out = File::create(&out_path).map_err(|err| format!("创建解包文件失败：{err}"))?;
        io::copy(&mut entry, &mut out).map_err(|err| format!("写入解包文件失败：{err}"))?;

        if enclosed_name
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("AppxManifest.xml"))
            && enclosed_name.components().count() == 1
        {
            let mut xml = String::new();
            File::open(&out_path)
                .and_then(|mut file| file.read_to_string(&mut xml))
                .map_err(|err| format!("读取 AppxManifest.xml 失败：{err}"))?;
            manifest_xml = Some(xml);
        }
        if index == 0 || index + 1 == total_entries || index % 25 == 0 {
            emit_step_progress(
                on_progress,
                "extracting",
                "正在解包 MSIX 安装包...",
                Some((index + 1) as u64),
                Some(total),
                Some(4),
                Some(7),
            );
        }
    }

    manifest_xml.ok_or_else(|| "MSIX 缺少 AppxManifest.xml。".to_string())
}

fn find_codex_exe(root: &Path) -> Result<PathBuf, String> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).map_err(|err| format!("扫描解包目录失败：{err}"))?
        {
            let entry = entry.map_err(|err| format!("读取解包目录项失败：{err}"))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|err| format!("读取文件类型失败：{err}"))?;
            if file_type.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("Codex.exe"))
            {
                return Ok(path);
            }
        }
    }
    Err("MSIX 中没有找到 Codex.exe。".to_string())
}

fn copy_dir_all(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let dest = to.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir_all(&source, &dest)?;
        } else if file_type.is_file() {
            fs::copy(source, dest)?;
        }
    }
    Ok(())
}

fn parse_msix_identity(xml: &str) -> Result<MsixIdentity, String> {
    let identity_tag = xml
        .split('<')
        .find(|part| part.trim_start().starts_with("Identity "))
        .ok_or_else(|| "AppxManifest.xml 缺少 Identity。".to_string())?;
    let get = |name: &str| -> Result<String, String> {
        let needle = format!("{name}=\"");
        let start = identity_tag
            .find(&needle)
            .ok_or_else(|| format!("Identity 缺少 {name}。"))?
            + needle.len();
        let rest = &identity_tag[start..];
        let end = rest
            .find('"')
            .ok_or_else(|| format!("Identity {name} 格式无效。"))?;
        Ok(rest[..end].to_string())
    };
    Ok(MsixIdentity {
        name: get("Name")?,
        publisher: get("Publisher")?,
        version: get("Version")?,
        processor_architecture: get("ProcessorArchitecture")?,
    })
}

fn probe_capabilities() -> Vec<CodexClientCapability> {
    let capabilities = if cfg!(target_os = "macos") {
        package::probe_macos_dmg_capabilities()
    } else {
        package::probe_msix_capabilities()
    };
    capabilities
        .into_iter()
        .map(|capability| CodexClientCapability {
            id: capability.id,
            label: capability.label,
            status: capability.status,
            detail: capability.detail,
        })
        .collect()
}

fn manifest_base(_settings: &CodexClientSettings) -> String {
    DEFAULT_MIRROR_BASE.to_string()
}

fn normalize_source(source: &str) -> String {
    match source.trim() {
        "official" if cfg!(target_os = "macos") => "official".to_string(),
        "mirror" => "mirror".to_string(),
        _ => "mirror".to_string(),
    }
}

fn current_macos_source(macos: &MacosSources) -> Result<(&MacosSource, &'static str), String> {
    if cfg!(target_arch = "aarch64") {
        macos
            .arm64
            .as_ref()
            .map(|source| (source, "arm64"))
            .ok_or_else(|| "Codex 镜像 manifest 没有 macOS arm64 安装包信息。".to_string())
    } else {
        macos
            .x64
            .as_ref()
            .map(|source| (source, "x64"))
            .ok_or_else(|| "Codex 镜像 manifest 没有 macOS x64 安装包信息。".to_string())
    }
}

fn official_macos_url(arch: &str) -> &'static str {
    if arch == "arm64" {
        OFFICIAL_MACOS_ARM64_URL
    } else {
        OFFICIAL_MACOS_X64_URL
    }
}

fn package_filename(url: &str) -> Option<String> {
    url.split('?')
        .next()
        .and_then(|part| part.rsplit('/').next())
        .filter(|part| !part.trim().is_empty())
        .map(ToString::to_string)
}

fn checksum_for_windows(text: &str, package_moniker: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?;
        if name.contains(package_moniker) || name.ends_with(".Msix") {
            Some(hash.to_string())
        } else {
            None
        }
    })
}

fn checksum_for_name(text: &str, expected_name: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        if name == expected_name || name.ends_with(&format!("/{expected_name}")) {
            Some(hash.to_string())
        } else {
            None
        }
    })
}

fn fetch_text(url: &str) -> Result<String, String> {
    let output = hidden_command("curl")
        .args(["-fsSL", "--connect-timeout", "20", "--retry", "2", url])
        .output()
        .map_err(|err| format!("启动 curl 失败：{err}"))?;
    if !output.status.success() {
        return Err(format!(
            "读取 {} 失败：{}",
            url_host(url),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|err| format!("响应不是 UTF-8：{err}"))
}

fn download_to_file<F>(
    url: &str,
    path: &Path,
    expected_total: Option<u64>,
    on_progress: &F,
) -> Result<(), String>
where
    F: Fn(CodexClientProgress),
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("创建下载目录失败：{err}"))?;
    }
    let temp = path.with_extension("download");
    if temp.exists() {
        let _ = fs::remove_file(&temp);
    }
    emit_step_progress(
        on_progress,
        "downloading",
        "正在下载安装包...",
        Some(0),
        expected_total,
        Some(2),
        Some(4),
    );
    let mut child = hidden_command("curl")
        .args([
            "-fLsS",
            "--connect-timeout",
            "20",
            "--retry",
            "2",
            "--output",
            &temp.to_string_lossy(),
            url,
        ])
        .spawn()
        .map_err(|err| format!("启动下载失败：{err}"))?;
    let mut last_emit = Instant::now() - Duration::from_secs(2);
    loop {
        match child
            .try_wait()
            .map_err(|err| format!("等待下载进程失败：{err}"))?
        {
            Some(_) => break,
            None => {
                let downloaded = fs::metadata(&temp).ok().map(|metadata| metadata.len());
                if last_emit.elapsed() >= Duration::from_millis(500) {
                    emit_step_progress(
                        on_progress,
                        "downloading",
                        "正在下载安装包...",
                        downloaded,
                        expected_total,
                        Some(2),
                        Some(4),
                    );
                    last_emit = Instant::now();
                }
                thread::sleep(Duration::from_millis(150));
            }
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|err| format!("读取下载结果失败：{err}"))?;
    if !output.status.success() {
        let _ = fs::remove_file(&temp);
        return Err(format!(
            "下载 {} 失败：{}",
            url_host(url),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let downloaded = fs::metadata(&temp).ok().map(|metadata| metadata.len());
    emit_step_progress(
        on_progress,
        "downloading",
        "安装包下载完成。",
        downloaded,
        expected_total,
        Some(2),
        Some(4),
    );
    fs::rename(&temp, path).map_err(|err| format!("保存下载文件失败：{err}"))
}

fn emit_step_progress<F>(
    on_progress: &F,
    phase: &str,
    message: impl Into<String>,
    downloaded: Option<u64>,
    total: Option<u64>,
    step: Option<u64>,
    step_total: Option<u64>,
) where
    F: Fn(CodexClientProgress),
{
    let percent = match (downloaded, total) {
        (Some(done), Some(total)) if total > 0 => {
            Some(((done as f64 / total as f64) * 100.0).clamp(0.0, 100.0))
        }
        _ => None,
    };
    on_progress(CodexClientProgress {
        phase: phase.to_string(),
        message: message.into(),
        downloaded,
        total,
        percent,
        step,
        step_total,
    });
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|err| format!("打开文件计算 SHA-256 失败：{err}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 128];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|err| format!("读取文件计算 SHA-256 失败：{err}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn staged_package_path(release: &CodexClientRelease) -> Result<PathBuf, String> {
    let dir = staging_dir()?;
    let lower = release.package_moniker.to_ascii_lowercase();
    let file = if lower.ends_with(".msix") || lower.ends_with(".dmg") || lower.ends_with(".zip") {
        release.package_moniker.clone()
    } else if release.package_kind == "dmg" {
        format!("{}.dmg", release.package_moniker)
    } else {
        format!("{}.Msix", release.package_moniker)
    };
    Ok(dir.join(file))
}

fn staging_dir() -> Result<PathBuf, String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    ensure_dirs(&paths).map_err(|err| err.to_string())?;
    let dir = paths.config_dir.join("downloads").join("codex-client");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}

fn settings_file() -> Result<PathBuf, String> {
    Ok(app_paths()
        .map_err(|err| err.to_string())?
        .config_dir
        .join("codex-client-settings.json"))
}

fn marker_file() -> Result<PathBuf, String> {
    Ok(app_paths()
        .map_err(|err| err.to_string())?
        .config_dir
        .join("codex-client-managed.json"))
}

fn load_settings() -> Result<CodexClientSettings, String> {
    let path = settings_file()?;
    if !path.exists() {
        let settings = CodexClientSettings::default();
        save_settings(&settings)?;
        return Ok(settings);
    }
    let mut settings: CodexClientSettings = serde_json::from_str(
        &fs::read_to_string(&path).map_err(|err| format!("读取 Codex 客户端设置失败：{err}"))?,
    )
    .map_err(|err| format!("解析 Codex 客户端设置失败：{err}"))?;
    settings.source = normalize_source(&settings.source);
    settings.custom_url = String::new();
    settings.signed_only = true;
    if settings.install_root.trim().is_empty() {
        settings.install_root = default_install_root();
    }
    Ok(settings)
}

fn save_settings(settings: &CodexClientSettings) -> Result<(), String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    ensure_dirs(&paths).map_err(|err| err.to_string())?;
    let path = settings_file()?;
    let json = serde_json::to_string_pretty(settings).map_err(|err| err.to_string())?;
    fs::write(path, json).map_err(|err| format!("保存 Codex 客户端设置失败：{err}"))
}

fn save_marker(marker: &ManagedInstallMarker) -> Result<(), String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    ensure_dirs(&paths).map_err(|err| err.to_string())?;
    let json = serde_json::to_string_pretty(marker).map_err(|err| err.to_string())?;
    fs::write(marker_file()?, json).map_err(|err| format!("保存 Codex 客户端托管标记失败：{err}"))
}

fn load_marker() -> Option<ManagedInstallMarker> {
    fs::read_to_string(marker_file().ok()?)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
}

fn install_class(installed: Option<&InstalledCodexClient>) -> String {
    let Some(installed) = installed else {
        return "none".to_string();
    };
    let Some(marker) = load_marker() else {
        return "external".to_string();
    };
    let marker_matches = marker
        .version
        .as_deref()
        .map(|version| compare_versions(version, &installed.version) == Ordering::Equal)
        .unwrap_or(true);
    if marker_matches {
        "managed".to_string()
    } else {
        "external".to_string()
    }
}

fn validate_install_target(settings: &CodexClientSettings) -> Result<(), String> {
    let path = expand_env_path(&settings.install_root)?;
    validate_install_path_for_platform(&path)
}

fn validate_install_path_for_platform(path: &Path) -> Result<(), String> {
    if cfg!(target_os = "windows") {
        validate_install_root(path)
    } else if cfg!(target_os = "macos") {
        validate_macos_install_target(path)
    } else {
        Ok(())
    }
}

fn validate_macos_install_target(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("安装位置必须是绝对路径。".to_string());
    }
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("app"))
        != Some(true)
    {
        return Err("macOS 安装位置必须指向 .app 应用包。".to_string());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "macOS 安装位置缺少父目录。".to_string())?;
    if !parent.exists() {
        return Err("macOS 安装位置的父目录不存在。".to_string());
    }
    if path.exists() && !path.is_dir() {
        return Err("macOS 安装位置已存在，但不是应用目录。".to_string());
    }
    Ok(())
}

fn validate_install_root(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("安装位置必须是绝对路径。".to_string());
    }
    if path.parent().is_none() {
        return Err("安装位置不能是磁盘根目录。".to_string());
    }
    if path.exists() && !path.is_dir() {
        return Err("安装位置必须是文件夹。".to_string());
    }
    if path.exists() && !is_empty_dir(path)? && !is_existing_portable_root(path) {
        return Err("安装位置必须是空文件夹，或已有的 Codex 便携版目录。".to_string());
    }
    let protected = protected_roots();
    if protected
        .iter()
        .any(|root| path_is_equal_or_child(path, root))
    {
        return Err("安装位置不能放在系统目录或管理员目录。".to_string());
    }
    Ok(())
}

fn protected_roots() -> Vec<PathBuf> {
    [
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "SystemRoot",
        "WINDIR",
    ]
    .iter()
    .filter_map(|name| std::env::var_os(name))
    .map(PathBuf::from)
    .collect()
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

fn path_is_equal_or_child(path: &Path, root: &Path) -> bool {
    let path = path_key(path);
    let root = path_key(root);
    path == root || path.starts_with(&format!("{root}\\"))
}

fn is_empty_dir(path: &Path) -> Result<bool, String> {
    Ok(fs::read_dir(path)
        .map_err(|err| format!("读取安装目录失败：{err}"))?
        .next()
        .is_none())
}

fn is_existing_portable_root(path: &Path) -> bool {
    path.join("Codex.exe").is_file() && path.join("AppxManifest.xml").is_file()
}

fn expand_env_path(raw: &str) -> Result<PathBuf, String> {
    let mut value = raw.trim().to_string();
    if cfg!(windows) {
        for (key, env_key) in [
            ("%LOCALAPPDATA%", "LOCALAPPDATA"),
            ("%APPDATA%", "APPDATA"),
            ("%USERPROFILE%", "USERPROFILE"),
        ] {
            if value.to_ascii_uppercase().starts_with(key) {
                let replacement =
                    std::env::var(env_key).map_err(|_| format!("环境变量 {env_key} 不可用。"))?;
                value = format!("{replacement}{}", &value[key.len()..]);
            }
        }
    }
    Ok(PathBuf::from(value))
}

fn default_install_root() -> String {
    if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join("AppData").join("Local")))
            .unwrap_or_else(|| PathBuf::from("C:\\Users\\Public\\AppData\\Local"))
            .join("Programs")
            .join("Codex")
            .to_string_lossy()
            .to_string()
    } else if cfg!(target_os = "macos") {
        "/Applications/Codex.app".to_string()
    } else {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".local")
            .join("share")
            .join("Codex")
            .to_string_lossy()
            .to_string()
    }
}

fn platform_label() -> String {
    if cfg!(target_os = "windows") {
        "windows".to_string()
    } else if cfg!(target_os = "macos") {
        "macos".to_string()
    } else if cfg!(target_os = "linux") {
        "linux".to_string()
    } else {
        "unknown".to_string()
    }
}

fn compare_versions(left: &str, right: &str) -> Ordering {
    let left_parts = version_parts(left);
    let right_parts = version_parts(right);
    let len = left_parts.len().max(right_parts.len());
    for index in 0..len {
        let left = *left_parts.get(index).unwrap_or(&0);
        let right = *right_parts.get(index).unwrap_or(&0);
        match left.cmp(&right) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
    }
    Ordering::Equal
}

fn version_parts(value: &str) -> Vec<u64> {
    value
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}

fn path_mtime(path: &Path) -> Option<String> {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|duration| chrono::DateTime::from_timestamp(duration.as_secs() as i64, 0))
        .map(|time| time.to_rfc3339())
}

#[cfg(windows)]
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn terminate_codex_process_for_uninstall(
    root: Option<&Path>,
    notes: &mut Vec<String>,
) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Ok(());
    }
    let root_filter = root
        .map(|path| ps_quote(&path.to_string_lossy()))
        .unwrap_or_else(|| "$null".to_string());
    let script = format!(
        r#"
$RootFilter = {root_filter}
if ($null -ne $RootFilter) {{
  try {{ $RootFilter = [System.IO.Path]::GetFullPath($RootFilter).TrimEnd('\') }} catch {{}}
}}
function Get-TargetCodexProcess {{
  $all = Get-Process -Name Codex -ErrorAction SilentlyContinue
  foreach ($p in $all) {{
    if ($null -eq $RootFilter) {{
      $p
      continue
    }}
    try {{
      $path = [string]$p.Path
      if (-not $path) {{ continue }}
      $full = [System.IO.Path]::GetFullPath($path)
      if ($full.Equals($RootFilter, [System.StringComparison]::OrdinalIgnoreCase) -or
          $full.StartsWith($RootFilter + '\', [System.StringComparison]::OrdinalIgnoreCase)) {{
        $p
      }}
    }} catch {{}}
  }}
}}
$procs = @(Get-TargetCodexProcess)
$targetIds = @($procs | ForEach-Object {{ $_.Id }})
foreach ($p in $procs) {{
  try {{
    if ($p.MainWindowHandle -ne 0) {{ [void]$p.CloseMainWindow() }}
  }} catch {{}}
}}
$deadline = (Get-Date).AddSeconds(8)
while ((Get-Date) -lt $deadline) {{
  Start-Sleep -Milliseconds 250
  $remaining = @()
  foreach ($id in $targetIds) {{
    $p = Get-Process -Id $id -ErrorAction SilentlyContinue
    if ($null -ne $p) {{ $remaining += $p }}
  }}
  if ($remaining.Count -eq 0) {{ break }}
}}
$remaining = @()
foreach ($id in $targetIds) {{
  $p = Get-Process -Id $id -ErrorAction SilentlyContinue
  if ($null -ne $p) {{ $remaining += $p }}
}}
$forced = 0
foreach ($p in $remaining) {{
  try {{
    Stop-Process -Id $p.Id -Force -ErrorAction Stop
    $forced += 1
  }} catch {{}}
}}
Start-Sleep -Milliseconds 300
$still = @()
foreach ($id in $targetIds) {{
  $p = Get-Process -Id $id -ErrorAction SilentlyContinue
  if ($null -ne $p) {{ $still += $p }}
}}
[pscustomobject]@{{
  total = [int]$targetIds.Count
  forced = [int]$forced
  remaining = [int]$still.Count
}} | ConvertTo-Json -Compress
"#
    );
    let json = run_powershell(&script)?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|err| format!("解析 Codex 进程结束结果失败：{err}"))?;
    let total = value
        .get("total")
        .and_then(|item| item.as_u64())
        .unwrap_or(0);
    let forced = value
        .get("forced")
        .and_then(|item| item.as_u64())
        .unwrap_or(0);
    let remaining = value
        .get("remaining")
        .and_then(|item| item.as_u64())
        .unwrap_or(0);
    if remaining > 0 {
        return Err("仍有 Codex 桌面端进程无法结束，未继续卸载。".to_string());
    }
    if total > 0 {
        if forced > 0 {
            notes.push(format!(
                "检测到正在运行的 Codex 桌面端，已强制结束 {forced} 个进程后卸载。"
            ));
        } else {
            notes.push("检测到正在运行的 Codex 桌面端，已自动关闭后卸载。".to_string());
        }
    }
    Ok(())
}

fn terminate_codex_process_for_restart(
    root: Option<&Path>,
    notes: &mut Vec<String>,
) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Ok(());
    }
    let root_filter = root
        .map(|path| ps_quote(&path.to_string_lossy()))
        .unwrap_or_else(|| "$null".to_string());
    let script = format!(
        r#"
$RootFilter = {root_filter}
if ($null -ne $RootFilter) {{
  try {{ $RootFilter = [System.IO.Path]::GetFullPath($RootFilter).TrimEnd('\') }} catch {{}}
}}
function Get-TargetCodexProcess {{
  $all = Get-Process -Name Codex -ErrorAction SilentlyContinue
  foreach ($p in $all) {{
    if ($null -eq $RootFilter) {{
      $p
      continue
    }}
    try {{
      $path = [string]$p.Path
      if (-not $path) {{ continue }}
      $full = [System.IO.Path]::GetFullPath($path)
      if ($full.Equals($RootFilter, [System.StringComparison]::OrdinalIgnoreCase) -or
          $full.StartsWith($RootFilter + '\', [System.StringComparison]::OrdinalIgnoreCase)) {{
        $p
      }}
    }} catch {{}}
  }}
}}
$procs = @(Get-TargetCodexProcess)
$targetIds = @($procs | ForEach-Object {{ $_.Id }})
foreach ($p in $procs) {{
  try {{
    if ($p.MainWindowHandle -ne 0) {{ [void]$p.CloseMainWindow() }}
  }} catch {{}}
}}
$deadline = (Get-Date).AddSeconds(8)
while ((Get-Date) -lt $deadline) {{
  Start-Sleep -Milliseconds 250
  $remaining = @()
  foreach ($id in $targetIds) {{
    $p = Get-Process -Id $id -ErrorAction SilentlyContinue
    if ($null -ne $p) {{ $remaining += $p }}
  }}
  if ($remaining.Count -eq 0) {{ break }}
}}
$remaining = @()
foreach ($id in $targetIds) {{
  $p = Get-Process -Id $id -ErrorAction SilentlyContinue
  if ($null -ne $p) {{ $remaining += $p }}
}}
$forced = 0
foreach ($p in $remaining) {{
  try {{
    Stop-Process -Id $p.Id -Force -ErrorAction Stop
    $forced += 1
  }} catch {{}}
}}
Start-Sleep -Milliseconds 300
$still = @()
foreach ($id in $targetIds) {{
  $p = Get-Process -Id $id -ErrorAction SilentlyContinue
  if ($null -ne $p) {{ $still += $p }}
}}
[pscustomobject]@{{
  total = [int]$targetIds.Count
  forced = [int]$forced
  remaining = [int]$still.Count
}} | ConvertTo-Json -Compress
"#
    );
    let json = run_powershell(&script)?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|err| format!("解析 Codex 进程重启结果失败：{err}"))?;
    let total = value
        .get("total")
        .and_then(|item| item.as_u64())
        .unwrap_or(0);
    let forced = value
        .get("forced")
        .and_then(|item| item.as_u64())
        .unwrap_or(0);
    let remaining = value
        .get("remaining")
        .and_then(|item| item.as_u64())
        .unwrap_or(0);
    if remaining > 0 {
        return Err("仍有 Codex 桌面端进程无法结束，未继续重启。".to_string());
    }
    if total > 0 {
        if forced > 0 {
            notes.push(format!(
                "已强制结束 {forced} 个正在运行的 Codex 桌面端进程。"
            ));
        } else {
            notes.push("已自动关闭正在运行的 Codex 桌面端。".to_string());
        }
    }
    Ok(())
}

fn launch_msix() -> Result<(), String> {
    package::launch_msix_package(PACKAGE_IDENTITY)
}

fn portable_registration<'a>(
    install_root: &'a Path,
    version: &'a str,
) -> package::PortableAppRegistration<'a> {
    package::PortableAppRegistration {
        display_name: CODEX_DISPLAY_NAME,
        publisher: CODEX_PUBLISHER,
        install_root,
        executable_name: CODEX_EXE_NAME,
        shortcut_name: CODEX_SHORTCUT_NAME,
        version,
        uninstall_key: CODEX_UNINSTALL_KEY,
    }
}

fn purge_user_data() -> Result<bool, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法定位用户主目录。".to_string())?;
    let path = home.join(".codex");
    if !path.exists() {
        return Ok(false);
    }
    fs::remove_dir_all(path).map_err(|err| format!("删除 ~/.codex 失败：{err}"))?;
    Ok(true)
}

fn open_folder(path: &Path) -> Result<(), String> {
    if cfg!(target_os = "windows") {
        hidden_command("explorer.exe")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("打开路径失败：{err}"))
    } else if cfg!(target_os = "macos") {
        hidden_command("open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("打开路径失败：{err}"))
    } else {
        hidden_command("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("打开路径失败：{err}"))
    }
}

fn url_host(url: &str) -> &str {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(source: &str) -> InstalledCodexClient {
        InstalledCodexClient {
            path: "C:\\Program Files\\WindowsApps\\OpenAI.Codex".to_string(),
            version: "1.0.0.0".to_string(),
            arch: None,
            source: source.to_string(),
            package_family_name: if source == "msix" {
                Some("OpenAI.Codex_abc".to_string())
            } else {
                None
            },
            installed_at: None,
        }
    }

    #[test]
    fn existing_msix_update_keeps_msix_route() {
        let mut settings = CodexClientSettings::default();
        settings.windows_install_mode = "portable".to_string();
        let installed = installed("msix");

        assert_eq!(
            select_install_route(&settings, Some(&installed), true),
            "msix-sideload"
        );
    }

    #[test]
    fn existing_portable_update_keeps_portable_route() {
        let settings = CodexClientSettings::default();
        let installed = installed("portable");

        assert_eq!(
            select_install_route(&settings, Some(&installed), false),
            "portable-fallback"
        );
    }
}
