use super::*;
mod native_sync;
pub(in crate::core::profile) use native_sync::sync_active_profiles_from_native_configs;
#[cfg(test)]
pub(in crate::core::profile) use native_sync::{
    claude_desktop_config_matches_profile, detected_native_profile_matches_existing_key,
    detected_native_profile_matches_existing_reference, is_auto_detected_native_profile_name,
    normalize_detected_provider, sync_codex_config_profile, sync_native_config_profile,
};
pub(in crate::core::profile) use native_sync::{
    native_optional_model, provider_slug_from_base_url,
};

pub fn switch_active_profile(
    request: SwitchActiveProfileRequest,
) -> Result<ProfileSummary, String> {
    ensure_app_dirs()?;

    let profile_id = normalize_token("Profile ID", &request.profile_id)?;
    let profiles = load_profiles()?;
    let profile = profiles
        .iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("Profile '{profile_id}' does not exist"))?;
    let mode = profile.mode.clone();
    if mode == ProviderApplyMode::Gateway && provider_is_official(&profile.provider) {
        return Err(
            "Official provider uses the client login directly and does not run through the local gateway."
                .to_string(),
        );
    }

    let mut config = read_app_config()?;
    activate_profile_for_tool(&mut config, profile, &profiles);
    write_app_config(&config)?;
    activity_log::append(
        Severity::Ok,
        format!(
            "Switched active profile for '{}' to '{}' in {:?} mode.",
            profile.app, profile.name, mode
        ),
    )?;

    load_profile_summary_without_native_sync()
}

pub fn save_profile_draft(request: SaveProfileDraftRequest) -> Result<ProfileDraft, String> {
    ensure_app_dirs()?;

    let plan = build_profile_write_plan(
        &request.name,
        &request.app,
        request.mode.as_ref(),
        &request.provider,
        request.protocol.as_deref(),
        &request.model,
        &request.base_url,
        request.secret_provided,
    )?;
    let model_mappings =
        normalize_profile_model_mappings(&plan.app, request.model_mappings.as_deref())?;
    let review_model = normalize_profile_review_model(&plan.app, request.review_model.as_deref());
    ensure_profile_tool_installed(&plan.app)?;
    let now = Utc::now().to_rfc3339();
    let sort_order = storage::next_profile_sort_order(&plan.app, &plan.mode)?;
    let draft = ProfileDraft {
        id: plan.id,
        name: plan.name,
        icon: normalize_profile_icon(request.icon.as_deref())?,
        remark: normalize_profile_remark(request.remark.as_deref()),
        app: plan.app,
        is_builtin: false,
        mode: plan.mode,
        provider: plan.provider,
        protocol: plan.protocol,
        model: plan.model,
        review_model,
        model_mappings,
        base_url: plan.base_url,
        auth_ref: plan.auth_ref,
        created_at: Some(now.clone()),
        updated_at: Some(now),
        last_test_status: Some("pending".to_string()),
        usage_enabled: false,
        sort_order,
    };

    capture_codex_oauth_profile_if_needed(&draft)?;
    storage::save_profile(&draft)?;
    if let (Some(auth_ref), Some(api_key)) = (draft.auth_ref.as_deref(), request.api_key.as_deref())
    {
        let trimmed = api_key.trim();
        if !trimmed.is_empty() {
            credentials::store_keychain_secret(auth_ref, trimmed)?;
        }
    }
    if draft.mode == ProviderApplyMode::Gateway {
        let mut config = read_app_config()?;
        let drafts = load_profiles()?;
        if activate_new_gateway_profile_if_unset(&mut config, &draft, &drafts) {
            write_app_config(&config)?;
        }
    }
    activity_log::append(
        Severity::Ok,
        format!(
            "Saved profile draft '{}' for {}/{}.",
            draft.name, draft.app, draft.provider
        ),
    )?;

    Ok(draft)
}

