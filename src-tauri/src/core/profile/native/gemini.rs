use super::super::*;
use super::NativeProfileAdapter;

pub(in crate::core::profile) static GEMINI_ADAPTER: GeminiAdapter = GeminiAdapter;
pub(in crate::core::profile) struct GeminiAdapter;

impl NativeProfileAdapter for GeminiAdapter {
    fn target(&self, paths: &crate::core::app_paths::AppPaths) -> PathBuf {
        paths.home_dir.join(".gemini").join(".env")
    }

    fn render(
        &self,
        current: &str,
        profile: &ProfileDraft,
        mode: ProviderApplyMode,
    ) -> Result<String, String> {
        match mode {
            ProviderApplyMode::Config if provider_is_official(&profile.provider) => {
                Ok(gemini_official_env_content(current))
            }
            ProviderApplyMode::Config => gemini_env_content(current, profile),
            ProviderApplyMode::Gateway => gemini_gateway_env_content(current, profile),
        }
    }

    fn render_preview(
        &self,
        current: &str,
        profile: &ProfileDraft,
        mode: ProviderApplyMode,
    ) -> Result<String, String> {
        match mode {
            ProviderApplyMode::Config if provider_is_official(&profile.provider) => {
                Ok(gemini_official_env_content(current))
            }
            ProviderApplyMode::Config => {
                gemini_env_content_with_api_key(current, profile, secret_preview(profile))
            }
            ProviderApplyMode::Gateway => gemini_gateway_env_content(current, profile),
        }
    }

    fn cleanup_gateway(&self, current: &str) -> Result<String, String> {
        gemini_gateway_cleanup_env_content(current, "gemini")
    }

    fn inspect(&self, current: &str) -> Result<Option<DetectedNativeProfile>, String> {
        Ok(detect_gemini_native_profile(&parse_env_content(current)))
    }

