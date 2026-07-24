use super::super::*;

pub(in crate::core::profile) fn sync_active_profiles_from_native_configs(
    config: &mut AppConfig,
    drafts: &mut Vec<ProfileDraft>,
    paths: &crate::core::app_paths::AppPaths,
) -> Result<bool, String> {
    let mut changed = false;

    let codex_config =
        fs::read_to_string(paths.home_dir.join(".codex").join("config.toml")).unwrap_or_default();
    if let Ok(codex_config) = parse_toml_or_empty(&codex_config, "Codex config") {
        let codex_auth = native::codex::read_auth_json(paths).ok();
        changed |= sync_or_import_native_config_profile(
            config,
            drafts,
            "codex",
            |profile| {
                native::codex::config_matches_profile_with_auth(
                    &codex_config,
                    codex_auth.as_ref(),
                    profile,
                    SecretMatchMode::KeychainReference,
                )
            },
            || native::codex::detect_native_profile_with_auth(&codex_config, codex_auth.as_ref()),
        )?;
    }

    changed |= sync_claude_desktop_config_profile(config, drafts, paths)?;

    if let Some(adapter) = native::adapter("claude") {
        let claude_config = fs::read_to_string(adapter.target(paths)).unwrap_or_default();
        changed |= sync_or_import_native_config_profile(
            config,
            drafts,
            "claude",
            |profile| {
                adapter
                    .matches(&claude_config, profile, SecretMatchMode::KeychainReference)
                    .unwrap_or(false)
            },
            || adapter.inspect(&claude_config).ok().flatten(),
        )?;
    }

    if let Some(adapter) = native::adapter("gemini-code-assist") {
        let settings = fs::read_to_string(adapter.target(paths)).unwrap_or_default();
        changed |= sync_or_import_native_config_profile(
            config,
            drafts,
            "gemini-code-assist",
            |profile| {
                adapter
                    .matches(&settings, profile, SecretMatchMode::KeychainReference)
                    .unwrap_or(false)
            },
            || adapter.inspect(&settings).ok().flatten(),
        )?;
    }

    if let Some(adapter) = native::adapter("opencode") {
        let opencode_config = fs::read_to_string(adapter.target(paths)).unwrap_or_default();
        changed |= sync_or_import_native_config_profile(
            config,
            drafts,
            "opencode",
            |profile| {
                adapter
                    .matches(
                        &opencode_config,
                        profile,
                        SecretMatchMode::KeychainReference,
                    )
                    .unwrap_or(false)
            },
            || adapter.inspect(&opencode_config).ok().flatten(),
        )?;
    }

    if let Some(adapter) = native::adapter("openclaw") {
        let openclaw_config = fs::read_to_string(adapter.target(paths)).unwrap_or_default();
        changed |= sync_or_import_native_config_profile(
            config,
            drafts,
            "openclaw",
            |profile| {
                adapter
                    .matches(
                        &openclaw_config,
                        profile,
                        SecretMatchMode::KeychainReference,
                    )
                    .unwrap_or(false)
            },
            || adapter.inspect(&openclaw_config).ok().flatten(),
        )?;
    }

    if let Some(adapter) = native::adapter("hermes") {
        let hermes_config = fs::read_to_string(adapter.target(paths)).unwrap_or_default();
        changed |= sync_or_import_native_config_profile(
            config,
            drafts,
            "hermes",
            |profile| {
                adapter
                    .matches(&hermes_config, profile, SecretMatchMode::KeychainReference)
                    .unwrap_or(false)
            },
            || adapter.inspect(&hermes_config).ok().flatten(),
        )?;
    }

    if let Some(adapter) = native::adapter("grok") {
        let grok_config = fs::read_to_string(adapter.target(paths)).unwrap_or_default();
        changed |= sync_or_import_native_config_profile(
            config,
            drafts,
            "grok",
            |profile| {
                adapter
                    .matches(&grok_config, profile, SecretMatchMode::KeychainReference)
                    .unwrap_or(false)
            },
            || adapter.inspect(&grok_config).ok().flatten(),
        )?;
    }

    if let Some(adapter) = native::adapter("pi") {
        let pi_config = fs::read_to_string(adapter.target(paths)).unwrap_or_default();
        changed |= sync_or_import_native_config_profile(
            config,
            drafts,
            "pi",
            |profile| {
                adapter
                    .matches(&pi_config, profile, SecretMatchMode::KeychainReference)
                    .unwrap_or(false)
            },
            || adapter.inspect(&pi_config).ok().flatten(),
        )?;
    }

    Ok(changed)
}