pub fn update_profile_draft(request: UpdateProfileDraftRequest) -> Result<ProfileDraft, String> {
    ensure_app_dirs()?;

    let profile_id = normalize_token("Profile ID", &request.profile_id)?;
    if is_builtin_profile_id(&profile_id) {
        return Err("Built-in official profiles cannot be modified.".to_string());
    }
    let existing = load_profile_by_id(&profile_id)?;
    if existing.is_builtin {
        return Err("Built-in official profiles cannot be modified.".to_string());
    }
    let name = normalize_required("Profile Name", &request.name)?;
    let provider = normalize_provider_token(&request.provider)?;
    let mode = normalize_profile_mode(&provider, request.mode.as_ref())?;
    let protocol = normalize_protocol(request.protocol.as_deref())?;
    let app = canonical_profile_app(&existing.app);
    ensure_custom_official_profile_allowed(&app, &provider, mode)?;
    ensure_profile_protocol_supported_for_mode(&app, mode, &provider, &protocol)?;
    let model = request.model.trim().to_string();
    let review_model = normalize_profile_review_model(&app, request.review_model.as_deref());
    let model_mappings = normalize_profile_model_mappings(
        &app,
        Some(
            request
                .model_mappings
                .as_deref()
                .unwrap_or(existing.model_mappings.as_slice()),
        ),
    )?;
    let base_url = validate_base_url_for_provider(&provider, &request.base_url)?;
    let now = Utc::now().to_rfc3339();
    let created_at = existing.created_at.clone().unwrap_or_else(|| now.clone());
    let api_key = request
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let auth_ref = if provider_is_official(&provider) {
        None
    } else if api_key.is_some() {
        Some(
            existing
                .auth_ref
                .clone()
                .unwrap_or_else(|| format!("keychain:codestudio-lite/{profile_id}/api_key")),
        )
    } else {
        existing.auth_ref.clone()
    };
    if provider_requires_api_key(&provider) && auth_ref.is_none() {
        return Err("Provider API key is required for non-official providers.".to_string());
    }
    let updated = ProfileDraft {
        id: profile_id.clone(),
        name,
        icon: normalize_profile_icon(request.icon.as_deref())?,
        remark: normalize_profile_remark(request.remark.as_deref()),
        app,
        is_builtin: false,
        mode,
        provider,
        protocol,
        model,
        review_model,
        model_mappings,
        base_url,
        auth_ref,
        created_at: Some(created_at.clone()),
        updated_at: Some(now.clone()),
        last_test_status: Some("pending".to_string()),
        usage_enabled: existing.usage_enabled,
        sort_order: existing.sort_order,
    };

    storage::save_profile(&updated)?;
    if let (Some(auth_ref), Some(api_key)) = (updated.auth_ref.as_deref(), api_key) {
        credentials::store_keychain_secret(auth_ref, api_key)?;
    }
    let mut config = read_app_config()?;
    let drafts = load_profiles()?;
    if clean_active_profiles(&mut config, &drafts) {
        write_app_config(&config)?;
    }
    // If this draft is the currently applied profile, rewrite the native tool
    // config before any later summary refresh can observe a temporary mismatch.
    if profile_is_active(&config, &updated) {
        rewrite_native_configs_for_profile(&updated, "update-active-profile")?;
    }

    activity_log::append(
        Severity::Ok,
        format!(
            "Updated profile draft '{}' for {}/{}.",
            updated.name, updated.app, updated.provider
        ),
    )?;

    Ok(updated)
}

