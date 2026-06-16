use crate::core::types::Severity;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{hidden_command, run_powershell};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMsixPackage {
    pub path: String,
    pub version: String,
    pub arch: Option<String>,
    pub package_family_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InstalledMacosApp {
    pub path: String,
    pub version: String,
    pub bundle_identifier: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MacosDmgInstallReport {
    pub installed: Option<InstalledMacosApp>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MsixInstallReport {
    pub success: bool,
    pub message: String,
    pub installed: Option<InstalledMsixPackage>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MsixRemoveReport {
    pub success: bool,
    pub message: String,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PackageCapability {
    pub id: String,
    pub label: String,
    pub status: Severity,
    pub detail: String,
}

pub struct PortableAppRegistration<'a> {
    pub display_name: &'a str,
    pub publisher: &'a str,
    pub install_root: &'a Path,
    pub executable_name: &'a str,
    pub shortcut_name: &'a str,
    pub version: &'a str,
    pub uninstall_key: &'a str,
}

pub fn detect_msix_package(package_identity: &str) -> Option<InstalledMsixPackage> {
    if !cfg!(target_os = "windows") {
        return None;
    }

    let script = format!(
        r#"
$p = Get-AppxPackage -Name {name} -ErrorAction SilentlyContinue |
  Sort-Object -Property Version -Descending |
  Select-Object -First 1
if ($null -ne $p) {{
  [pscustomobject]@{{
    path = [string]$p.InstallLocation
    version = [string]$p.Version
    arch = $null
    packageFamilyName = [string]$p.PackageFamilyName
  }} | ConvertTo-Json -Compress
}}
"#,
        name = ps_quote(package_identity)
    );
    let output = run_powershell(&script).ok()?;
    if output.trim().is_empty() {
        return None;
    }
    serde_json::from_str(&output).ok()
}

pub fn detect_macos_app(
    candidate_paths: &[PathBuf],
    bundle_identifier: Option<&str>,
) -> Option<InstalledMacosApp> {
    if !cfg!(target_os = "macos") {
        return None;
    }

    for path in candidate_paths {
        if !path.exists() {
            continue;
        }

        let detected_bundle_id = read_macos_plist_value(path, "CFBundleIdentifier");
        if let (Some(expected), Some(actual)) = (bundle_identifier, detected_bundle_id.as_deref()) {
            if actual != expected {
                continue;
            }
        }

        return Some(InstalledMacosApp {
            path: path.to_string_lossy().to_string(),
            version: read_macos_plist_value(path, "CFBundleShortVersionString")
                .unwrap_or_else(|| "installed".to_string()),
            bundle_identifier: detected_bundle_id.or_else(|| bundle_identifier.map(str::to_string)),
        });
    }

    None
}

pub fn install_msix_package(
    path: &Path,
    package_identity: &str,
) -> Result<MsixInstallReport, String> {
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
try {{
  $cmd = Get-Command Add-AppxPackage -ErrorAction Stop
  $args = @{{ ErrorAction = 'Stop' }}
  if ($cmd.Parameters.ContainsKey('LiteralPath')) {{
    $args['LiteralPath'] = {path}
  }} else {{
    $args['Path'] = {path}
  }}
  if ($cmd.Parameters.ContainsKey('ForceUpdateFromAnyVersion')) {{
    $args['ForceUpdateFromAnyVersion'] = $true
  }}
  if ($cmd.Parameters.ContainsKey('ForceApplicationShutdown')) {{
    $args['ForceApplicationShutdown'] = $true
  }}
  Add-AppxPackage @args
  $p = Get-AppxPackage -Name {name} -ErrorAction SilentlyContinue |
    Sort-Object -Property Version -Descending |
    Select-Object -First 1
  [pscustomobject]@{{
    success = $true
    message = 'Add-AppxPackage succeeded'
    installed = if ($null -ne $p) {{
      [pscustomobject]@{{
        path = [string]$p.InstallLocation
        version = [string]$p.Version
        arch = $null
        packageFamilyName = [string]$p.PackageFamilyName
      }}
    }} else {{ $null }}
  }} | ConvertTo-Json -Compress -Depth 4
}} catch {{
  [pscustomobject]@{{
    success = $false
    message = [string]$_.Exception.Message
    installed = $null
  }} | ConvertTo-Json -Compress -Depth 4
}}
"#,
        path = ps_quote(&path.to_string_lossy()),
        name = ps_quote(package_identity)
    );
    let json = run_powershell(&script)?;
    serde_json::from_str(&json).map_err(|err| format!("解析 MSIX 安装结果失败：{err}"))
}

pub fn remove_msix_package(package_identity: &str) -> Result<MsixRemoveReport, String> {
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$notes = @()
try {{
  $packages = Get-AppxPackage -Name {name} -ErrorAction SilentlyContinue
  if (-not $packages) {{
    [pscustomobject]@{{ success = $true; message = 'MSIX package was not installed'; notes = $notes }} | ConvertTo-Json -Compress
    exit 0
  }}
  foreach ($p in $packages) {{
    Remove-AppxPackage -Package $p.PackageFullName -ErrorAction Stop
  }}
  [pscustomobject]@{{ success = $true; message = 'Remove-AppxPackage succeeded'; notes = $notes }} | ConvertTo-Json -Compress
}} catch {{
  [pscustomobject]@{{ success = $false; message = [string]$_.Exception.Message; notes = $notes }} | ConvertTo-Json -Compress
}}
"#,
        name = ps_quote(package_identity)
    );
    let json = run_powershell(&script)?;
    serde_json::from_str(&json).map_err(|err| format!("解析 MSIX 卸载结果失败：{err}"))
}

pub fn probe_msix_capabilities() -> Vec<PackageCapability> {
    if !cfg!(target_os = "windows") {
        return vec![PackageCapability {
            id: "platform".to_string(),
            label: "平台".to_string(),
            status: Severity::Info,
            detail: "当前不是 Windows，MSIX/便携版执行链路不可用。".to_string(),
        }];
    }

    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$add = Get-Command Add-AppxPackage -ErrorAction SilentlyContinue
$svc = Get-Service AppXSvc -ErrorAction SilentlyContinue
$pmOk = $false
$pmError = ''
try {
  $pm = New-Object -TypeName Windows.Management.Deployment.PackageManager -ErrorAction Stop
  $pmOk = ($null -ne $pm)
} catch {
  $pmOk = $false
  $pmError = [string]$_.Exception.Message
}
[pscustomobject]@{
  addAppx = [bool]$add
  appxSvc = [bool]$svc
  appxSvcStatus = if ($svc) { [string]$svc.Status } else { '' }
  appxSvcStart = if ($svc) { [string]$svc.StartType } else { '' }
  packageManager = $pmOk
  packageManagerError = $pmError
} | ConvertTo-Json -Compress
"#;
    let value = run_powershell(script)
        .ok()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok());
    let Some(value) = value else {
        return vec![PackageCapability {
            id: "probe".to_string(),
            label: "能力探测".to_string(),
            status: Severity::Warning,
            detail: "PowerShell 能力探测失败，将保守允许便携版 fallback。".to_string(),
        }];
    };

    let add_appx = value
        .get("addAppx")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let appx_svc = value
        .get("appxSvc")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let appx_start = value
        .get("appxSvcStart")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let package_manager = value
        .get("packageManager")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let package_manager_error = value
        .get("packageManagerError")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    vec![
        PackageCapability {
            id: "add-appx".to_string(),
            label: "Add-AppxPackage".to_string(),
            status: if add_appx {
                Severity::Ok
            } else {
                Severity::Error
            },
            detail: if add_appx {
                "MSIX 安装命令可用。".to_string()
            } else {
                "Add-AppxPackage 不可用，将使用便携版 fallback。".to_string()
            },
        },
        PackageCapability {
            id: "appx-service".to_string(),
            label: "AppXSvc".to_string(),
            status: if appx_svc && !appx_start.eq_ignore_ascii_case("Disabled") {
                Severity::Ok
            } else {
                Severity::Error
            },
            detail: if appx_svc {
                format!("AppXSvc 启动类型：{appx_start}")
            } else {
                "AppXSvc 服务缺失。".to_string()
            },
        },
        PackageCapability {
            id: "msix-runtime".to_string(),
            label: "MSIX 运行时".to_string(),
            status: if package_manager {
                Severity::Ok
            } else {
                Severity::Error
            },
            detail: if package_manager {
                "Windows PackageManager 可激活。".to_string()
            } else {
                msix_runtime_unavailable_message(Some(package_manager_error))
            },
        },
    ]
}

pub fn probe_macos_dmg_capabilities() -> Vec<PackageCapability> {
    if !cfg!(target_os = "macos") {
        return vec![PackageCapability {
            id: "platform".to_string(),
            label: "平台".to_string(),
            status: Severity::Info,
            detail: "当前不是 macOS，DMG 安装链路不可用。".to_string(),
        }];
    }

    let hdiutil = command_available("hdiutil");
    let ditto = command_available("ditto");
    vec![
        PackageCapability {
            id: "hdiutil".to_string(),
            label: "hdiutil".to_string(),
            status: if hdiutil {
                Severity::Ok
            } else {
                Severity::Error
            },
            detail: if hdiutil {
                "DMG 挂载命令可用。".to_string()
            } else {
                "hdiutil 不可用，无法挂载 DMG。".to_string()
            },
        },
        PackageCapability {
            id: "ditto".to_string(),
            label: "ditto".to_string(),
            status: if ditto { Severity::Ok } else { Severity::Error },
            detail: if ditto {
                "应用复制命令可用。".to_string()
            } else {
                "ditto 不可用，无法复制 .app 应用包。".to_string()
            },
        },
    ]
}

pub fn launch_msix_package(package_identity: &str) -> Result<(), String> {
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$pkg = Get-AppxPackage -Name {name} | Sort-Object -Property Version -Descending | Select-Object -First 1
if ($null -eq $pkg) {{ throw 'Package is not installed' }}
$app = (Get-AppxPackageManifest $pkg).Package.Applications.Application
if ($app -is [array]) {{ $app = $app[0] }}
$id = $app.Id
if (-not $id) {{ $id = 'App' }}
Start-Process ("shell:AppsFolder\" + $pkg.PackageFamilyName + "!" + $id)
"#,
        name = ps_quote(package_identity)
    );
    run_powershell(&script).map(|_| ())
}

pub fn launch_macos_app(path: &Path) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("当前平台暂不支持启动 macOS 应用。".to_string());
    }

    hidden_command("open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("启动 macOS 应用失败：{err}"))
}