#[cfg(test)]
pub(in crate::core::profile) fn sync_codex_config_profile(
    config: &mut AppConfig,
    drafts: &[ProfileDraft],
    codex_config: &toml::Value,
) -> bool {
    sync_native_config_profile(config, drafts, "codex", |profile| {
        codex_direct_config_matches_profile(codex_config, None, profile)
    })
}

fn sync_claude_desktop_config_profile(
    config: &mut AppConfig,
    drafts: &mut Vec<ProfileDraft>,
    paths: &crate::core::app_paths::AppPaths,
) -> Result<bool, String> {
    let desktop_paths = claude_desktop_paths(paths).ok();
    let official = desktop_paths
        .as_ref()
        .map(claude_desktop_is_official)
        .unwrap_or(true);

    sync_or_import_native_config_profile(
        config,
        drafts,
        "claude-desktop",
        |profile| {
            claude_desktop_config_matches_profile_without_keychain(
                profile,
                desktop_paths.as_ref(),
                official,
            )
        },
        || {
            desktop_paths
                .as_ref()
                .and_then(detect_claude_desktop_native_profile)
        },
    )
}

#[cfg(test)]
pub(in crate::core::profile) fn sync_native_config_profile<F>(
    config: &mut AppConfig,
    drafts: &[ProfileDraft],
    app: &str,
    matches_profile: F,
) -> bool
where
    F: Fn(&ProfileDraft) -> bool,
{
    let current_active_id = config.active_profiles_by_mode.config.get(app).cloned();
    let matching_profiles = matching_native_config_profiles(drafts, app, &matches_profile);

    let selected_profile_id = current_active_id
        .as_ref()
        .and_then(|active_id| {
            matching_profiles
                .iter()
                .find(|profile| profile.id == *active_id)
                .map(|profile| profile.id.clone())
        })
        .or_else(|| matching_profiles.first().map(|profile| profile.id.clone()));

    match selected_profile_id {
        Some(profile_id) if config.active_profiles_by_mode.config.get(app) != Some(&profile_id) => {
            config
                .active_profiles_by_mode
                .config
                .insert(app.to_string(), profile_id);
            true
        }
        Some(_) => false,
        None => config.active_profiles_by_mode.config.remove(app).is_some(),
    }
}

fn sync_or_import_native_config_profile<F, G>(
    config: &mut AppConfig,
    drafts: &mut Vec<ProfileDraft>,
    app: &str,
    matches_profile: F,
    detect_profile: G,
) -> Result<bool, String>
where
    F: Fn(&ProfileDraft) -> bool,
    G: FnOnce() -> Option<DetectedNativeProfile>,
{
    let app = canonical_profile_app(app);
    let current_active_id = config.active_profiles_by_mode.config.get(&app).cloned();
    let detected = detect_profile();
    let (selected_profile_id, should_correct_detected_profile) = {
        let matching_profiles = matching_native_config_profiles(drafts, &app, &matches_profile);
        let selected_profile_id = current_active_id
            .as_ref()
            .and_then(|active_id| {
                matching_profiles
                    .iter()
                    .find(|profile| profile.id == *active_id)
                    .map(|profile| profile.id.clone())
            })
            .or_else(|| matching_profiles.first().map(|profile| profile.id.clone()));
        let should_correct_detected_profile = detected
            .as_ref()
            .map(|detected| {
                matching_profiles
                    .iter()
                    .any(|profile| should_correct_detected_native_profile(profile, &app, detected))
            })
            .unwrap_or(false);
        (selected_profile_id, should_correct_detected_profile)
    };

    if should_correct_detected_profile {
        if let Some(detected) = detected {
            let imported = upsert_detected_native_profile(drafts, detected)?;
            let changed = config.active_profiles_by_mode.config.get(&app) != Some(&imported.id);
            config
                .active_profiles_by_mode
                .config
                .insert(app, imported.id);
            return Ok(changed);
        }
    }

    if let Some(profile_id) = selected_profile_id {
        if config.active_profiles_by_mode.config.get(&app) != Some(&profile_id) {
            config
                .active_profiles_by_mode
                .config
                .insert(app, profile_id);
            return Ok(true);
        }
        return Ok(false);
    }

    if let Some(detected) = detected {
        let imported = upsert_detected_native_profile(drafts, detected)?;
        let changed = config.active_profiles_by_mode.config.get(&app) != Some(&imported.id);
        config
            .active_profiles_by_mode
            .config
            .insert(app, imported.id);
        return Ok(changed);
    }

    Ok(config.active_profiles_by_mode.config.remove(&app).is_some())
}