pub fn duplicate_profile_draft(
    request: DuplicateProfileDraftRequest,
) -> Result<ProfileDraft, String> {
    ensure_app_dirs()?;

    let source_id = normalize_token("Profile ID", &request.profile_id)?;
    let source = load_profile_by_id(&source_id)?;
    if source.is_builtin || is_builtin_profile_id(&source.id) {
        return Err("Built-in official profiles cannot be duplicated.".to_string());
    }
    ensure_profile_tool_installed(&canonical_profile_app(&source.app))?;
    let new_id = unique_profile_id(&slugify(&source.name))?;
    let now = Utc::now().to_rfc3339();
    let app = canonical_profile_app(&source.app);
    let sort_order = storage::next_profile_sort_order(&app, &source.mode)?;
    let auth_ref = if provider_is_official(&source.provider) {
        None
    } else {
        source
            .auth_ref
            .as_ref()
            .map(|_| format!("keychain:codestudio-lite/{new_id}/api_key"))
    };

    if let (Some(source_auth_ref), Some(target_auth_ref)) =
        (source.auth_ref.as_deref(), auth_ref.as_deref())
    {
        let secret = credentials::load_keychain_secret(source_auth_ref)?;
        let trimmed = secret.trim();
        if trimmed.is_empty() {
            return Err("Stored Provider API key is empty.".to_string());
        }
        credentials::store_keychain_secret(target_auth_ref, trimmed)?;
    }

    let duplicated = ProfileDraft {
        id: new_id,
        name: source.name.clone(),
        icon: source.icon.clone(),
        remark: source.remark.clone(),
        app,
        is_builtin: false,
        mode: source.mode,
        provider: source.provider.clone(),
        protocol: source.protocol.clone(),
        model: source.model.clone(),
        review_model: source.review_model.clone(),
        model_mappings: source.model_mappings.clone(),
        base_url: source.base_url.clone(),
        auth_ref,
        created_at: Some(now.clone()),
        updated_at: Some(now.clone()),
        last_test_status: source.last_test_status.clone(),
        usage_enabled: false,
        sort_order,
    };
    clone_codex_oauth_profile_if_needed(&source, &duplicated)?;
    storage::save_profile(&duplicated)?;
    activity_log::append(
        Severity::Ok,
        format!(
            "Duplicated profile draft '{}' for {}/{}.",
            duplicated.name, duplicated.app, duplicated.provider
        ),
    )?;

    Ok(duplicated)
}

pub fn delete_profile_draft(request: DeleteProfileDraftRequest) -> Result<ProfileSummary, String> {
    ensure_app_dirs()?;

    let profile_id = normalize_token("Profile ID", &request.profile_id)?;
    if is_builtin_profile_id(&profile_id) {
        return Err("Built-in official profiles cannot be deleted.".to_string());
    }
    let source = load_profile_by_id(&profile_id)?;
    if source.is_builtin {
        return Err("Built-in official profiles cannot be deleted.".to_string());
    }

    if !storage::delete_profile(&profile_id)? {
        return Err(format!("Profile '{profile_id}' does not exist"));
    }
    delete_codex_oauth_profile_cache_if_needed(&source)?;

    let mut config = read_app_config()?;
    let mut changed =
        replace_deleted_active_profile_with_official(&mut config, &source.app, &profile_id);
    let drafts = load_profiles()?;
    changed |= clean_active_profiles(&mut config, &drafts);
    if changed {
        write_app_config(&config)?;
    }

    activity_log::append(
        Severity::Ok,
        format!(
            "Deleted profile draft '{}' for {}/{}.",
            source.name, source.app, source.provider
        ),
    )?;

    load_profile_summary_without_native_sync()
}

pub fn reorder_profile_drafts(
    request: ReorderProfileDraftsRequest,
) -> Result<ProfileSummary, String> {
    ensure_app_dirs()?;

    let app = canonical_profile_app(&normalize_token("Tool", &request.app)?);
    let mode = request.mode;
    let profiles = load_profiles()?;
    let expected_ids = profiles
        .iter()
        .filter(|profile| canonical_profile_app(&profile.app) == app && profile.mode == mode)
        .map(|profile| profile.id.clone())
        .collect::<HashSet<_>>();
    let requested_ids = request
        .profile_ids
        .iter()
        .map(|id| normalize_token("Profile ID", id))
        .collect::<Result<Vec<_>, _>>()?;
    let requested_set = requested_ids.iter().cloned().collect::<HashSet<_>>();
    if requested_set != expected_ids {
        return Err("Profile order must include every profile in this tool category.".to_string());
    }

    storage::reorder_profiles(&app, &mode, &requested_ids)?;
    activity_log::append(
        Severity::Info,
        format!(
            "Reordered {} profile draft(s) for {app}/{}.",
            requested_ids.len(),
            provider_apply_mode_value(&mode)
        ),
    )?;

    load_profile_summary_without_native_sync()
}

