use super::*;

pub(in crate::core::profile) fn load_profiles() -> Result<Vec<ProfileDraft>, String> {
    let mut profiles = builtin_official_profiles();
    let usage_enabled_profile_ids = storage::load_usage_enabled_profile_ids()?;
    for mut profile in storage::load_profiles()? {
        let app = canonical_profile_app(&profile.app);
        if tool_catalog::profile_capabilities(&app).is_none() {
            continue;
        }
        if is_builtin_profile_id(&profile.id) {
            continue;
        }
        let mode = normalize_stored_profile_mode(
            &profile.provider,
            Some(provider_apply_mode_value(&profile.mode).to_string()),
        );
        if ensure_custom_official_profile_allowed(&app, &profile.provider, mode).is_err() {
            continue;
        }
        profile.app = app;
        profile.is_builtin = false;
        profile.mode = mode;
        profile.protocol = normalize_protocol(Some(profile.protocol.as_str()))?;
        profile.usage_enabled = usage_enabled_profile_ids.contains(&profile.id);
        profiles.push(profile);
    }
    apply_stored_profile_order(&mut profiles)?;
    profiles.sort_by(compare_profiles);
    Ok(profiles)
}

fn apply_stored_profile_order(profiles: &mut [ProfileDraft]) -> Result<(), String> {
    let groups = profiles
        .iter()
        .map(|profile| (canonical_profile_app(&profile.app), profile.mode))
        .collect::<HashSet<_>>();
    for (app, mode) in groups {
        let order = storage::load_profile_order(&app, &mode)?;
        if order.is_empty() {
            continue;
        }
        let order_by_id = order
            .iter()
            .enumerate()
            .map(|(index, id)| (id.as_str(), index as i64))
            .collect::<HashMap<_, _>>();
        let mut next_unordered_index = order.len() as i64;
        let mut indexes = profiles
            .iter()
            .enumerate()
            .filter(|(_, profile)| {
                canonical_profile_app(&profile.app) == app && profile.mode == mode
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        indexes.sort_by(|left, right| compare_profiles(&profiles[*left], &profiles[*right]));
        for (index, profile_id) in order.iter().enumerate() {
            if let Some(profile) = profiles.iter_mut().find(|profile| {
                profile.id == *profile_id
                    && canonical_profile_app(&profile.app) == app
                    && profile.mode == mode
            }) {
                profile.sort_order = index as i64;
            }
        }
        for profile_index in indexes {
            let profile = &mut profiles[profile_index];
            if order_by_id.contains_key(profile.id.as_str()) {
                continue;
            }
            profile.sort_order = next_unordered_index;
            next_unordered_index += 1;
        }
    }
    Ok(())
}

pub(in crate::core::profile) fn builtin_official_profiles() -> Vec<ProfileDraft> {
    BUILTIN_OFFICIAL_PROFILES
        .iter()
        .map(|(app, name, protocol)| ProfileDraft {
            id: builtin_official_profile_id(app),
            name: (*name).to_string(),
            icon: Some(default_builtin_profile_icon(app).to_string()),
            remark: None,
            app: (*app).to_string(),
            is_builtin: true,
            mode: ProviderApplyMode::Config,
            provider: "official".to_string(),
            protocol: (*protocol).to_string(),
            model: String::new(),
            review_model: None,
            model_mappings: Vec::new(),
            base_url: String::new(),
            auth_ref: None,
            created_at: None,
            updated_at: None,
            last_test_status: Some("builtin".to_string()),
            usage_enabled: false,
            sort_order: 0,
        })
        .collect()
}

pub(in crate::core::profile) fn builtin_official_profile_id(app: &str) -> String {
    format!("{BUILTIN_OFFICIAL_ID_PREFIX}{}", canonical_profile_app(app))
}

fn default_builtin_profile_icon(app: &str) -> &'static str {
    match canonical_profile_app(app).as_str() {
        "codex" => "C",
        "claude-desktop" => "CD",
        "claude" => "CC",
        "gemini-code-assist" => "GA",
        "opencode" => "OC",
        "openclaw" => "O",
        "hermes" => "H",
        "grok" => "G",
        "pi" => "π",
        _ => "?",
    }
}

pub(in crate::core::profile) fn is_builtin_profile_id(id: &str) -> bool {
    id.starts_with(BUILTIN_OFFICIAL_ID_PREFIX)
}

pub(in crate::core::profile) fn compare_profiles(
    left: &ProfileDraft,
    right: &ProfileDraft,
) -> std::cmp::Ordering {
    left.app
        .cmp(&right.app)
        .then_with(|| {
            provider_apply_mode_value(&left.mode).cmp(provider_apply_mode_value(&right.mode))
        })
        .then_with(|| left.sort_order.cmp(&right.sort_order))
        .then_with(|| left.name.cmp(&right.name))
}

pub(in crate::core::profile) fn load_profile_by_id(
    profile_id: &str,
) -> Result<ProfileDraft, String> {
    load_profiles()?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("Profile '{profile_id}' does not exist"))
}