    fn matches(
        &self,
        current: &str,
        profile: &ProfileDraft,
        secret_match: SecretMatchMode,
    ) -> Result<bool, String> {
        Ok(gemini_env_matches_profile_with_secret_match(
            &parse_env_content(current),
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
        match mode {
            ProviderApplyMode::Config => verify_gemini_env_config(path, profile),
            ProviderApplyMode::Gateway => verify_gemini_gateway_env_config(path, profile),
        }
    }

    fn preview(
        &self,
        profile: &ProfileDraft,
        path: PathBuf,
        display_path: String,
        mode: ProviderApplyMode,
    ) -> Result<NativeConfigPreview, String> {
        let official = provider_is_official(&profile.provider);
        let mut warnings = match (mode, official) {
            (ProviderApplyMode::Config, true) => vec![
                "Official provider restores Gemini CLI to its own login.".to_string(),
                "CodeStudio Lite removes managed API or Gateway values from ~/.gemini/.env."
                    .to_string(),
            ],
            (ProviderApplyMode::Config, false) => vec![
                "Gemini CLI reads API key and base URL from environment variables, so this adapter writes ~/.gemini/.env.".to_string(),
                "Restart Gemini CLI or open a new terminal session after applying so environment variables reload.".to_string(),
            ],
            (ProviderApplyMode::Gateway, _) => vec![
                "Gateway profiles write Gemini CLI environment values to the tool-scoped local gateway URL.".to_string(),
                "Restart Gemini CLI or open a new terminal session after applying so environment variables reload.".to_string(),
                "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running.".to_string(),
                "Real upstream Provider API keys stay in the system keychain and are used by the local gateway.".to_string(),
            ],
        };
        let (env, status) = read_env_preview(&path, &mut warnings)?;
        let changes = if official {
            vec![
                env_diff_remove_line(
                    &env,
                    "GEMINI_API_KEY",
                    "Removes the CodeStudio Lite managed Gemini API key.",
                ),
                env_diff_remove_line(
                    &env,
                    "GOOGLE_GEMINI_BASE_URL",
                    "Restores Gemini CLI to the client's own official endpoint.",
                ),
                env_diff_remove_line(
                    &env,
                    "GEMINI_MODEL",
                    "Removes the CodeStudio Lite managed model override.",
                ),
            ]
        } else if mode == ProviderApplyMode::Gateway {
            let client = gateway::client_config_for_tool("gemini")?;
            let model = gateway_config_model_for_profile(profile);
            vec![
                env_diff_line(&env, "GEMINI_API_KEY", &client.token_preview, "Stores only the local CodeStudio token, not the real upstream Provider API key."),
                env_diff_line(&env, "GOOGLE_GEMINI_BASE_URL", &client.base_url, "Points Gemini CLI at the tool-scoped CodeStudio Lite Local Gateway."),
                env_diff_line(&env, "GEMINI_MODEL", model, "Sets Gemini CLI to the virtual model name resolved by the Local Gateway."),
            ]
        } else {
            let mut changes = vec![
                env_diff_line(
                    &env,
                    "GEMINI_API_KEY",
                    secret_preview(profile),
                    "Stores the selected Provider API key for Gemini CLI.",
                ),
                env_diff_line(
                    &env,
                    "GOOGLE_GEMINI_BASE_URL",
                    &profile_runtime_base_url_for_protocol(&profile.protocol, &profile.base_url),
                    "Points Gemini CLI at the selected upstream Provider Base URL.",
                ),
            ];
            if let Some(model) = profile_model(profile) {
                changes.push(env_diff_line(
                    &env,
                    "GEMINI_MODEL",
                    model,
                    "Sets Gemini CLI to the selected upstream model.",
                ));
            } else {
                changes.push(env_diff_remove_line(
                    &env,
                    "GEMINI_MODEL",
                    "Model is optional; no Gemini model override will be written.",
                ));
            }
            changes
        };
        Ok(NativeConfigPreview {
            tool: "gemini".to_string(),
            path: display_path,
            status,
            write_enabled: true,
            changes,
            warnings,
            content: None,
        })
    }
}

pub(in crate::core::profile) fn detect_gemini_native_profile(
    env: &HashMap<String, String>,
) -> Option<DetectedNativeProfile> {
    let base_url = env.get("GOOGLE_GEMINI_BASE_URL")?.trim().to_string();
    if base_url.is_empty() {
        return None;
    }
    let api_key = env.get("GEMINI_API_KEY")?.trim().to_string();
    if api_key.is_empty() || looks_like_local_gateway_token(&api_key) {
        return None;
    }
    Some(DetectedNativeProfile {
        app: "gemini".to_string(),
        provider: provider_slug_from_base_url(&base_url).unwrap_or_else(|| "gemini".to_string()),
        protocol: PROTOCOL_GOOGLE_GEMINI.to_string(),
        model: env
            .get("GEMINI_MODEL")
            .and_then(|model| native_optional_model(model))
            .unwrap_or_default(),
        review_model: None,
        base_url,
        api_key,
    })
}

pub(in crate::core::profile) fn gemini_env_matches_profile(
    env: &HashMap<String, String>,
    profile: &ProfileDraft,
) -> bool {
    gemini_env_matches_profile_with_secret_match(env, profile, SecretMatchMode::ExactKeychain)
}

pub(in crate::core::profile) fn gemini_env_matches_profile_with_secret_match(
    env: &HashMap<String, String>,
    profile: &ProfileDraft,
    secret_match: SecretMatchMode,
) -> bool {
    if provider_is_official(&profile.provider) {
        return canonical_profile_app(&profile.app) == "gemini"
            && profile.mode == ProviderApplyMode::Config
            && normalize_protocol(Some(&profile.protocol)).as_deref()
                == Ok(PROTOCOL_GOOGLE_GEMINI)
            && !gemini_env_has_managed_endpoint(env);
    }
    if canonical_profile_app(&profile.app) != "gemini"
        || profile.mode != ProviderApplyMode::Config
        || normalize_protocol(Some(&profile.protocol)).as_deref() != Ok(PROTOCOL_GOOGLE_GEMINI)
    {
        return false;
    }
    let model_matches = match profile_model(profile) {
        Some(model) => env.get("GEMINI_MODEL").map(String::as_str) == Some(model),
        None => env.get("GEMINI_MODEL").is_none(),
    };
    let token_matches = env
        .get("GEMINI_API_KEY")
        .map(|token| profile_api_key_matches_config(profile, token, secret_match))
        .unwrap_or(false);
    env.get("GOOGLE_GEMINI_BASE_URL")
        .map(|base_url| {
            profile_runtime_base_url_matches(&profile.protocol, base_url, &profile.base_url)
        })
        .unwrap_or(false)
        && token_matches
        && model_matches
}

fn gemini_env_has_managed_endpoint(env: &HashMap<String, String>) -> bool {
    env.get("GOOGLE_GEMINI_BASE_URL").is_some() || env.get("GEMINI_API_KEY").is_some()
}

fn verify_gemini_env_config(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let env = parse_env_content(&content);
    if provider_is_official(&profile.provider) {
        return Ok(gemini_env_matches_profile(&env, profile));
    }
    let model_matches = match profile_model(profile) {
        Some(model) => env.get("GEMINI_MODEL").map(String::as_str) == Some(model),
        None => env.get("GEMINI_MODEL").is_none(),
    };
    let token_matches = env
        .get("GEMINI_API_KEY")
        .map(|token| profile_api_key_matches_config_by_reading_keychain(profile, token))
        .unwrap_or(false);
    Ok(env
        .get("GOOGLE_GEMINI_BASE_URL")
        .map(|base_url| {
            profile_runtime_base_url_matches(&profile.protocol, base_url, &profile.base_url)
        })
        .unwrap_or(false)
        && token_matches
        && model_matches)
}

fn verify_gemini_gateway_env_config(path: &Path, profile: &ProfileDraft) -> Result<bool, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let env = parse_env_content(&fs::read_to_string(path).map_err(|err| err.to_string())?);
    let model = gateway_config_model_for_profile(profile);
    Ok(
        env.get("GOOGLE_GEMINI_BASE_URL").map(String::as_str) == Some(client.base_url.as_str())
            && env.get("GEMINI_API_KEY").map(String::as_str) == Some(client.token.as_str())
            && env.get("GEMINI_MODEL").map(String::as_str) == Some(model),
    )
}

pub(in crate::core::profile) fn gemini_env_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let api_key = load_provider_api_key_for_direct_config(profile)?;
    gemini_env_content_with_api_key(current, profile, &api_key)
}