pub fn preview_profile_write(
    request: PreviewProfileWriteRequest,
) -> Result<PreviewProfileWriteResult, String> {
    let plan = build_profile_write_plan(
        &request.name,
        &request.app,
        request.mode.as_ref(),
        &request.provider,
        request.protocol.as_deref(),
        &request.model,
        &request.base_url,
        request.secret_provided,
    )?;
    let paths = app_paths().map_err(|err| err.to_string())?;
    let base_id = slugify(&plan.name);
    let tool = tool_catalog::ai_tools()
        .into_iter()
        .find(|tool| tool.id == plan.app);
    let target_tool_path = tool
        .as_ref()
        .and_then(|definition| definition.config_relative_path)
        .map(|relative| display_path(&paths.home_dir.join(relative)));
    let target_tool_name = tool
        .as_ref()
        .map(|definition| definition.name)
        .unwrap_or("Target tool");
    let mut warnings = Vec::new();

    if plan.id != base_id && !base_id.is_empty() {
        warnings.push(format!(
            "Profile id '{base_id}' already exists, so this draft will use '{}'.",
            plan.id
        ));
    }
    if tool.is_none() {
        warnings.push(format!("Tool '{}' is not in the local registry.", plan.app));
    }
    let now = Utc::now().to_rfc3339();
    let database_path = display_path(&paths.database_file);
    let preview_profile = ProfileDraft {
        id: plan.id.clone(),
        name: plan.name.clone(),
        icon: normalize_profile_icon(request.icon.as_deref())?,
        remark: normalize_profile_remark(request.remark.as_deref()),
        app: plan.app.clone(),
        is_builtin: false,
        mode: plan.mode,
        provider: plan.provider.clone(),
        protocol: plan.protocol.clone(),
        model: plan.model.clone(),
        review_model: normalize_profile_review_model(&plan.app, request.review_model.as_deref()),
        model_mappings: normalize_profile_model_mappings(
            &plan.app,
            request.model_mappings.as_deref(),
        )?,
        base_url: plan.base_url.clone(),
        auth_ref: plan.auth_ref.clone(),
        created_at: Some(now.clone()),
        updated_at: Some(now.clone()),
        last_test_status: Some("pending".to_string()),
        usage_enabled: false,
        sort_order: 0,
    };
    let profile_content =
        profile_sql_preview_content(&preview_profile, plan.secret_status, "pending")?;
    let auto_activate_gateway = if preview_profile.mode == ProviderApplyMode::Gateway {
        let config = read_app_config()?;
        let drafts = load_profiles()?;
        gateway_profile_will_auto_activate(&config, &preview_profile, &drafts)
    } else {
        false
    };
    let mut items = vec![
        ProfileWritePreviewItem {
            label: "Profile row".to_string(),
            path: Some(database_path.clone()),
            action: "create".to_string(),
            backup_required: false,
            detail: format!(
                "Save Profile Draft stores normalized metadata in SQLite for {}/{} and excludes API keys.",
                plan.protocol, plan.provider
            ),
            content: Some(profile_content),
        },
        ProfileWritePreviewItem {
            label: "Active tool profile pointer".to_string(),
            path: Some(database_path.clone()),
            action: if auto_activate_gateway {
                "update".to_string()
            } else {
                "not_modified".to_string()
            },
            backup_required: false,
            detail: if auto_activate_gateway {
                format!(
                    "Saving the first Gateway profile for '{}' makes it the active Gateway profile.",
                    preview_profile.app
                )
            } else {
                "Saving this draft preserves the current active profile.".to_string()
            },
            content: None,
        },
    ];

    items.push(ProfileWritePreviewItem {
        label: format!("{target_tool_name} config"),
        path: target_tool_path.clone(),
        action: "future_confirmation_required".to_string(),
        backup_required: target_tool_path.is_some(),
        detail: "Client config is not modified when saving a Provider Profile. Client Bootstrap remains a separate confirmation flow."
            .to_string(),
        content: None,
    });

    items.push(ProfileWritePreviewItem {
        label: "Credential".to_string(),
        path: None,
        action: plan.secret_status.to_string(),
        backup_required: false,
        detail: credential_detail(&plan.provider, request.secret_provided),
        content: None,
    });

    Ok(PreviewProfileWriteResult {
        generated_at: now,
        profile_id: plan.id,
        profile_path: database_path,
        target_tool_path,
        backup_required: false,
        items,
        warnings,
    })
}

