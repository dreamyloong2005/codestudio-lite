use super::super::*;
use super::NativeProfileAdapter;

const API_KEY_SETTING: &str = "geminicodeassist.geminiApiKey";
#[cfg(test)]
pub(in crate::core::profile) const GEMINI_CODE_ASSIST_API_KEY_SETTING: &str = API_KEY_SETTING;

pub(in crate::core::profile) static GEMINI_CODE_ASSIST_ADAPTER: GeminiCodeAssistAdapter =
    GeminiCodeAssistAdapter;
pub(in crate::core::profile) struct GeminiCodeAssistAdapter;

impl NativeProfileAdapter for GeminiCodeAssistAdapter {
    fn supports_mode(&self, mode: ProviderApplyMode) -> bool {
        mode == ProviderApplyMode::Config
    }
    fn target(&self, paths: &crate::core::app_paths::AppPaths) -> PathBuf {
        vs_code_user_settings_path(paths)
    }
    fn render(
        &self,
        current: &str,
        profile: &ProfileDraft,
        mode: ProviderApplyMode,
    ) -> Result<String, String> {
        if !self.supports_mode(mode) {
            return Err("Gemini Code Assist does not support Gateway profiles.".to_string());
        }
        if provider_is_official(&profile.provider) {
            official_content(current)
        } else {
            settings_content(current, profile)
        }
    }
    fn render_preview(
        &self,
        current: &str,
        profile: &ProfileDraft,
        mode: ProviderApplyMode,
    ) -> Result<String, String> {
        if !self.supports_mode(mode) {
            return Err("Gemini Code Assist does not support Gateway profiles.".to_string());
        }
        if provider_is_official(&profile.provider) {
            official_content(current)
        } else {
            settings_content_with_api_key(current, profile, secret_preview(profile))
        }
    }
    fn cleanup_gateway(&self, current: &str) -> Result<String, String> {
        Ok(current.to_string())
    }
    fn inspect(&self, current: &str) -> Result<Option<DetectedNativeProfile>, String> {
        Ok(detect_native_profile(&parse_json5_or_empty(
            current,
            "VS Code user settings",
        )?))
    }
    fn matches(
        &self,
        current: &str,
        profile: &ProfileDraft,
        secret_match: SecretMatchMode,
    ) -> Result<bool, String> {
        Ok(settings_match_profile_with_secret_match(
            &parse_json5_or_empty(current, "VS Code user settings")?,
            profile,
            secret_match,
        ))
    }
    fn verify(
        &self,
        path: &Path,
        profile: &ProfileDraft,
        mode: ProviderApplyMode,
    ) -> Result<bool, String> {
        if !self.supports_mode(mode) {
            return Err("Gemini Code Assist does not support Gateway profiles.".to_string());
        }
        verify_settings(path, profile)
    }
    fn preview(
        &self,
        profile: &ProfileDraft,
        path: PathBuf,
        display_path: String,
        mode: ProviderApplyMode,
    ) -> Result<NativeConfigPreview, String> {
        if !self.supports_mode(mode) {
            return Err("Gemini Code Assist does not support Gateway profiles.".to_string());
        }
        let official = provider_is_official(&profile.provider);
        let mut warnings = if official {
            vec![
                "Official provider restores Gemini Code Assist to its own login.".to_string(),
                "CodeStudio Lite removes the managed API key setting from VS Code user settings."
                    .to_string(),
            ]
        } else {
            vec!["Gemini Code Assist stores its API key in VS Code user settings.".to_string(), "The public Gemini Code Assist VS Code setting exposes the API key; Provider Base URL and model are kept in CodeStudio Lite but are not written to the extension config.".to_string(), "Restart VS Code or reload the Gemini Code Assist extension after applying so settings reload.".to_string()]
        };
        let (json, status) = read_json_preview(&path, "VS Code user settings", &mut warnings)?;
        let mut changes = if official {
            vec![json_diff_remove_line(
                &json,
                &[API_KEY_SETTING],
                "Removes the CodeStudio Lite managed Gemini Code Assist API key.",
            )]
        } else {
            vec![json_diff_line(
                &json,
                &[API_KEY_SETTING],
                secret_preview(profile),
                "Stores the selected Provider API key for Gemini Code Assist.",
            )]
        };
        if !official {
            changes.push(diff_value_line("Provider Base URL".to_string(), None, Some(profile.base_url.trim().to_string()), "Gemini Code Assist does not expose a VS Code setting for custom Base URL; this stays in the CodeStudio Lite profile."));
            if let Some(model) = profile_model(profile) {
                changes.push(diff_value_line("Model".to_string(), None, Some(model.to_string()), "Gemini Code Assist does not expose a VS Code setting for model override; this stays in the CodeStudio Lite profile."));
            }
        }
        Ok(NativeConfigPreview {
            tool: "gemini-code-assist".to_string(),
            path: display_path,
            status,
            write_enabled: true,
            changes,
            warnings,
            content: None,
        })
    }
}