fn matching_native_config_profiles<'a, F>(
    drafts: &'a [ProfileDraft],
    app: &str,
    matches_profile: &F,
) -> Vec<&'a ProfileDraft>
where
    F: Fn(&ProfileDraft) -> bool,
{
    drafts
        .iter()
        .filter(|profile| {
            canonical_profile_app(&profile.app) == app
                && profile.mode == ProviderApplyMode::Config
                && matches_profile(profile)
        })
        .collect()
}

fn should_correct_detected_native_profile(
    profile: &ProfileDraft,
    app: &str,
    detected: &DetectedNativeProfile,
) -> bool {
    if profile.is_builtin
        || canonical_profile_app(&profile.app) != app
        || profile.mode != ProviderApplyMode::Config
        || !is_auto_imported_native_profile(profile)
    {
        return false;
    }

    let provider = normalize_detected_provider(&detected.provider, &detected.base_url);
    if profile.provider == provider {
        return false;
    }
    let Ok(protocol) = normalize_protocol(Some(&detected.protocol)) else {
        return false;
    };
    let Ok(base_url) = validate_base_url(&detected.base_url) else {
        return false;
    };
    let model = native_optional_model(&detected.model).unwrap_or_default();

    profile.protocol == protocol
        && profile.model.trim() == model
        && profile_runtime_base_url_matches(&protocol, &base_url, &profile.base_url)
}

fn upsert_detected_native_profile(
    drafts: &mut Vec<ProfileDraft>,
    detected: DetectedNativeProfile,
) -> Result<ProfileDraft, String> {
    let app = canonical_profile_app(&normalize_token("Tool", &detected.app)?);
    let provider = normalize_detected_provider(&detected.provider, &detected.base_url);
    if provider_is_official(&provider) {
        return Err("Detected Provider cannot be official.".to_string());
    }
    let protocol = normalize_protocol(Some(&detected.protocol))?;
    ensure_profile_protocol_supported_for_mode(
        &app,
        ProviderApplyMode::Config,
        &provider,
        &protocol,
    )?;
    let base_url = validate_base_url(&detected.base_url)?;
    let api_key = detected.api_key.trim();
    if api_key.is_empty() || looks_like_local_gateway_token(api_key) {
        return Err("Detected Provider API key is not importable.".to_string());
    }
    let model = native_optional_model(&detected.model).unwrap_or_default();
    let review_model = normalize_profile_review_model(&app, detected.review_model.as_deref());

    if let Some(existing) = drafts.iter().find(|profile| {
        detected_native_profile_matches_existing_reference(
            profile,
            &app,
            &provider,
            &protocol,
            &model,
            review_model.as_deref(),
            &base_url,
            api_key,
        )
    }) {
        return Ok(existing.clone());
    }

    if let Some(existing_index) = drafts.iter().position(|profile| {
        is_auto_imported_native_profile(profile)
            && detected_native_profile_identity_matches(profile, &app, &protocol, &model, &base_url)
            && profile_api_key_matches_config_without_keychain(profile, api_key)
    }) {
        let now = Utc::now().to_rfc3339();
        let mut updated = drafts[existing_index].clone();
        let old_provider = updated.provider.clone();
        updated.provider = provider;
        updated.review_model = review_model.clone();
        if is_auto_detected_native_profile_name(&updated.name, &app, &old_provider) {
            updated.name = unique_detected_native_profile_name_excluding(
                drafts,
                &app,
                &updated.provider,
                Some(&updated.id),
            );
        }
        if updated.auth_ref.is_none() {
            updated.auth_ref = Some(format!("keychain:codestudio-lite/{}/api_key", updated.id));
        }
        updated.updated_at = Some(now);
        updated.last_test_status = Some("detected".to_string());

        storage::save_profile(&updated)?;
        if let Some(auth_ref) = updated.auth_ref.as_deref() {
            credentials::store_keychain_secret(auth_ref, api_key)?;
        }
        drafts[existing_index] = updated.clone();
        drafts.sort_by(compare_profiles);
        activity_log::append(
            Severity::Info,
            format!(
                "Updated imported native config profile '{}' for {}/{}.",
                updated.name, updated.app, updated.provider
            ),
        )?;

        return Ok(updated);
    }

    let name = unique_detected_native_profile_name(drafts, &app, &provider);
    let id = unique_profile_id(&slugify(&name))?;
    let now = Utc::now().to_rfc3339();
    let auth_ref = Some(format!("keychain:codestudio-lite/{id}/api_key"));
    let sort_order = storage::next_profile_sort_order(&app, &ProviderApplyMode::Config)?;
    let draft = ProfileDraft {
        id,
        name,
        icon: None,
        remark: None,
        app,
        is_builtin: false,
        mode: ProviderApplyMode::Config,
        provider,
        protocol,
        model,
        review_model,
        model_mappings: Vec::new(),
        base_url,
        auth_ref,
        created_at: Some(now.clone()),
        updated_at: Some(now),
        last_test_status: Some("detected".to_string()),
        usage_enabled: false,
        sort_order,
    };

    storage::save_profile(&draft)?;
    if let Some(auth_ref) = draft.auth_ref.as_deref() {
        credentials::store_keychain_secret(auth_ref, api_key)?;
    }
    drafts.push(draft.clone());
    drafts.sort_by(compare_profiles);
    activity_log::append(
        Severity::Info,
        format!(
            "Imported existing native config as profile '{}' for {}/{}.",
            draft.name, draft.app, draft.provider
        ),
    )?;

    Ok(draft)
}