pub fn preview_profile_apply(
    request: PreviewProfileApplyRequest,
) -> Result<PreviewProfileApplyResult, String> {
    ensure_app_dirs()?;

    let profile_id = normalize_token("Profile ID", &request.profile_id)?;
    let profile = load_profile_by_id(&profile_id)?;
    let paths = app_paths().map_err(|err| err.to_string())?;
    let is_codex_tool = is_codex_family_app(&profile.app);
    let tool = tool_catalog::ai_tools()
        .into_iter()
        .find(|tool| tool.id == profile.app);
    let native_config_path = native_config_path_for_profile_mode(&profile, &paths, profile.mode)?
        .map(|path| display_path(&path))
        .or_else(|| {
            tool.as_ref()
                .and_then(|definition| definition.config_relative_path)
                .map(|relative| display_path(&paths.home_dir.join(relative)))
        });
    let tool_name = tool
        .as_ref()
        .map(|definition| definition.name)
        .or_else(|| is_codex_tool.then_some("ChatGPT Desktop"))
        .unwrap_or("Target tool");
    let config_native_diff = build_native_config_preview(
        &profile,
        native_config_path.as_deref(),
        &paths,
        ProviderApplyMode::Config,
    )?;
    let gateway_native_diff = build_native_config_preview(
        &profile,
        native_config_path.as_deref(),
        &paths,
        ProviderApplyMode::Gateway,
    )?;
    let config_native_diff = attach_native_config_content_preview(
        config_native_diff,
        &profile,
        &paths,
        ProviderApplyMode::Config,
    );
    let gateway_native_diff = attach_native_config_content_preview(
        gateway_native_diff,
        &profile,
        &paths,
        ProviderApplyMode::Gateway,
    );
    let native_diff = match profile.mode {
        ProviderApplyMode::Config => config_native_diff.clone(),
        ProviderApplyMode::Gateway => gateway_native_diff.clone(),
    };
    let native_write_enabled = native_diff
        .as_ref()
        .map(|diff| diff.write_enabled)
        .unwrap_or(false);
    let mode_previews =
        build_provider_mode_previews(&profile, &config_native_diff, &gateway_native_diff);
    let mut warnings = Vec::new();

    if tool.is_none() && !is_codex_tool {
        warnings.push(format!(
            "Tool '{}' is not in the local registry, so this profile cannot be applied yet.",
            profile.app
        ));
    }
    let env_conflicts = env_health::claude_env_conflicts_for_profile(&profile);

    Ok(PreviewProfileApplyResult {
        generated_at: Utc::now().to_rfc3339(),
        profile_id: profile.id.clone(),
        profile_name: profile.name.clone(),
        app: profile.app.clone(),
        provider: profile.provider.clone(),
        can_apply: tool.is_some() || is_codex_tool,
        items: vec![
            ProfileApplyPreviewItem {
                label: "Active tool profile pointer".to_string(),
                path: Some(display_path(&paths.database_file)),
                action: "update".to_string(),
                backup_required: false,
                detail: format!(
                    "Sets the SQLite active profile pointer for '{}' to '{}' before refreshing detection.",
                    profile.app, profile.id
                ),
            },
            ProfileApplyPreviewItem {
                label: format!("{tool_name} native config"),
                path: native_config_path,
                action: if native_write_enabled {
                    "create_or_update".to_string()
                } else {
                    "not_modified".to_string()
                },
                backup_required: native_write_enabled,
                detail: if native_write_enabled {
                    "Selected mode writes this client config; detailed file changes are shown below."
                        .to_string()
                } else {
                    "This profile does not require a native client config write."
                        .to_string()
                },
            },
            ProfileApplyPreviewItem {
                label: "Credential".to_string(),
                path: None,
                action: "not_written".to_string(),
                backup_required: false,
                detail: "Apply writes no API keys or tokens. Existing official login/keychain state remains untouched."
                    .to_string(),
            },
        ],
        native_diff,
        mode_previews,
        warnings,
        env_conflicts,
    })
}