pub fn quit_macos_app(app_name: &str) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }

    let display_name = app_name.trim_end_matches(".app");
    let script = format!("tell application \"{display_name}\" to quit");
    let _ = hidden_command("osascript").args(["-e", &script]).output();
    thread::sleep(Duration::from_secs(3));

    if !macos_process_running(display_name) {
        return Ok(());
    }
    let _ = hidden_command("pkill")
        .args(["-TERM", "-x", display_name])
        .output();
    thread::sleep(Duration::from_secs(1));
    if !macos_process_running(display_name) {
        return Ok(());
    }
    let _ = hidden_command("pkill")
        .args(["-KILL", "-x", display_name])
        .output();
    thread::sleep(Duration::from_millis(500));
    if macos_process_running(display_name) {
        Err(format!("{display_name} 仍在运行。"))
    } else {
        Ok(())
    }
}

pub fn install_macos_dmg(
    dmg_path: &Path,
    app_name: &str,
    destination: &Path,
    bundle_identifier: Option<&str>,
) -> Result<MacosDmgInstallReport, String> {
    if !cfg!(target_os = "macos") {
        return Err("当前平台暂不支持安装 macOS DMG。".to_string());
    }
    if !dmg_path.is_file() {
        return Err("macOS DMG 安装包不存在。".to_string());
    }
    let parent = destination
        .parent()
        .ok_or_else(|| "macOS 应用安装位置缺少父目录。".to_string())?;
    fs::create_dir_all(parent).map_err(|err| format!("创建 macOS 应用父目录失败：{err}"))?;

    let mount_point = temporary_macos_mount_point();
    if mount_point.exists() {
        fs::remove_dir_all(&mount_point).map_err(|err| format!("清理旧挂载目录失败：{err}"))?;
    }
    fs::create_dir_all(&mount_point).map_err(|err| format!("创建 DMG 挂载目录失败：{err}"))?;

    let attach = hidden_command("hdiutil")
        .arg("attach")
        .arg("-nobrowse")
        .arg("-readonly")
        .arg("-mountpoint")
        .arg(&mount_point)
        .arg(dmg_path)
        .output()
        .map_err(|err| format!("启动 hdiutil 挂载 DMG 失败：{err}"))?;
    if !attach.status.success() {
        let _ = fs::remove_dir_all(&mount_point);
        return Err(format!(
            "挂载 macOS DMG 失败：{}",
            String::from_utf8_lossy(&attach.stderr).trim()
        ));
    }

    let install_result =
        install_macos_app_from_mount(&mount_point, app_name, destination, bundle_identifier);
    let detach_result = detach_macos_mount(&mount_point);
    let _ = fs::remove_dir_all(&mount_point);

    let mut report = install_result?;
    if let Err(err) = detach_result {
        report.notes.push(err);
    }
    Ok(report)
}