#[cfg(test)]
pub(in crate::core::profile) fn detected_native_profile_matches_existing_key(
    profile: &ProfileDraft,
    app: &str,
    provider: &str,
    protocol: &str,
    model: &str,
    review_model: Option<&str>,
    base_url: &str,
    api_key: &str,
) -> bool {
    detected_native_profile_matches_existing_secret(
        profile,
        app,
        provider,
        protocol,
        model,
        review_model,
        base_url,
        api_key,
        SecretMatchMode::ExactKeychain,
    )
}

pub(in crate::core::profile) fn detected_native_profile_matches_existing_reference(
    profile: &ProfileDraft,
    app: &str,
    provider: &str,
    protocol: &str,
    model: &str,
    review_model: Option<&str>,
    base_url: &str,
    api_key: &str,
) -> bool {
    detected_native_profile_matches_existing_secret(
        profile,
        app,
        provider,
        protocol,
        model,
        review_model,
        base_url,
        api_key,
        SecretMatchMode::KeychainReference,
    )
}

fn detected_native_profile_matches_existing_secret(
    profile: &ProfileDraft,
    app: &str,
    provider: &str,
    protocol: &str,
    model: &str,
    review_model: Option<&str>,
    base_url: &str,
    api_key: &str,
    secret_match: SecretMatchMode,
) -> bool {
    profile.provider == provider
        && detected_native_profile_base_matches(
            profile,
            app,
            protocol,
            model,
            review_model,
            base_url,
        )
        && profile_api_key_matches_config(profile, api_key, secret_match)
}

fn detected_native_profile_base_matches(
    profile: &ProfileDraft,
    app: &str,
    protocol: &str,
    model: &str,
    review_model: Option<&str>,
    base_url: &str,
) -> bool {
    detected_native_profile_identity_matches(profile, app, protocol, model, base_url)
        && effective_profile_review_model(app, profile.review_model.as_deref(), &profile.model)
            == effective_profile_review_model(app, review_model, model)
}

fn detected_native_profile_identity_matches(
    profile: &ProfileDraft,
    app: &str,
    protocol: &str,
    model: &str,
    base_url: &str,
) -> bool {
    !profile.is_builtin
        && canonical_profile_app(&profile.app) == app
        && profile.mode == ProviderApplyMode::Config
        && profile.protocol == protocol
        && profile.model.trim() == model
        && profile_runtime_base_url_matches(protocol, base_url, &profile.base_url)
}

fn unique_detected_native_profile_name(
    drafts: &[ProfileDraft],
    app: &str,
    provider: &str,
) -> String {
    unique_detected_native_profile_name_excluding(drafts, app, provider, None)
}

fn unique_detected_native_profile_name_excluding(
    drafts: &[ProfileDraft],
    app: &str,
    provider: &str,
    exclude_id: Option<&str>,
) -> String {
    let base = format!("{} {}", native_profile_tool_name(app), provider);
    let existing = drafts
        .iter()
        .filter(|profile| exclude_id != Some(profile.id.as_str()))
        .map(|profile| profile.name.as_str())
        .collect::<HashSet<_>>();
    for index in 0..1000 {
        let candidate = if index == 0 {
            base.clone()
        } else {
            format!("{base} {index}")
        };
        if !existing.contains(candidate.as_str()) {
            return candidate;
        }
    }
    base
}