fn build_provider_mode_previews(
    profile: &ProfileDraft,
    config_native_diff: &Option<NativeConfigPreview>,
    gateway_native_diff: &Option<NativeConfigPreview>,
) -> Vec<ProviderApplyModePreview> {
    let is_codex_tool = is_codex_family_app(&profile.app);
    let is_official = provider_is_official(&profile.provider);
    let official_client_config = is_official && !is_codex_tool;
    let config_protocol_supported = config_file_protocol_supported(profile);
    let config_supported = config_native_diff.is_some() || official_client_config;
    let config_writes_native_config = native_preview_writes(config_native_diff);
    let gateway_writes_native_config = native_preview_writes(gateway_native_diff);
    let gateway_supported = !is_official;
    let config_blocked_reason = if !config_protocol_supported && !is_official {
        Some(format!(
            "Config profiles do not support {} for '{}'.",
            protocol_display_name(&profile.protocol),
            profile.app
        ))
    } else if !config_supported && !is_official {
        Some(format!(
            "Config profile adapter is not implemented for '{}'.",
            profile.app
        ))
    } else if profile.auth_ref.is_none() && provider_requires_api_key(&profile.provider) {
        Some("Config profiles need a stored Provider API key for this Provider.".to_string())
    } else {
        None
    };

    vec![
        ProviderApplyModePreview {
            mode: ProviderApplyMode::Config,
            label: "Client config profile".to_string(),
            description: "Back up and modify the target client's native provider config directly. This makes the client talk to the selected upstream Provider without CodeStudio Lite in the request path."
                .to_string(),
            supported: config_supported && config_blocked_reason.is_none(),
            recommended: is_official && config_supported && config_blocked_reason.is_none(),
            writes_native_config: config_writes_native_config,
            starts_gateway: false,
            blocked_reason: config_blocked_reason,
            native_diff: config_native_diff.clone(),
            warnings: if official_client_config {
                vec![
                    "Official provider uses the target client's own login.".to_string(),
                    "No Provider API key or model override is required.".to_string(),
                ]
            } else if config_supported {
                vec![
                    "Config profiles write Provider connection details into the client config.".to_string(),
                    "Frequent Provider switching may require the client to reload its own config.".to_string(),
                ]
            } else {
                Vec::new()
            },
        },
        ProviderApplyModePreview {
            mode: ProviderApplyMode::Gateway,
            label: "Gateway profile".to_string(),
            description: if gateway_writes_native_config {
                "Back up and point the client at the local CodeStudio Gateway once. This apply only switches the active Provider profile; start the Gateway from the sidebar when needed."
            } else {
                "Switch the active Provider profile for the local Gateway. This apply does not start the Gateway or modify this tool's native config."
            }
                .to_string(),
            supported: gateway_supported,
            recommended: gateway_supported && !is_official,
            writes_native_config: gateway_writes_native_config,
            starts_gateway: false,
            blocked_reason: if is_official {
                Some("Official provider uses the client login directly and does not run through the local gateway.".to_string())
            } else {
                None
            },
            native_diff: gateway_native_diff.clone(),
            warnings: if gateway_supported {
                let mut warnings = vec![
                    "Real upstream Provider API keys stay in the system keychain and are used by the local gateway.".to_string(),
                    "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running.".to_string(),
                ];
                if gateway_writes_native_config {
                    warnings.push(
                        "The client still needs to reload config after the first gateway bootstrap."
                            .to_string(),
                    );
                } else {
                    warnings.push(format!(
                        "No native gateway bootstrap is written for '{}'; configure the client to use the Gateway URL manually or wait for a validated adapter.",
                        profile.app
                    ));
                }
                warnings
            } else {
                Vec::new()
            },
        },
    ]
}

