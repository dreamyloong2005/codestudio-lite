pub(in crate::core::profile) mod claude;
pub(in crate::core::profile) mod claude_desktop;
pub(in crate::core::profile) mod codex;
pub(in crate::core::profile) mod gemini;
pub(in crate::core::profile) mod gemini_code_assist;
pub(in crate::core::profile) mod grok;
pub(in crate::core::profile) mod hermes;
pub(in crate::core::profile) mod openclaw;
pub(in crate::core::profile) mod opencode;
pub(in crate::core::profile) mod pi;
pub(in crate::core::profile) mod plan;

use super::{
    DetectedNativeProfile, NativeConfigPreview, ProfileDraft, ProviderApplyMode, SecretMatchMode,
};
use crate::core::app_paths::AppPaths;
use std::path::{Path, PathBuf};

pub(in crate::core::profile) fn model_from_provider_ref(
    value: Option<&str>,
    provider_id: &str,
) -> Option<String> {
    let value = value?.trim();
    let prefix = format!("{provider_id}/");
    value
        .strip_prefix(&prefix)
        .and_then(super::native_optional_model)
        .or_else(|| super::native_optional_model(value))
}

pub(in crate::core::profile) trait NativeProfileAdapter: Sync {
    fn supports_mode(&self, _mode: ProviderApplyMode) -> bool {
        true
    }
    fn target(&self, paths: &AppPaths) -> PathBuf;
    fn render(
        &self,
        current: &str,
        profile: &ProfileDraft,
        mode: ProviderApplyMode,
    ) -> Result<String, String>;
    fn render_preview(
        &self,
        current: &str,
        profile: &ProfileDraft,
        mode: ProviderApplyMode,
    ) -> Result<String, String>;
    fn cleanup_gateway(&self, current: &str) -> Result<String, String>;
    fn inspect(&self, current: &str) -> Result<Option<DetectedNativeProfile>, String>;
    fn matches(
        &self,
        current: &str,
        profile: &ProfileDraft,
        secret_match: SecretMatchMode,
    ) -> Result<bool, String>;
    fn verify(
        &self,
        path: &Path,
        profile: &ProfileDraft,
        mode: ProviderApplyMode,
    ) -> Result<bool, String>;
    fn preview(
        &self,
        profile: &ProfileDraft,
        path: PathBuf,
        display_path: String,
        mode: ProviderApplyMode,
    ) -> Result<NativeConfigPreview, String>;
}

pub(in crate::core::profile) fn adapter(
    tool_id: &str,
) -> Option<&'static dyn NativeProfileAdapter> {
    match tool_id {
        "claude" => Some(&claude::CLAUDE_ADAPTER),
        "codex" => Some(&codex::CODEX_ADAPTER),
        "gemini" => Some(&gemini::GEMINI_ADAPTER),
        "gemini-code-assist" => Some(&gemini_code_assist::GEMINI_CODE_ASSIST_ADAPTER),
        "grok" => Some(&grok::GROK_ADAPTER),
        "hermes" => Some(&hermes::HERMES_ADAPTER),
        "openclaw" => Some(&openclaw::OPENCLAW_ADAPTER),
        "opencode" => Some(&opencode::OPENCODE_ADAPTER),
        "pi" => Some(&pi::PI_ADAPTER),
        _ => None,
    }
}