pub(in crate::core::profile) fn settings_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let api_key = load_provider_api_key_for_direct_config(profile)?;
    settings_content_with_api_key(current, profile, &api_key)
}

pub(in crate::core::profile) fn settings_content_with_api_key(
    current: &str,
    profile: &ProfileDraft,
    api_key: &str,
) -> Result<String, String> {
    require_profile_protocol(profile, &[PROTOCOL_GOOGLE_GEMINI])?;
    let mut value = parse_json5_or_empty(current, "VS Code user settings")?;
    set_json_string_path(&mut value, &[API_KEY_SETTING], api_key);
    render_json_config(value, "VS Code user settings")
}

fn official_content(current: &str) -> Result<String, String> {
    let mut value = parse_json5_or_empty(current, "VS Code user settings")?;
    remove_json_path(&mut value, &[API_KEY_SETTING]);
    render_json_config(value, "VS Code user settings")
}

pub(in crate::core::profile) fn detect_native_profile(
    value: &serde_json::Value,
) -> Option<DetectedNativeProfile> {
    let api_key = json_string_lookup(value, &[API_KEY_SETTING])?
        .trim()
        .to_string();
    if api_key.is_empty() || looks_like_local_gateway_token(&api_key) {
        return None;
    }
    Some(DetectedNativeProfile {
        app: "gemini-code-assist".to_string(),
        provider: "gemini".to_string(),
        protocol: PROTOCOL_GOOGLE_GEMINI.to_string(),
        model: String::new(),
        review_model: None,
        base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
        api_key,
    })
}

pub(in crate::core::profile) fn settings_match_profile(
    value: &serde_json::Value,
    profile: &ProfileDraft,
) -> bool {
    settings_match_profile_with_secret_match(value, profile, SecretMatchMode::ExactKeychain)
}

fn settings_match_profile_with_secret_match(
    value: &serde_json::Value,
    profile: &ProfileDraft,
    secret_match: SecretMatchMode,
) -> bool {
    if provider_is_official(&profile.provider) {
        return canonical_profile_app(&profile.app) == "gemini-code-assist"
            && profile.mode == ProviderApplyMode::Config
            && normalize_protocol(Some(&profile.protocol)).as_deref()
                == Ok(PROTOCOL_GOOGLE_GEMINI)
            && json_string_lookup(value, &[API_KEY_SETTING]).is_none();
    }
    canonical_profile_app(&profile.app) == "gemini-code-assist"
        && profile.mode == ProviderApplyMode::Config
        && normalize_protocol(Some(&profile.protocol)).as_deref() == Ok(PROTOCOL_GOOGLE_GEMINI)
        && json_string_lookup(value, &[API_KEY_SETTING])
            .map(|token| profile_api_key_matches_config(profile, &token, secret_match))
            .unwrap_or(false)
}

fn verify_settings(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let value = parse_json5_or_empty(
        &fs::read_to_string(path).map_err(|err| err.to_string())?,
        "VS Code user settings",
    )?;
    Ok(settings_match_profile(&value, profile))
}

#[cfg(test)]
pub(in crate::core::profile) use detect_native_profile as detect_gemini_code_assist_native_profile;
#[cfg(test)]
pub(in crate::core::profile) use settings_match_profile as gemini_code_assist_settings_match_profile;