pub(in crate::core::profile) fn clean_active_profiles(
    config: &mut AppConfig,
    drafts: &[ProfileDraft],
) -> bool {
    clean_active_profile_map(
        &mut config.active_profiles_by_mode.config,
        ProviderApplyMode::Config,
        drafts,
    ) | clean_active_profile_map(
        &mut config.active_profiles_by_mode.gateway,
        ProviderApplyMode::Gateway,
        drafts,
    )
}

pub(in crate::core::profile) fn replace_deleted_active_profile_with_official(
    config: &mut AppConfig,
    app: &str,
    profile_id: &str,
) -> bool {
    let canonical_app = canonical_profile_app(app);
    let mut changed = false;
    let config_active = &mut config.active_profiles_by_mode.config;
    for key in config_active.keys().cloned().collect::<Vec<_>>() {
        if config_active.get(&key).map(String::as_str) == Some(profile_id) {
            config_active.remove(&key);
            config_active.insert(
                canonical_profile_app(&key),
                builtin_official_profile_id(&canonical_app),
            );
            changed = true;
        }
    }
    let gateway_active = &mut config.active_profiles_by_mode.gateway;
    let before = gateway_active.len();
    gateway_active.retain(|_, active_profile_id| active_profile_id != profile_id);
    changed | (gateway_active.len() != before)
}

fn clean_active_profile_map(
    active_profiles: &mut HashMap<String, String>,
    mode: ProviderApplyMode,
    drafts: &[ProfileDraft],
) -> bool {
    let mut changed = false;
    for (app, profile_id) in active_profiles.clone() {
        let canonical_app = canonical_profile_app(&app);
        let valid = drafts.iter().any(|profile| {
            profile.id == profile_id && profile.app == canonical_app && profile.mode == mode
        });
        if !valid {
            active_profiles.remove(&app);
            changed = true;
        } else if app != canonical_app {
            active_profiles.remove(&app);
            active_profiles.entry(canonical_app).or_insert(profile_id);
            changed = true;
        }
    }
    changed
}

pub(in crate::core::profile) fn activate_profile_for_tool(
    config: &mut AppConfig,
    profile: &ProfileDraft,
    drafts: &[ProfileDraft],
) {
    active_profiles_for_mode_mut(&mut config.active_profiles_by_mode, &profile.mode)
        .insert(profile.app.clone(), profile.id.clone());
    clean_active_profiles(config, drafts);
}

pub(in crate::core::profile) fn activate_new_gateway_profile_if_unset(
    config: &mut AppConfig,
    draft: &ProfileDraft,
    drafts: &[ProfileDraft],
) -> bool {
    if !gateway_profile_will_auto_activate(config, draft, drafts) {
        return false;
    }
    activate_profile_for_tool(config, draft, drafts);
    true
}

pub(in crate::core::profile) fn gateway_profile_will_auto_activate(
    config: &AppConfig,
    draft: &ProfileDraft,
    drafts: &[ProfileDraft],
) -> bool {
    if draft.mode != ProviderApplyMode::Gateway {
        return false;
    }
    let canonical_app = canonical_profile_app(&draft.app);
    !config
        .active_profiles_by_mode
        .gateway
        .iter()
        .any(|(app, profile_id)| {
            canonical_profile_app(app) == canonical_app
                && drafts.iter().any(|profile| {
                    profile.id == profile_id.as_str()
                        && canonical_profile_app(&profile.app) == canonical_app
                        && profile.mode == ProviderApplyMode::Gateway
                })
        })
}

pub(in crate::core::profile) fn active_profiles_for_mode_mut<'a>(
    active_profiles: &'a mut ActiveProfilesByMode,
    mode: &ProviderApplyMode,
) -> &'a mut HashMap<String, String> {
    match mode {
        ProviderApplyMode::Config => &mut active_profiles.config,
        ProviderApplyMode::Gateway => &mut active_profiles.gateway,
    }
}

pub(in crate::core::profile) fn default_active_profile_id(
    active_profiles: &HashMap<String, String>,
    drafts: &[ProfileDraft],
) -> Option<String> {
    const PREFERRED_APPS: [&str; 9] = [
        "codex",
        "claude-desktop",
        "claude",
        "gemini-code-assist",
        "opencode",
        "openclaw",
        "hermes",
        "grok",
        "pi",
    ];
    for app in PREFERRED_APPS {
        if let Some(profile_id) = active_profiles.get(app) {
            if drafts
                .iter()
                .any(|profile| profile.id == *profile_id && profile.app == app)
            {
                return Some(profile_id.clone());
            }
        }
    }
    let mut apps = active_profiles.keys().collect::<Vec<_>>();
    apps.sort();
    apps.into_iter().find_map(|app| {
        let profile_id = active_profiles.get(app)?;
        drafts
            .iter()
            .any(|profile| profile.id == *profile_id && profile.app == *app)
            .then(|| profile_id.clone())
    })
}