pub fn msix_runtime_unavailable_message(detail: Option<&str>) -> String {
    let mut message = "Windows MSIX 部署运行时不可用，通常出现在精简版系统、虚拟机镜像或被移除 App Installer/AppXSvc/应用部署组件的环境。本应用会自动改用便携版安装；如需使用 MSIX，请恢复 App Installer、启用 AppXSvc，并确保 Windows 应用部署运行时完整。".to_string();
    if let Some(detail) = detail.map(str::trim).filter(|value| !value.is_empty()) {
        message.push_str(" 原始错误：");
        message.push_str(detail);
    }
    message
}

pub fn create_portable_start_menu_shortcut(
    registration: &PortableAppRegistration<'_>,
) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Ok(());
    }

    let shortcut = start_menu_shortcut_path(registration.shortcut_name)?;
    let exe = registration.install_root.join(registration.executable_name);
    let script = format!(
        r#"
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut({shortcut})
$shortcut.TargetPath = {target}
$shortcut.WorkingDirectory = {workdir}
$shortcut.IconLocation = {icon}
$shortcut.Save()
"#,
        shortcut = ps_quote(&shortcut.to_string_lossy()),
        target = ps_quote(&exe.to_string_lossy()),
        workdir = ps_quote(&registration.install_root.to_string_lossy()),
        icon = ps_quote(&format!("{},0", exe.to_string_lossy()))
    );
    run_powershell(&script).map(|_| ())
}