pub(in crate::core::profile) fn is_auto_detected_native_profile_name(
    name: &str,
    app: &str,
    provider: &str,
) -> bool {
    let base = format!("{} {}", native_profile_tool_name(app), provider);
    if name == base {
        return true;
    }
    name.strip_prefix(&(base + " "))
        .map(|suffix| !suffix.is_empty() && suffix.chars().all(|item| item.is_ascii_digit()))
        .unwrap_or(false)
}

fn is_auto_imported_native_profile(profile: &ProfileDraft) -> bool {
    profile.last_test_status.as_deref() == Some("detected")
        || is_auto_detected_native_profile_name(&profile.name, &profile.app, &profile.provider)
}

fn native_profile_tool_name(app: &str) -> &'static str {
    match canonical_profile_app(app).as_str() {
        "codex" => "Codex",
        "claude-desktop" => "Claude Desktop",
        "claude" => "Claude Code",
        "gemini-code-assist" => "Gemini Code Assist",
        "opencode" => "OpenCode",
        "openclaw" => "OpenClaw",
        "hermes" => "Hermes",
        "grok" => "Grok",
        "pi" => "Pi Agent",
        _ => "Tool",
    }
}

pub(in crate::core::profile) fn native_optional_model(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty() && trimmed != "codestudio-default").then(|| trimmed.to_string())
}

pub(in crate::core::profile) fn normalize_detected_provider(
    provider: &str,
    base_url: &str,
) -> String {
    let raw_provider = provider.trim();
    let raw_provider_lower = raw_provider.to_ascii_lowercase();
    let generated_codestudio_label = raw_provider_lower.starts_with("codestudio-")
        || raw_provider_lower.starts_with("codestudio ");
    let from_base_url = provider_slug_from_base_url(base_url);
    if generated_codestudio_label {
        if let Some(provider) = from_base_url.clone() {
            return provider;
        }
    }
    let from_provider = raw_provider
        .strip_prefix("codestudio-")
        .unwrap_or(raw_provider);
    if let Some(provider) = normalize_detected_provider_display_token(from_provider) {
        return provider;
    }
    let mut slug = slugify(from_provider);
    if let Some(stripped) = slug.strip_prefix("codestudio-") {
        slug = stripped.to_string();
    }
    if slug.is_empty()
        || matches!(
            slug.as_str(),
            "official" | "codestudio-local" | "custom" | "provider"
        )
    {
        slug = from_base_url.unwrap_or_else(|| "custom".to_string());
    }
    if slug == "official" || slug == "codestudio-local" {
        "custom".to_string()
    } else {
        slug
    }
}

fn normalize_detected_provider_display_token(provider: &str) -> Option<String> {
    let provider = provider.trim().to_ascii_lowercase();
    if !provider.contains('.')
        || provider.starts_with('.')
        || provider.ends_with('.')
        || !provider.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return None;
    }
    provider_slug_from_base_url(&provider).or(Some(provider))
}

pub(in crate::core::profile) fn provider_slug_from_base_url(base_url: &str) -> Option<String> {
    let host = base_url
        .trim()
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(base_url)
        .split('/')
        .next()
        .unwrap_or_default()
        .split('@')
        .next_back()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .trim_matches('.')
        .to_ascii_lowercase();
    let mut parts = host
        .split('.')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    while matches!(parts.first(), Some(&"api" | &"gateway" | &"router")) && parts.len() > 2 {
        parts.remove(0);
    }
    let provider = if parts.len() >= 2 {
        parts[parts.len() - 2..].join(".")
    } else {
        parts.join(".")
    };
    (!provider.is_empty())
        .then(|| provider)
        .filter(|slug| !slug.is_empty())
}

#[cfg(test)]
pub(in crate::core::profile) fn claude_desktop_config_matches_profile(
    profile: &ProfileDraft,
    paths: Option<&ClaudeDesktopPaths>,
    official: bool,
) -> bool {
    native::claude_desktop::config_matches_profile(
        profile,
        paths,
        official,
        SecretMatchMode::ExactKeychain,
    )
}

fn claude_desktop_config_matches_profile_without_keychain(
    profile: &ProfileDraft,
    paths: Option<&ClaudeDesktopPaths>,
    official: bool,
) -> bool {
    native::claude_desktop::config_matches_profile(
        profile,
        paths,
        official,
        SecretMatchMode::KeychainReference,
    )
}