pub(in crate::core::profile) fn gemini_env_content_with_api_key(
    current: &str,
    profile: &ProfileDraft,
    api_key: &str,
) -> Result<String, String> {
    require_profile_protocol(profile, &[PROTOCOL_GOOGLE_GEMINI])?;
    let updates = vec![
        ("GEMINI_API_KEY", Some(api_key.to_string())),
        (
            "GOOGLE_GEMINI_BASE_URL",
            Some(profile_runtime_base_url_for_protocol(
                &profile.protocol,
                &profile.base_url,
            )),
        ),
        (
            "GEMINI_MODEL",
            profile_model(profile).map(ToString::to_string),
        ),
    ];

    Ok(update_env_content(current, &updates))
}

pub(in crate::core::profile) fn gemini_official_env_content(current: &str) -> String {
    update_env_content(
        current,
        &[
            ("GEMINI_API_KEY", None),
            ("GOOGLE_GEMINI_BASE_URL", None),
            ("GEMINI_MODEL", None),
        ],
    )
}

pub(in crate::core::profile) fn gemini_gateway_env_content(
    current: &str,
    profile: &ProfileDraft,
) -> Result<String, String> {
    let client = gateway::client_config_for_tool(&profile.app)?;
    let model = gateway_config_model_for_profile(profile);
    Ok(update_env_content(
        current,
        &[
            ("GEMINI_API_KEY", Some(client.token)),
            ("GOOGLE_GEMINI_BASE_URL", Some(client.base_url)),
            ("GEMINI_MODEL", Some(model.to_string())),
        ],
    ))
}

pub(in crate::core::profile) fn gemini_gateway_cleanup_env_content(
    current: &str,
    tool_id: &str,
) -> Result<String, String> {
    let client = gateway::client_config_for_tool(tool_id)?;
    let env = parse_env_content(current);
    let mut updates = Vec::new();

    if env
        .get("GEMINI_API_KEY")
        .map(String::as_str)
        .map(looks_like_local_gateway_token)
        .unwrap_or(false)
    {
        updates.push(("GEMINI_API_KEY", None));
    }
    if env.get("GOOGLE_GEMINI_BASE_URL").map(String::as_str) == Some(client.base_url.as_str()) {
        updates.push(("GOOGLE_GEMINI_BASE_URL", None));
    }
    if env.get("GEMINI_MODEL").map(String::as_str) == Some(client.model.as_str()) {
        updates.push(("GEMINI_MODEL", None));
    }
    if env.get("GEMINI_MODEL").map(String::as_str) == Some(GATEWAY_FALLBACK_MODEL) {
        updates.push(("GEMINI_MODEL", None));
    }

    Ok(update_env_content(current, &updates))
}