pub fn create_portable_uninstall_entry(
    registration: &PortableAppRegistration<'_>,
) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Ok(());
    }

    let exe = registration.install_root.join(registration.executable_name);
    let escaped_root = registration
        .install_root
        .to_string_lossy()
        .replace('\'', "''");
    let escaped_shortcut_name = registration.shortcut_name.replace('\'', "''");
    let escaped_uninstall_key = registration.uninstall_key.replace('\'', "''");
    let uninstall_script = format!(
        "if ($env:APPDATA) {{ $Shortcut = Join-Path $env:APPDATA 'Microsoft\\Windows\\Start Menu\\Programs\\{escaped_shortcut_name}'; Remove-Item -LiteralPath $Shortcut -Force -ErrorAction SilentlyContinue }}; Remove-Item -LiteralPath '{escaped_root}' -Recurse -Force -ErrorAction SilentlyContinue; Remove-Item -LiteralPath '{escaped_uninstall_key}' -Recurse -Force -ErrorAction SilentlyContinue"
    );
    let uninstall_string = format!(
        "powershell.exe -NoProfile -ExecutionPolicy Bypass -Command \"{uninstall_script}\""
    );
    let script = format!(
        r#"
$key = {key}
New-Item -Path $key -Force | Out-Null
New-ItemProperty -Path $key -Name DisplayName -Value {display_name} -PropertyType String -Force | Out-Null
New-ItemProperty -Path $key -Name DisplayVersion -Value {version} -PropertyType String -Force | Out-Null
New-ItemProperty -Path $key -Name Publisher -Value {publisher} -PropertyType String -Force | Out-Null
New-ItemProperty -Path $key -Name InstallLocation -Value {install_root} -PropertyType String -Force | Out-Null
New-ItemProperty -Path $key -Name DisplayIcon -Value {icon} -PropertyType String -Force | Out-Null
New-ItemProperty -Path $key -Name UninstallString -Value {uninstall_string} -PropertyType String -Force | Out-Null
New-ItemProperty -Path $key -Name QuietUninstallString -Value {uninstall_string} -PropertyType String -Force | Out-Null
New-ItemProperty -Path $key -Name NoModify -Value 1 -PropertyType DWord -Force | Out-Null
New-ItemProperty -Path $key -Name NoRepair -Value 1 -PropertyType DWord -Force | Out-Null
"#,
        key = ps_quote(registration.uninstall_key),
        display_name = ps_quote(registration.display_name),
        version = ps_quote(registration.version),
        publisher = ps_quote(registration.publisher),
        install_root = ps_quote(&registration.install_root.to_string_lossy()),
        icon = ps_quote(&format!("{},0", exe.to_string_lossy())),
        uninstall_string = ps_quote(&uninstall_string)
    );
    run_powershell(&script).map(|_| ())
}