pub(in crate::core::profile) fn attach_native_config_content_preview(
    preview: Option<NativeConfigPreview>,
    profile: &ProfileDraft,
    paths: &crate::core::app_paths::AppPaths,
    mode: ProviderApplyMode,
) -> Option<NativeConfigPreview> {
    let mut preview = preview?;
    normalize_native_config_preview(&mut preview);
    if !preview.write_enabled {
        preview.content = None;
        return Some(preview);
    }
    if let Ok(Some(content)) =
        build_native_config_content_preview(profile, paths, mode, &preview.path)
    {
        preview.content = Some(redact_native_config_preview_content(
            &content, profile, mode,
        ));
    }
    Some(preview)
}

fn build_native_config_content_preview(
    profile: &ProfileDraft,
    paths: &crate::core::app_paths::AppPaths,
    mode: ProviderApplyMode,
    preview_path: &str,
) -> Result<Option<String>, String> {
    if provider_is_official(&profile.provider) && mode == ProviderApplyMode::Gateway {
        return Ok(None);
    }

    if canonical_profile_app(&profile.app) == "claude-desktop" {
        let desktop_paths = claude_desktop_paths(paths)?;
        if display_path(&desktop_paths.profile_path) != preview_path {
            return Ok(None);
        }
        if mode == ProviderApplyMode::Config && provider_is_official(&profile.provider) {
            return Ok(None);
        }
        let content = match mode {
            ProviderApplyMode::Config => claude_desktop_direct_profile_content_with_api_key(
                profile,
                secret_preview(profile),
            )?,
            ProviderApplyMode::Gateway => claude_desktop_gateway_profile_content(profile)?,
        };
        return Ok(Some(content));
    }

    let Some(path) = native_config_path_for_profile_mode(profile, paths, mode)? else {
        return Ok(None);
    };
    if display_path(&path) != preview_path {
        return Ok(None);
    }

    let current = read_file_if_exists(&path)?;
    let render = |current: &str| native_config_content_for_preview(current, profile, mode);
    match render(&current) {
        Ok(content) => Ok(Some(content)),
        Err(err) if preview_content_parse_error(&err) => render("").map(Some),
        Err(err) => Err(err),
    }
}

fn native_config_content_for_preview(
    current: &str,
    profile: &ProfileDraft,
    mode: ProviderApplyMode,
) -> Result<String, String> {
    let app = canonical_profile_app(&profile.app);
    if let Some(adapter) = native::adapter(&app) {
        return adapter.render_preview(current, profile, mode);
    }
    match mode {
        ProviderApplyMode::Config => match app.as_str() {
            "codex" => {
                if provider_is_official(&profile.provider) {
                    native::codex::codex_official_config_content(current, profile)
                } else {
                    native::codex::codex_direct_config_content(current, profile)
                }
            }
            _ => Err(format!(
                "Config profile adapter is not implemented for tool '{}'.",
                profile.app
            )),
        },
        ProviderApplyMode::Gateway => match app.as_str() {
            "codex" => native::codex::codex_gateway_config_content(current, profile),
            _ => Err(format!(
                "Gateway profile adapter is not implemented for tool '{}'.",
                profile.app
            )),
        },
    }
}

fn preview_content_parse_error(err: &str) -> bool {
    err.starts_with("Existing ") && err.contains(" could not be parsed")
}

fn native_preview_writes(preview: &Option<NativeConfigPreview>) -> bool {
    preview
        .as_ref()
        .map(|preview| preview.write_enabled)
        .unwrap_or(false)
}

fn normalize_native_config_preview(preview: &mut NativeConfigPreview) {
    preview.changes.retain(native_config_change_writes);
    if preview.changes.is_empty() {
        preview.write_enabled = false;
    }
}

fn native_config_change_writes(change: &NativeConfigDiffLine) -> bool {
    change.action != "unchanged" && change.before != change.after
}

