use crate::core::platform::windows_native_architecture;

pub(crate) const LATEST_MACOS_URL: &str =
    "https://downloads.claude.ai/releases/darwin/universal/.latest";

pub(crate) fn windows_release_architecture() -> Result<&'static str, String> {
    windows_native_architecture()
}

pub(crate) fn claude_desktop_windows_latest_url() -> Result<String, String> {
    claude_desktop_windows_latest_url_for_arch(windows_release_architecture()?)
}

pub(crate) fn claude_desktop_windows_latest_url_for_arch(
    architecture: &str,
) -> Result<String, String> {
    validate_architecture(architecture)?;
    Ok(format!(
        "https://downloads.claude.ai/releases/win32/{architecture}/.latest"
    ))
}

pub(crate) fn claude_desktop_windows_msix_url() -> Result<String, String> {
    claude_desktop_windows_msix_url_for_arch(windows_release_architecture()?)
}

pub(crate) fn claude_desktop_windows_msix_url_for_arch(
    architecture: &str,
) -> Result<String, String> {
    validate_architecture(architecture)?;
    Ok(format!(
        "https://claude.ai/api/desktop/win32/{architecture}/msix/latest/redirect"
    ))
}

pub(crate) fn claude_desktop_windows_update_command() -> Result<String, String> {
    Ok(format!(
        "Download and install the latest Claude Desktop MSIX from {} with Add-AppxPackage -Path",
        claude_desktop_windows_msix_url()?
    ))
}

fn validate_architecture(architecture: &str) -> Result<(), String> {
    match architecture {
        "arm64" | "x64" => Ok(()),
        architecture => Err(format!(
            "Claude Desktop has no Windows installer for architecture {architecture}."
        )),
    }
}