pub fn remove_portable_start_menu_shortcut(shortcut_name: &str) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Ok(());
    }

    let shortcut = start_menu_shortcut_path(shortcut_name)?;
    if shortcut.exists() {
        fs::remove_file(shortcut).map_err(|err| err.to_string())?;
    }
    Ok(())
}

pub fn remove_portable_uninstall_entry(uninstall_key: &str) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Ok(());
    }

    let script = format!(
        r#"
$key = {key}
if (Test-Path $key) {{
  Remove-Item -Path $key -Recurse -Force
}}
"#,
        key = ps_quote(uninstall_key)
    );
    run_powershell(&script).map(|_| ())
}

fn macos_process_running(process_name: &str) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    hidden_command("pgrep")
        .args(["-x", process_name])
        .output()
        .map(|output| output.status.success() && !output.stdout.is_empty())
        .unwrap_or(false)
}

fn command_available(command: &str) -> bool {
    hidden_command("which")
        .arg(command)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn temporary_macos_mount_point() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "codestudio-lite-codex-dmg-{}-{suffix}",
        std::process::id()
    ))
}

fn install_macos_app_from_mount(
    mount_point: &Path,
    app_name: &str,
    destination: &Path,
    bundle_identifier: Option<&str>,
) -> Result<MacosDmgInstallReport, String> {
    let source_app = find_macos_app_bundle(mount_point, app_name)?;
    let parent = destination
        .parent()
        .ok_or_else(|| "macOS 应用安装位置缺少父目录。".to_string())?;
    let rollback = parent.join(format!(
        "{}.rollback",
        destination
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("Codex")
    ));
    if rollback.exists() {
        fs::remove_dir_all(&rollback).map_err(|err| format!("清理旧回滚目录失败：{err}"))?;
    }

    let had_previous = destination.exists();
    if had_previous {
        fs::rename(destination, &rollback)
            .map_err(|err| format!("创建 macOS 回滚备份失败：{err}"))?;
    }

    let copy = hidden_command("ditto")
        .arg(&source_app)
        .arg(destination)
        .output()
        .map_err(|err| format!("启动 ditto 复制 macOS 应用失败：{err}"));
    match copy {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            if had_previous && rollback.exists() {
                let _ = fs::rename(&rollback, destination);
            }
            return Err(format!(
                "复制 macOS 应用失败，已尝试回滚：{}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Err(err) => {
            if had_previous && rollback.exists() {
                let _ = fs::rename(&rollback, destination);
            }
            return Err(err);
        }
    }

    let mut notes = Vec::new();
    if had_previous && rollback.exists() {
        if let Err(err) = fs::remove_dir_all(&rollback) {
            notes.push(format!("macOS 回滚备份清理失败：{err}"));
        }
    }
    let installed =
        detect_macos_app(&[destination.to_path_buf()], bundle_identifier).or_else(|| {
            Some(InstalledMacosApp {
                path: destination.to_string_lossy().to_string(),
                version: "installed".to_string(),
                bundle_identifier: bundle_identifier.map(str::to_string),
            })
        });
    Ok(MacosDmgInstallReport { installed, notes })
}

fn find_macos_app_bundle(root: &Path, app_name: &str) -> Result<PathBuf, String> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).map_err(|err| format!("扫描 DMG 挂载目录失败：{err}"))?
        {
            let entry = entry.map_err(|err| format!("读取 DMG 挂载目录项失败：{err}"))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|err| format!("读取 DMG 文件类型失败：{err}"))?;
            if file_type.is_dir()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == app_name)
            {
                return Ok(path);
            }
            if file_type.is_dir() {
                stack.push(path);
            }
        }
    }
    Err(format!("DMG 中没有找到 {app_name}。"))
}