pub fn apply_profile(request: ApplyProfileRequest) -> Result<ApplyProfileResult, String> {
    ensure_app_dirs()?;

    let profile_id = normalize_token("Profile ID", &request.profile_id)?;
    let profiles = load_profiles()?;
    let profile = profiles
        .iter()
        .find(|profile| profile.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("Profile '{profile_id}' does not exist"))?;
    let is_codex_tool = is_codex_family_app(&profile.app);
    let is_registered_tool = tool_catalog::ai_tools()
        .into_iter()
        .any(|tool| tool.id == profile.app);
    if !is_registered_tool && !is_codex_tool {
        return Err(format!(
            "Tool '{}' is not in the local registry, so this profile cannot be applied yet.",
            profile.app
        ));
    }
    let paths = app_paths().map_err(|err| err.to_string())?;
    let mode = profile.mode;
    if request.restart_after_apply && mode != ProviderApplyMode::Config {
        return Err("Apply and restart is only available for Config profiles.".to_string());
    }
    let native_plans = filter_native_write_plans(build_native_apply_plan(
        &profile,
        &paths,
        &mode,
        request.sync_claude_vs_code,
    )?)?;
    if request.restart_after_apply && native_plans.is_empty() {
        return Err(
            "Apply and restart requires a native client config write for this profile.".to_string(),
        );
    }
    let mut config = read_app_config()?;
    if clean_active_profiles(&mut config, &profiles) {
        write_app_config(&config)?;
    }
    if profile_is_active(&config, &profile) {
        return Err("Profile is already active for this tool and mode.".to_string());
    }
    let mut backup_targets = Vec::new();
    for plan in &native_plans {
        backup_targets.push(plan.path.clone());
    }
    let backup = backup::backup_files("apply-profile", Some(&profile.id), &backup_targets)?;

    // Write native tool configs before flipping the active pointer so a concurrent
    // summary load cannot observe "new active + old on-disk config" and import a
    // ghost draft from the previous disk state.
    let native_verified = if native_plans.is_empty() {
        false
    } else {
        for plan in &native_plans {
            apply_native_config_write_plan(plan)?;
        }
        native_plans
            .iter()
            .map(|plan| verify_native_config_write(plan, &profile, &mode))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .all(|verified| verified)
    };
    activate_profile_for_tool(&mut config, &profile, &profiles);
    write_app_config(&config)?;
    let verified = verify_active_profile(&config, &profile);
    if !verified {
        return Err("Applied profile database record did not pass verification".to_string());
    }
    let restart_outcome = if request.restart_after_apply {
        restart_tool_for_profile(
            &profile,
            RestartContext {
                sync_claude_vs_code: request.sync_claude_vs_code,
            },
        )?
    } else {
        RestartOutcome {
            performed: false,
            message: None,
        }
    };

    activity_log::append(
        Severity::Ok,
        if mode == ProviderApplyMode::Gateway {
            format!(
                "Applied profile '{}' for {}/{} in Gateway profile.",
                profile.name, profile.app, profile.provider
            )
        } else if native_verified && mode == ProviderApplyMode::Config {
            format!(
                "Applied profile '{}' for {}/{} through direct client config profile.",
                profile.name, profile.app, profile.provider
            )
        } else {
            format!(
                "Applied profile '{}' for {}/{}.",
                profile.name, profile.app, profile.provider
            )
        },
    )?;

    let env_conflicts = env_health::claude_env_conflicts_for_profile(&profile);

    Ok(ApplyProfileResult {
        // Disk and active pointer already match; skip native import here so a
        // follow-up UI refresh cannot race another sync against this apply.
        summary: load_profile_summary_without_native_sync()?,
        mode,
        backup,
        applied_path: display_path(&paths.database_file),
        verified,
        native_path: native_plans.first().map(|plan| display_path(&plan.path)),
        native_verified,
        restart_requested: request.restart_after_apply,
        restart_performed: restart_outcome.performed,
        restart_message: restart_outcome.message,
        gateway_status: None,
        env_conflicts,
    })
}