fn detach_macos_mount(mount_point: &Path) -> Result<(), String> {
    let output = hidden_command("hdiutil")
        .arg("detach")
        .arg(mount_point)
        .arg("-quiet")
        .output()
        .map_err(|err| format!("启动 hdiutil 卸载 DMG 失败：{err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let forced = hidden_command("hdiutil")
        .arg("detach")
        .arg(mount_point)
        .arg("-force")
        .arg("-quiet")
        .output()
        .map_err(|err| format!("启动 hdiutil 强制卸载 DMG 失败：{err}"))?;
    if forced.status.success() {
        Ok(())
    } else {
        Err(format!(
            "DMG 挂载点卸载失败：{}",
            String::from_utf8_lossy(&forced.stderr).trim()
        ))
    }
}

fn start_menu_shortcut_path(shortcut_name: &str) -> Result<PathBuf, String> {
    let appdata = std::env::var_os("APPDATA").ok_or_else(|| "APPDATA 不可用。".to_string())?;
    Ok(PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join(shortcut_name))
}

fn read_macos_plist_value(app: &Path, key: &str) -> Option<String> {
    let plist = app.join("Contents").join("Info.plist");
    let text = fs::read_to_string(plist).ok()?;
    plist_string_value(&text, key)
}

fn plist_string_value(text: &str, key: &str) -> Option<String> {
    let key_marker = format!("<key>{key}</key>");
    let key_index = text.find(&key_marker)?;
    let rest = &text[key_index + key_marker.len()..];
    let string_index = rest.find("<string>")? + "<string>".len();
    let rest = &rest[string_index..];
    let end = rest.find("</string>")?;
    Some(rest[..end].trim().to_string())
}

#[cfg(windows)]
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(not(windows))]
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::plist_string_value;

    #[test]
    fn reads_string_values_from_macos_info_plist() {
        let plist = r#"
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>com.openai.codex</string>
  <key>CFBundleShortVersionString</key>
  <string>1.2.3</string>
</dict>
</plist>
"#;

        assert_eq!(
            plist_string_value(plist, "CFBundleIdentifier").as_deref(),
            Some("com.openai.codex")
        );
        assert_eq!(
            plist_string_value(plist, "CFBundleShortVersionString").as_deref(),
            Some("1.2.3")
        );
    }
}
