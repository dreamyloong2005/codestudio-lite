use super::*;

fn test_app_config() -> AppConfig {
    AppConfig {
        active_profiles_by_mode: ActiveProfilesByMode::default(),
        ui: UiConfig {
            theme: "system".to_string(),
            language: "zh-CN".to_string(),
            language_set_by_user: false,
        },
        security: SecurityConfig {
            backup_before_write: true,
            redact_secrets: true,
            confirm_install_commands: true,
            confirm_config_writes: true,
        },
    }
}

#[test]
fn new_gateway_profile_becomes_active_when_tool_has_no_gateway_profile() {
    let mut config = test_app_config();
    let profile = test_profile("claude", ProviderApplyMode::Gateway);
    let drafts = vec![profile.clone()];

    assert!(gateway_profile_will_auto_activate(
        &config, &profile, &drafts
    ));
    assert!(activate_new_gateway_profile_if_unset(
        &mut config,
        &profile,
        &drafts
    ));
    assert_eq!(
        config.active_profiles_by_mode.gateway.get("claude"),
        Some(&profile.id)
    );
}

#[test]
fn new_gateway_profile_replaces_a_stale_gateway_pointer() {
    let mut config = test_app_config();
    config
        .active_profiles_by_mode
        .gateway
        .insert("claude".to_string(), "missing-profile".to_string());
    let profile = test_profile("claude", ProviderApplyMode::Gateway);
    let drafts = vec![profile.clone()];

    assert!(gateway_profile_will_auto_activate(
        &config, &profile, &drafts
    ));
    assert!(activate_new_gateway_profile_if_unset(
        &mut config,
        &profile,
        &drafts
    ));
    assert_eq!(
        config.active_profiles_by_mode.gateway.get("claude"),
        Some(&profile.id)
    );
}

#[test]
fn new_gateway_profile_preserves_the_existing_active_profile() {
    let mut config = test_app_config();
    let mut active = test_profile("claude", ProviderApplyMode::Gateway);
    active.id = "claude-active".to_string();
    let mut created = test_profile("claude", ProviderApplyMode::Gateway);
    created.id = "claude-created".to_string();
    config
        .active_profiles_by_mode
        .gateway
        .insert("claude".to_string(), active.id.clone());
    let drafts = vec![active.clone(), created.clone()];

    assert!(!gateway_profile_will_auto_activate(
        &config, &created, &drafts
    ));
    assert!(!activate_new_gateway_profile_if_unset(
        &mut config,
        &created,
        &drafts
    ));
    assert_eq!(
        config.active_profiles_by_mode.gateway.get("claude"),
        Some(&active.id)
    );
}

#[test]
fn new_direct_profile_does_not_change_gateway_activation() {
    let mut config = test_app_config();
    let profile = test_profile("claude", ProviderApplyMode::Config);
    let drafts = vec![profile.clone()];

    assert!(!gateway_profile_will_auto_activate(
        &config, &profile, &drafts
    ));
    assert!(!activate_new_gateway_profile_if_unset(
        &mut config,
        &profile,
        &drafts
    ));
    assert!(config.active_profiles_by_mode.gateway.is_empty());
    assert!(config.active_profiles_by_mode.config.is_empty());
}

fn assert_codex_managed_provider_contract(value: &toml::Value, provider_id: &str) {
    assert_eq!(
        toml_lookup(
            value,
            &format!("model_providers.{provider_id}.requires_openai_auth")
        )
        .and_then(|item| item.as_bool()),
        Some(false)
    );
    assert_eq!(
        toml_lookup(
            value,
            &format!(
                "model_providers.{provider_id}.http_headers.{CODEX_ACTOR_AUTHORIZATION_HEADER}"
            )
        )
        .and_then(|item| item.as_str()),
        Some(CODEX_ACTOR_AUTHORIZATION_VALUE)
    );
}

fn assert_codex_managed_provider_contract_lines(content: &str) {
    assert!(content.contains(
        "requires_openai_auth = false\nhttp_headers = { \"x-openai-actor-authorization\" = \"codestudio-lite\" }\n"
    ), "generated config did not preserve the expected adjacent contract lines:\n{content}");
}

#[test]
fn sync_codex_config_profile_marks_matching_official_profile_active() {
    let mut config = test_app_config();
    let drafts = builtin_official_profiles();
    let codex_config: toml::Value = toml::from_str(
        r#"
model_provider = "openai"

[model_providers.openai]
wire_api = "responses"
requires_openai_auth = false
http_headers = { "x-openai-actor-authorization" = "codestudio-lite" }
"#,
    )
    .expect("config should parse");

    assert!(sync_codex_config_profile(
        &mut config,
        &drafts,
        &codex_config
    ));
    assert_eq!(
        config.active_profiles_by_mode.config.get("codex"),
        Some(&builtin_official_profile_id("codex"))
    );
}

#[test]
fn sync_codex_config_profile_marks_empty_config_as_official() {
    let mut config = test_app_config();
    let drafts = builtin_official_profiles();
    let codex_config = parse_toml_or_empty("", "Codex config").expect("config should parse");

    assert!(sync_codex_config_profile(
        &mut config,
        &drafts,
        &codex_config
    ));
    assert_eq!(
        config.active_profiles_by_mode.config.get("codex"),
        Some(&builtin_official_profile_id("codex"))
    );
}

#[test]
fn sync_codex_config_profile_clears_stale_config_active_profile() {
    let mut config = test_app_config();
    config
        .active_profiles_by_mode
        .config
        .insert("codex".to_string(), builtin_official_profile_id("codex"));
    let drafts = builtin_official_profiles();
    let codex_config: toml::Value = toml::from_str(
        r#"
model_provider = "other"

[model_providers.other]
requires_openai_auth = true
"#,
    )
    .expect("config should parse");

    assert!(sync_codex_config_profile(
        &mut config,
        &drafts,
        &codex_config
    ));
    assert!(!config.active_profiles_by_mode.config.contains_key("codex"));
}

#[test]
fn sync_codex_config_profile_rejects_managed_openai_override() {
    let mut config = test_app_config();
    config
        .active_profiles_by_mode
        .config
        .insert("codex".to_string(), builtin_official_profile_id("codex"));
    let drafts = builtin_official_profiles();
    let codex_config: toml::Value = toml::from_str(
        r#"
model_provider = "openai"

[model_providers.openai]
wire_api = "responses"
requires_openai_auth = false
http_headers = { "x-openai-actor-authorization" = "codestudio-lite" }
base_url = "https://example.test/v1"
"#,
    )
    .expect("config should parse");

    assert!(sync_codex_config_profile(
        &mut config,
        &drafts,
        &codex_config
    ));
    assert!(!config.active_profiles_by_mode.config.contains_key("codex"));
}

#[test]
fn official_non_codex_configs_match_when_not_managed() {
    let drafts = builtin_official_profiles();

    assert!(sync_native_config_profile(
        &mut test_app_config(),
        &drafts,
        "claude",
        |profile| claude_config_matches_profile(&serde_json::json!({}), profile)
    ));
    assert!(sync_native_config_profile(
        &mut test_app_config(),
        &drafts,
        "gemini-code-assist",
        |profile| { gemini_code_assist_settings_match_profile(&serde_json::json!({}), profile) }
    ));
    assert!(sync_native_config_profile(
        &mut test_app_config(),
        &drafts,
        "opencode",
        |profile| opencode_config_matches_profile(&serde_json::json!({}), profile)
    ));
    assert!(sync_native_config_profile(
        &mut test_app_config(),
        &drafts,
        "openclaw",
        |profile| openclaw_config_matches_profile(&serde_json::json!({}), profile)
    ));
    assert!(sync_native_config_profile(
        &mut test_app_config(),
        &drafts,
        "hermes",
        |profile| {
            hermes_config_matches_profile(
                &serde_norway::Value::Mapping(Default::default()),
                profile,
            )
        }
    ));
}

#[test]
fn hermes_unmanaged_config_marks_builtin_official_profile_active() {
    let mut config = test_app_config();
    let drafts = builtin_official_profiles();
    let value =
        parse_hermes_yaml_or_empty("{}\nmcp_servers: {}\n").expect("Hermes config should parse");

    assert!(sync_native_config_profile(
        &mut config,
        &drafts,
        "hermes",
        |profile| hermes_config_matches_profile(&value, profile)
    ));
    assert_eq!(
        config.active_profiles_by_mode.config.get("hermes"),
        Some(&builtin_official_profile_id("hermes"))
    );
}

#[test]
fn official_non_codex_configs_do_not_match_managed_values() {
    let drafts = builtin_official_profiles();

    assert!(!sync_native_config_profile(
        &mut test_app_config(),
        &drafts,
        "claude",
        |profile| claude_config_matches_profile(
            &serde_json::json!({ "env": { "ANTHROPIC_BASE_URL": "https://example.test" } }),
            profile,
        )
    ));
    assert!(!sync_native_config_profile(
        &mut test_app_config(),
        &drafts,
        "opencode",
        |profile| opencode_config_matches_profile(
            &serde_json::json!({ "provider": { "custom": {} } }),
            profile,
        )
    ));
    assert!(!sync_native_config_profile(
        &mut test_app_config(),
        &drafts,
        "openclaw",
        |profile| openclaw_config_matches_profile(
            &serde_json::json!({ "models": { "providers": { "custom": {} } } }),
            profile,
        )
    ));
}

#[test]
fn detects_codex_custom_native_profile() {
    let value: toml::Value = toml::from_str(
        r#"
model_provider = "codestudio-openrouter"
model = "gpt-5.5"
review_model = "gpt-5.6-review"

[model_providers.codestudio-openrouter]
name = "CodeStudio OpenRouter"
base_url = "https://openrouter.ai/api/v1"
wire_api = "responses"
requires_openai_auth = true
"#,
    )
    .expect("config should parse");
    let auth = serde_json::json!({
        "OPENAI_API_KEY": "sk-router"
    });
    let detected = detect_codex_native_profile_with_auth(&value, Some(&auth))
        .expect("custom profile should import");

    assert_eq!(detected.app, "codex");
    assert_eq!(
        normalize_detected_provider(&detected.provider, &detected.base_url),
        "openrouter.ai"
    );
    assert_eq!(detected.protocol, PROTOCOL_OPENAI_RESPONSES);
    assert_eq!(detected.model, "gpt-5.5");
    assert_eq!(detected.review_model.as_deref(), Some("gpt-5.6-review"));
    assert_eq!(detected.base_url, "https://openrouter.ai/api/v1");
    assert_eq!(detected.api_key, "sk-router");
}

#[test]
fn provider_slug_preserves_second_level_domain() {
    assert_eq!(
        provider_slug_from_base_url("https://api.apikey.fun/v1").as_deref(),
        Some("apikey.fun")
    );
    assert_eq!(
        provider_slug_from_base_url("https://openrouter.ai/api/v1").as_deref(),
        Some("openrouter.ai")
    );
}

#[test]
fn detected_provider_preserves_dotted_display_tokens() {
    assert_eq!(
        normalize_detected_provider("APIKEY.FUN", "https://api.apikey.fun/v1"),
        "apikey.fun"
    );
    assert_eq!(
        normalize_detected_provider("CodeStudio OpenRouter", "https://openrouter.ai/api/v1"),
        "openrouter.ai"
    );
}

#[test]
fn profile_runtime_base_url_adds_v1_only_for_openai_upstream_protocols() {
    let cases = [
        (
            PROTOCOL_OPENAI_RESPONSES,
            "https://api.apikey.fun",
            "https://api.apikey.fun/v1",
        ),
        (
            PROTOCOL_OPENAI_CHAT_COMPLETIONS,
            "http://127.0.0.1:8000/",
            "http://127.0.0.1:8000/v1",
        ),
        (
            PROTOCOL_ANTHROPIC_MESSAGES,
            "https://api.anthropic.test",
            "https://api.anthropic.test/",
        ),
        (
            PROTOCOL_GOOGLE_GEMINI,
            "https://generativelanguage.googleapis.com/v1beta",
            "https://generativelanguage.googleapis.com/v1beta",
        ),
        (
            PROTOCOL_GOOGLE_GEMINI,
            "https://generativelanguage.googleapis.com",
            "https://generativelanguage.googleapis.com/",
        ),
    ];

    for (protocol, base_url, expected) in cases {
        assert_eq!(
            profile_runtime_base_url_for_protocol(protocol, base_url),
            expected
        );
    }
}

#[test]
fn profile_model_list_url_matches_protocol_base_rules() {
    let cases = [
        (
            PROTOCOL_OPENAI_RESPONSES,
            "https://api.apikey.fun",
            "https://api.apikey.fun/v1/models",
        ),
        (
            PROTOCOL_OPENAI_CHAT_COMPLETIONS,
            "https://open.bigmodel.cn/api/coding/paas/v4",
            "https://open.bigmodel.cn/api/coding/paas/v4/models",
        ),
        (
            PROTOCOL_ANTHROPIC_MESSAGES,
            "https://api.anthropic.test/v1",
            "https://api.anthropic.test/v1/models",
        ),
        (
            PROTOCOL_GOOGLE_GEMINI,
            "https://generativelanguage.googleapis.com/v1beta",
            "https://generativelanguage.googleapis.com/v1beta/models",
        ),
    ];

    for (protocol, base_url, expected) in cases {
        assert_eq!(profile_model_list_url(protocol, base_url), expected);
    }
}

#[test]
fn profile_model_options_parse_common_provider_shapes() {
    let openai = serde_json::json!({
        "data": [
            { "id": "gpt-5", "owned_by": "openai" },
            { "id": "gpt-5", "owned_by": "duplicate" },
            { "id": "gpt-5-mini", "display_name": "GPT-5 mini" }
        ]
    });
    assert_eq!(
        profile_model_options_from_payload(PROTOCOL_OPENAI_RESPONSES, &openai),
        vec![
            ProfileModelOption {
                id: "gpt-5".to_string(),
                name: None,
                owned_by: Some("openai".to_string()),
                supports_1m: false,
            },
            ProfileModelOption {
                id: "gpt-5-mini".to_string(),
                name: Some("GPT-5 mini".to_string()),
                owned_by: None,
                supports_1m: false,
            },
        ]
    );

    let anthropic = serde_json::json!({
        "data": [
            { "id": "claude-sonnet-4-6", "display_name": "Claude Sonnet 4.6" }
        ]
    });
    assert_eq!(
        profile_model_options_from_payload(PROTOCOL_ANTHROPIC_MESSAGES, &anthropic)[0]
            .name
            .as_deref(),
        Some("Claude Sonnet 4.6")
    );

    let gemini = serde_json::json!({
        "models": [
            {
                "name": "models/gemini-2.5-pro",
                "displayName": "Gemini 2.5 Pro",
                "supportedGenerationMethods": ["generateContent"]
            },
            {
                "name": "models/embedding-001",
                "displayName": "Embedding",
                "supportedGenerationMethods": ["embedContent"]
            }
        ]
    });
    assert_eq!(
        profile_model_options_from_payload(PROTOCOL_GOOGLE_GEMINI, &gemini),
        vec![ProfileModelOption {
            id: "gemini-2.5-pro".to_string(),
            name: Some("Gemini 2.5 Pro".to_string()),
            owned_by: None,
            supports_1m: false,
        }]
    );
}

#[test]
fn auto_detected_native_profile_name_allows_provider_correction() {
    assert!(is_auto_detected_native_profile_name(
        "Claude Code fun",
        "claude",
        "fun"
    ));
    assert!(is_auto_detected_native_profile_name(
        "Claude Code fun 1",
        "claude",
        "fun"
    ));
    assert!(!is_auto_detected_native_profile_name(
        "My Claude Code fun",
        "claude",
        "fun"
    ));
}

#[test]
fn grok_config_roundtrip_matches_custom_model_shape() {
    let mut profile = test_profile("grok", ProviderApplyMode::Config);
    profile.provider = "compatible".to_string();
    profile.protocol = PROTOCOL_OPENAI_RESPONSES.to_string();
    profile.model = "grok-4.5".to_string();
    profile.base_url = "https://api.apikey.fun/v1".to_string();
    profile.name = "Grok 4.5 (apikey.fun)".to_string();
    store_test_profile_secret(&profile, "sk-test-grok");

    let content = grok_config_content("", &profile).expect("grok config should render");
    assert!(
        content.contains("default = \"codestudio\""),
        "unexpected grok config:\n{content}"
    );
    assert!(
        content.contains("[model.codestudio]"),
        "unexpected grok config:\n{content}"
    );
    assert!(
        content.contains("api_backend = \"responses\""),
        "unexpected grok config:\n{content}"
    );
    assert!(
        content.contains("https://api.apikey.fun/v1"),
        "unexpected grok config:\n{content}"
    );
    assert!(
        content.contains("grok-4.5"),
        "unexpected grok config:\n{content}"
    );

    let value: toml::Value = toml::from_str(&content).expect("rendered config should parse");
    assert!(grok_config_matches_profile(&value, &profile));

    let detected = detect_grok_native_profile(&value).expect("managed grok config should detect");
    assert_eq!(detected.app, "grok");
    assert_eq!(detected.protocol, PROTOCOL_OPENAI_RESPONSES);
    assert_eq!(detected.model, "grok-4.5");
    assert_eq!(detected.base_url, "https://api.apikey.fun/v1");
    assert_eq!(detected.api_key, "sk-test-grok");
}

#[test]
fn pi_config_roundtrip_supports_all_native_api_families() {
    let cases = [
        (
            PROTOCOL_OPENAI_CHAT_COMPLETIONS,
            "openai-completions",
            "https://api.example.test",
            "https://api.example.test/v1",
        ),
        (
            PROTOCOL_OPENAI_RESPONSES,
            "openai-responses",
            "https://api.example.test/v1",
            "https://api.example.test/v1",
        ),
        (
            PROTOCOL_ANTHROPIC_MESSAGES,
            "anthropic-messages",
            "https://api.example.test",
            "https://api.example.test/",
        ),
        (
            PROTOCOL_GOOGLE_GEMINI,
            "google-generative-ai",
            "https://generativelanguage.googleapis.com/v1beta",
            "https://generativelanguage.googleapis.com/v1beta",
        ),
    ];

    for (protocol, expected_api, base_url, expected_base_url) in cases {
        let mut profile = test_profile("pi", ProviderApplyMode::Config);
        profile.provider = "compatible".to_string();
        profile.protocol = protocol.to_string();
        profile.model = "gpt-5.5".to_string();
        profile.base_url = base_url.to_string();
        profile.name = "Pi Custom".to_string();
        store_test_profile_secret(&profile, "sk-test-pi");

        let content = pi_config_content(
            r#"{
  "providers": {
    "keep": {
      "baseUrl": "https://keep.example/v1",
      "api": "openai-completions",
      "apiKey": "keep-key",
      "models": [{ "id": "keep-model" }]
    }
  },
  "other": true
}"#,
            &profile,
        )
        .expect("Pi config should render");
        let value = parse_json5_or_empty(&content, "Pi Agent models").expect("Pi JSON");

        assert_eq!(
            json_string_lookup(&value, &["providers", "codestudio", "baseUrl"]).as_deref(),
            Some(expected_base_url)
        );
        assert_eq!(
            json_string_lookup(&value, &["providers", "codestudio", "api"]).as_deref(),
            Some(expected_api)
        );
        assert_eq!(
            json_string_lookup(&value, &["providers", "codestudio", "apiKey"]).as_deref(),
            Some("sk-test-pi")
        );
        assert_eq!(
            value
                .pointer("/providers/codestudio/models/0/id")
                .and_then(serde_json::Value::as_str),
            Some("gpt-5.5")
        );
        assert_eq!(
            value
                .pointer("/providers/codestudio/models/0/name")
                .and_then(serde_json::Value::as_str),
            Some("Pi Custom")
        );
        assert!(value.pointer("/providers/keep").is_some());
        assert_eq!(
            value.get("other").and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert!(pi_config_matches_profile(&value, &profile));

        let detected = detect_pi_native_profile(&value).expect("managed Pi provider should detect");
        assert_eq!(detected.app, "pi");
        assert_eq!(detected.protocol, protocol);
        assert_eq!(detected.model, "gpt-5.5");
        assert_eq!(detected.base_url, expected_base_url);
        assert_eq!(detected.api_key, "sk-test-pi");
    }
}

#[test]
fn pi_native_detection_skips_unusable_providers_before_a_valid_entry() {
    let value = serde_json::json!({
        "providers": {
            "aaa-keyless": {
                "baseUrl": "https://keyless.example/v1",
                "api": "openai-completions",
                "models": [{ "id": "ignored" }]
            },
            "bbb-local-gateway": {
                "baseUrl": "http://127.0.0.1:43112/tools/pi/v1",
                "api": "openai-completions",
                "apiKey": "codestudio-local-test",
                "models": [{ "id": "ignored" }]
            },
            "zzz-valid": {
                "baseUrl": "https://api.valid.example/v1",
                "api": "openai-responses",
                "apiKey": "sk-valid",
                "models": [{ "id": "gpt-5.5" }]
            }
        }
    });

    let detected = detect_pi_native_profile(&value).expect("valid provider should be imported");
    assert_eq!(detected.provider, "zzz-valid");
    assert_eq!(detected.protocol, PROTOCOL_OPENAI_RESPONSES);
    assert_eq!(detected.model, "gpt-5.5");
    assert_eq!(detected.api_key, "sk-valid");
}

#[test]
fn pi_official_cleanup_preserves_unrelated_providers() {
    let content = pi_official_config_content(
        r#"{
  "providers": {
    "codestudio": { "baseUrl": "https://managed.example/v1" },
    "codestudio-old": { "baseUrl": "https://old.example/v1" },
    "keep": {
      "baseUrl": "https://keep.example/v1",
      "api": "openai-completions",
      "apiKey": "keep-key",
      "models": [{ "id": "keep-model" }]
    }
  },
  "other": true
}"#,
    )
    .expect("Pi official cleanup should render");
    let value = parse_json5_or_empty(&content, "Pi Agent models").expect("Pi JSON");

    assert!(value.pointer("/providers/codestudio").is_none());
    assert!(value.pointer("/providers/codestudio-old").is_none());
    assert!(value.pointer("/providers/keep").is_some());
    assert_eq!(
        value.get("other").and_then(serde_json::Value::as_bool),
        Some(true)
    );
}

#[test]
fn pi_gateway_config_verification_and_cleanup_use_the_tool_scoped_client() {
    let mut profile = test_profile("pi", ProviderApplyMode::Gateway);
    profile.model = "gpt-5.5".to_string();
    let client = gateway::client_config_for_tool("pi").expect("Pi gateway client");
    let content = pi_gateway_config_content(
        r#"{
  "providers": {
    "keep": {
      "baseUrl": "https://keep.example/v1",
      "api": "openai-completions",
      "apiKey": "keep-key",
      "models": [{ "id": "keep-model" }]
    }
  }
}"#,
        &profile,
    )
    .expect("Pi gateway config should render");
    let value = parse_json5_or_empty(&content, "Pi Agent models").expect("Pi JSON");

    assert_eq!(
        json_string_lookup(&value, &["providers", "codestudio", "baseUrl"]).as_deref(),
        Some(client.base_url.as_str())
    );
    assert_eq!(
        json_string_lookup(&value, &["providers", "codestudio", "api"]).as_deref(),
        Some("openai-completions")
    );
    assert_eq!(
        json_string_lookup(&value, &["providers", "codestudio", "apiKey"]).as_deref(),
        Some(client.token.as_str())
    );
    assert_eq!(
        value
            .pointer("/providers/codestudio/models/0/id")
            .and_then(serde_json::Value::as_str),
        Some("gpt-5.5")
    );
    assert!(value.pointer("/providers/keep").is_some());

    let paths = test_paths();
    let config_path = paths.home_dir.join(".pi").join("agent").join("models.json");
    write_native_config(&config_path, &content).expect("Pi config should write");
    assert!(verify_pi_gateway_config(&config_path, &profile).expect("Pi config should verify"));
    write_native_config(
        &config_path,
        &content.replace("openai-completions", "openai-responses"),
    )
    .expect("tampered Pi config should write");
    assert!(!verify_pi_gateway_config(&config_path, &profile)
        .expect("tampered Pi config should be readable"));

    let cleaned = pi_gateway_cleanup_config_content(&content, "pi")
        .expect("Pi gateway cleanup should render");
    let cleaned = parse_json5_or_empty(&cleaned, "Pi Agent models").expect("Pi JSON");
    assert!(cleaned.pointer("/providers/codestudio").is_none());
    assert!(cleaned.pointer("/providers/keep").is_some());
}

#[test]
fn pi_native_paths_and_previews_cover_direct_official_and_gateway_modes() {
    let paths = test_paths();
    let expected_path = paths.home_dir.join(".pi").join("agent").join("models.json");
    let mut direct = test_profile("pi", ProviderApplyMode::Config);
    direct.provider = "compatible".to_string();
    direct.protocol = PROTOCOL_OPENAI_RESPONSES.to_string();
    direct.model = "gpt-5.5".to_string();
    direct.base_url = "https://api.example.test/v1".to_string();

    assert_eq!(
        native_config_path_for_profile_mode(&direct, &paths, ProviderApplyMode::Config)
            .expect("Pi direct path"),
        Some(expected_path.clone())
    );
    assert_eq!(
        native_config_path_for_profile_mode(&direct, &paths, ProviderApplyMode::Gateway)
            .expect("Pi gateway path"),
        Some(expected_path.clone())
    );

    let direct_preview =
        build_native_config_preview(&direct, None, &paths, ProviderApplyMode::Config)
            .expect("Pi direct preview should build")
            .expect("Pi direct preview should exist");
    assert_eq!(direct_preview.tool, "pi");
    for key in [
        "providers.codestudio.baseUrl",
        "providers.codestudio.api",
        "providers.codestudio.apiKey",
        "providers.codestudio.models",
    ] {
        assert!(
            direct_preview
                .changes
                .iter()
                .any(|change| change.key == key),
            "Pi direct preview is missing {key}"
        );
    }

    let gateway = test_profile("pi", ProviderApplyMode::Gateway);
    let gateway_preview =
        build_native_config_preview(&gateway, None, &paths, ProviderApplyMode::Gateway)
            .expect("Pi gateway preview should build")
            .expect("Pi gateway preview should exist");
    assert!(gateway_preview
        .changes
        .iter()
        .any(|change| change.key == "providers.codestudio.api"
            && change.after.as_deref() == Some("openai-completions")));

    write_native_config(
        &expected_path,
        r#"{
  "providers": {
    "codestudio": {
      "baseUrl": "https://managed.example/v1",
      "api": "openai-responses",
      "apiKey": "sk-managed",
      "models": [{ "id": "managed" }]
    },
    "keep": { "baseUrl": "https://keep.example/v1" }
  }
}"#,
    )
    .expect("existing Pi config should write");
    let official = builtin_official_profiles()
        .into_iter()
        .find(|profile| profile.app == "pi")
        .expect("Pi official profile");
    let official_preview =
        build_native_config_preview(&official, None, &paths, ProviderApplyMode::Config)
            .expect("Pi official preview should build")
            .expect("Pi official preview should exist");
    assert!(official_preview
        .changes
        .iter()
        .any(|change| { change.key == "providers.codestudio" && change.action == "remove" }));
}

#[test]
fn detects_codex_native_profile_with_api_key_from_auth_json() {
    let value: toml::Value = toml::from_str(
        r#"
model_provider = "custom"
model = "gpt-5.5"

[model_providers.custom]
name = "APIKEY.FUN"
base_url = "https://api.apikey.fun/v1"
wire_api = "responses"
requires_openai_auth = false
http_headers = { "x-openai-actor-authorization" = "codestudio-lite" }
"#,
    )
    .expect("config should parse");
    let auth = serde_json::json!({
        "OPENAI_API_KEY": "sk-auth-json"
    });

    let detected = detect_codex_native_profile_with_auth(&value, Some(&auth))
        .expect("auth json backed profile should import");

    assert_eq!(detected.app, "codex");
    assert_eq!(
        normalize_detected_provider(&detected.provider, &detected.base_url),
        "apikey.fun"
    );
    assert_eq!(detected.protocol, PROTOCOL_OPENAI_RESPONSES);
    assert_eq!(detected.model, "gpt-5.5");
    assert_eq!(detected.base_url, "https://api.apikey.fun/v1");
    assert_eq!(detected.api_key, "sk-auth-json");
}

#[test]
fn detects_codex_native_profile_with_api_key_even_when_openai_auth_not_required() {
    let value: toml::Value = toml::from_str(
        r#"
model_provider = "custom"
model = "gpt-5.5"

[model_providers.custom]
name = "APIKEY.FUN"
base_url = "https://api.apikey.fun/v1"
wire_api = "responses"
requires_openai_auth = false
"#,
    )
    .expect("config should parse");
    let auth = serde_json::json!({
        "OPENAI_API_KEY": "sk-auth-json"
    });

    let detected = detect_codex_native_profile_with_auth(&value, Some(&auth))
        .expect("auth json backed profile should import");

    assert_eq!(detected.app, "codex");
    assert_eq!(detected.api_key, "sk-auth-json");
}

#[test]
fn codex_direct_profile_matches_auth_json_key_when_available() {
    let value: toml::Value = toml::from_str(
        r#"
model_provider = "custom"
model = "gpt-5.5"

[model_providers.custom]
name = "APIKEY.FUN"
base_url = "https://api.apikey.fun/v1"
wire_api = "responses"
requires_openai_auth = false
http_headers = { "x-openai-actor-authorization" = "codestudio-lite" }
"#,
    )
    .expect("config should parse");
    let auth = serde_json::json!({
        "OPENAI_API_KEY": "sk-auth-json"
    });
    let profile = ProfileDraft {
        id: "detected-codex".to_string(),
        name: "Detected Codex API".to_string(),
        icon: None,
        remark: None,
        app: "codex".to_string(),
        is_builtin: false,
        mode: ProviderApplyMode::Config,
        provider: "apikey.fun".to_string(),
        protocol: PROTOCOL_OPENAI_RESPONSES.to_string(),
        model: "gpt-5.5".to_string(),
        review_model: None,
        model_mappings: Vec::new(),
        base_url: "https://api.apikey.fun/v1".to_string(),
        auth_ref: Some("keychain:test/codex-auth-json/api_key".to_string()),
        created_at: None,
        updated_at: None,
        last_test_status: Some("detected".to_string()),
        usage_enabled: false,
        sort_order: 0,
    };
    store_test_profile_secret(&profile, "sk-auth-json");
    assert!(codex_direct_config_matches_profile(
        &value,
        Some(&auth),
        &profile
    ));
    assert!(!codex_direct_config_matches_profile(
        &value,
        Some(&serde_json::json!({ "OPENAI_API_KEY": "sk-other-auth-json" })),
        &profile,
    ));
}

#[test]
fn codex_review_model_matching_is_exact_for_custom_and_compatible_for_official() {
    let mut custom = test_profile("codex", ProviderApplyMode::Config);
    custom.provider = "compatible".to_string();
    custom.protocol = PROTOCOL_OPENAI_RESPONSES.to_string();
    custom.model = "gpt-5.5".to_string();
    custom.review_model = Some("gpt-5.6-review".to_string());
    let custom_config =
        codex_direct_config_content("", &custom).expect("custom config should render");
    let custom_value: toml::Value =
        toml::from_str(&custom_config).expect("custom config should parse");
    let auth = serde_json::json!({ "OPENAI_API_KEY": "sk-present" });

    assert!(codex_direct_config_matches_profile_without_keychain(
        &custom_value,
        Some(&auth),
        &custom,
    ));
    custom.review_model = Some("gpt-5.6-review-other".to_string());
    assert!(!codex_direct_config_matches_profile_without_keychain(
        &custom_value,
        Some(&auth),
        &custom,
    ));
    custom.review_model = None;
    assert!(!codex_direct_config_matches_profile_without_keychain(
        &custom_value,
        Some(&auth),
        &custom,
    ));
    let custom_follow_config =
        codex_direct_config_content("", &custom).expect("follow config should render");
    let mut custom_follow_value: toml::Value =
        toml::from_str(&custom_follow_config).expect("follow config should parse");
    assert_eq!(
        read_toml_string(&custom_follow_value, "review_model").as_deref(),
        Some("gpt-5.5")
    );
    assert!(codex_direct_config_matches_profile_without_keychain(
        &custom_follow_value,
        Some(&auth),
        &custom,
    ));
    custom_follow_value
        .as_table_mut()
        .expect("custom config table")
        .remove("review_model");
    assert!(codex_direct_config_matches_profile_without_keychain(
        &custom_follow_value,
        Some(&auth),
        &custom,
    ));

    let official_value: toml::Value = toml::from_str(
        r#"
model_provider = "openai"
review_model = "gpt-5.6-review"

[model_providers.openai]
requires_openai_auth = false
http_headers = { "x-openai-actor-authorization" = "codestudio-lite" }
"#,
    )
    .expect("official config should parse");
    let mut official = builtin_official_profiles()
        .into_iter()
        .find(|profile| profile.app == "codex")
        .expect("codex official profile");

    assert!(codex_official_config_matches_profile(
        &official_value,
        &official
    ));
    official.review_model = Some("gpt-5.6-review".to_string());
    assert!(codex_official_config_matches_profile(
        &official_value,
        &official
    ));
    official.review_model = Some("gpt-5.6-review-other".to_string());
    assert!(!codex_official_config_matches_profile(
        &official_value,
        &official
    ));
}

#[test]
fn detected_codex_profile_matching_includes_review_model() {
    let mut profile = test_profile("codex", ProviderApplyMode::Config);
    profile.provider = "openrouter.ai".to_string();
    profile.protocol = PROTOCOL_OPENAI_RESPONSES.to_string();
    profile.model = "gpt-5.5".to_string();
    profile.review_model = Some("gpt-5.6-review".to_string());
    profile.base_url = "https://openrouter.ai/api/v1".to_string();
    store_test_profile_secret(&profile, "sk-router");

    assert!(detected_native_profile_matches_existing_key(
        &profile,
        "codex",
        "openrouter.ai",
        PROTOCOL_OPENAI_RESPONSES,
        "gpt-5.5",
        Some("gpt-5.6-review"),
        "https://openrouter.ai/api/v1",
        "sk-router",
    ));
    assert!(!detected_native_profile_matches_existing_key(
        &profile,
        "codex",
        "openrouter.ai",
        PROTOCOL_OPENAI_RESPONSES,
        "gpt-5.5",
        Some("gpt-5.6-review-other"),
        "https://openrouter.ai/api/v1",
        "sk-router",
    ));
    profile.review_model = None;
    assert!(detected_native_profile_matches_existing_key(
        &profile,
        "codex",
        "openrouter.ai",
        PROTOCOL_OPENAI_RESPONSES,
        "gpt-5.5",
        None,
        "https://openrouter.ai/api/v1",
        "sk-router",
    ));
    assert!(detected_native_profile_matches_existing_key(
        &profile,
        "codex",
        "openrouter.ai",
        PROTOCOL_OPENAI_RESPONSES,
        "gpt-5.5",
        Some("gpt-5.5"),
        "https://openrouter.ai/api/v1",
        "sk-router",
    ));
    assert!(!detected_native_profile_matches_existing_key(
        &profile,
        "codex",
        "openrouter.ai",
        PROTOCOL_OPENAI_RESPONSES,
        "gpt-5.5",
        Some("gpt-5.6-review"),
        "https://openrouter.ai/api/v1",
        "sk-router",
    ));
}

#[test]
fn native_profile_matching_requires_auth_json_without_reading_keychain_secret() {
    let value: toml::Value = toml::from_str(
        r#"
model_provider = "codestudio-openrouter"
model = "gpt-5.5"

[model_providers.codestudio-openrouter]
name = "CodeStudio openrouter"
base_url = "https://openrouter.ai/api/v1"
wire_api = "responses"
requires_openai_auth = true
"#,
    )
    .expect("config should parse");
    let profile = ProfileDraft {
        id: "detected-codex".to_string(),
        name: "Detected Codex API".to_string(),
        icon: None,
        remark: None,
        app: "codex".to_string(),
        is_builtin: false,
        mode: ProviderApplyMode::Config,
        provider: "openrouter".to_string(),
        protocol: PROTOCOL_OPENAI_RESPONSES.to_string(),
        model: "gpt-5.5".to_string(),
        review_model: None,
        model_mappings: Vec::new(),
        base_url: "https://openrouter.ai/api/v1".to_string(),
        auth_ref: Some("keychain:test/missing-secret/api_key".to_string()),
        created_at: None,
        updated_at: None,
        last_test_status: Some("detected".to_string()),
        usage_enabled: false,
        sort_order: 0,
    };

    assert!(!codex_direct_config_matches_profile(&value, None, &profile));
    assert!(codex_direct_config_matches_profile_without_keychain(
        &value,
        Some(&serde_json::json!({ "OPENAI_API_KEY": "sk-present" })),
        &profile,
    ));
}

#[test]
fn startup_native_matching_uses_keychain_reference_without_loading_secret() {
    let mut profile = test_profile("claude", ProviderApplyMode::Config);
    profile.provider = "anthropic.test".to_string();
    profile.protocol = PROTOCOL_ANTHROPIC_MESSAGES.to_string();
    profile.model = "claude-sonnet-4-6".to_string();
    profile.base_url = "https://api.anthropic.test/v1".to_string();
    profile.auth_ref = Some("keychain:test/missing-startup-secret/api_key".to_string());

    let native_config = serde_json::json!({
        "model": "claude-sonnet-4-6",
        "env": {
            "ANTHROPIC_BASE_URL": "https://api.anthropic.test/v1",
            "ANTHROPIC_AUTH_TOKEN": "sk-native"
        }
    });

    assert!(!claude_config_matches_profile(&native_config, &profile));
    assert!(claude_config_matches_profile_without_keychain(
        &native_config,
        &profile
    ));
    assert!(!detected_native_profile_matches_existing_key(
        &profile,
        "claude",
        "anthropic.test",
        PROTOCOL_ANTHROPIC_MESSAGES,
        "claude-sonnet-4-6",
        None,
        "https://api.anthropic.test/v1",
        "sk-native",
    ));
    assert!(detected_native_profile_matches_existing_reference(
        &profile,
        "claude",
        "anthropic.test",
        PROTOCOL_ANTHROPIC_MESSAGES,
        "claude-sonnet-4-6",
        None,
        "https://api.anthropic.test/v1",
        "sk-native",
    ));
}

#[test]
fn claude_config_matching_requires_same_auth_token_for_same_url() {
    let mut profile = test_profile("claude", ProviderApplyMode::Config);
    profile.provider = "anthropic.test".to_string();
    profile.protocol = PROTOCOL_ANTHROPIC_MESSAGES.to_string();
    profile.model = "claude-sonnet-4-6".to_string();
    profile.base_url = "https://api.anthropic.test/v1".to_string();
    profile.auth_ref = Some("keychain:test/claude-same-url/api_key".to_string());
    let auth_ref = profile.auth_ref.as_deref().expect("auth ref");
    credentials::store_keychain_secret(auth_ref, "sk-old").expect("test key should store");

    let matching = serde_json::json!({
        "model": "claude-sonnet-4-6",
        "env": {
            "ANTHROPIC_BASE_URL": "https://api.anthropic.test/v1",
            "ANTHROPIC_AUTH_TOKEN": "sk-old"
        }
    });
    assert!(claude_config_matches_profile(&matching, &profile));
    assert!(detected_native_profile_matches_existing_key(
        &profile,
        "claude",
        "anthropic.test",
        PROTOCOL_ANTHROPIC_MESSAGES,
        "claude-sonnet-4-6",
        None,
        "https://api.anthropic.test/v1",
        "sk-old",
    ));

    let different_key = serde_json::json!({
        "model": "claude-sonnet-4-6",
        "env": {
            "ANTHROPIC_BASE_URL": "https://api.anthropic.test/v1",
            "ANTHROPIC_AUTH_TOKEN": "sk-new"
        }
    });
    assert!(!claude_config_matches_profile(&different_key, &profile));
    assert!(!detected_native_profile_matches_existing_key(
        &profile,
        "claude",
        "anthropic.test",
        PROTOCOL_ANTHROPIC_MESSAGES,
        "claude-sonnet-4-6",
        None,
        "https://api.anthropic.test/v1",
        "sk-new",
    ));
}

#[test]
fn non_codex_native_config_matching_requires_same_api_key_for_same_endpoint() {
    let paths = test_paths();
    let desktop_profile_path = paths.config_dir.join("claude-desktop-profile.json");
    fs::create_dir_all(
        desktop_profile_path
            .parent()
            .expect("desktop profile parent"),
    )
    .expect("desktop profile parent should be created");
    let desktop_paths = ClaudeDesktopPaths {
        normal_config_path: paths.config_dir.join("claude-normal.json"),
        threep_config_path: paths.config_dir.join("claude-threep.json"),
        profile_path: desktop_profile_path.clone(),
        meta_path: paths.config_dir.join("claude-meta.json"),
        developer_settings_paths: Vec::new(),
    };
    let mut desktop_profile = test_profile("claude-desktop", ProviderApplyMode::Config);
    desktop_profile.provider = "anthropic.test".to_string();
    desktop_profile.protocol = PROTOCOL_ANTHROPIC_MESSAGES.to_string();
    desktop_profile.model = "claude-sonnet-4-6".to_string();
    desktop_profile.base_url = "https://api.anthropic.test/v1".to_string();
    desktop_profile.auth_ref = Some("keychain:test/claude-desktop-same-url/api_key".to_string());
    store_test_profile_secret(&desktop_profile, "sk-desktop-old");
    fs::write(
        &desktop_profile_path,
        serde_json::json!({
            "inferenceProvider": "gateway",
            "inferenceGatewayBaseUrl": "https://api.anthropic.test/v1",
            "inferenceGatewayApiKey": "sk-desktop-old",
            "inferenceModels": ["claude-sonnet-4-6"]
        })
        .to_string(),
    )
    .expect("desktop profile should be written");
    assert!(claude_desktop_config_matches_profile(
        &desktop_profile,
        Some(&desktop_paths),
        false,
    ));
    fs::write(
        &desktop_profile_path,
        serde_json::json!({
            "inferenceProvider": "gateway",
            "inferenceGatewayBaseUrl": "https://api.anthropic.test/v1",
            "inferenceGatewayApiKey": "sk-desktop-new",
            "inferenceModels": ["claude-sonnet-4-6"]
        })
        .to_string(),
    )
    .expect("desktop profile should be updated");
    assert!(!claude_desktop_config_matches_profile(
        &desktop_profile,
        Some(&desktop_paths),
        false,
    ));

    let mut code_assist_profile = test_profile("gemini-code-assist", ProviderApplyMode::Config);
    code_assist_profile.provider = "gemini-compatible".to_string();
    code_assist_profile.protocol = PROTOCOL_GOOGLE_GEMINI.to_string();
    code_assist_profile.auth_ref =
        Some("keychain:test/gemini-code-assist-same-url/api_key".to_string());
    store_test_profile_secret(&code_assist_profile, "sk-code-assist-old");
    assert!(gemini_code_assist_settings_match_profile(
        &serde_json::json!({ GEMINI_CODE_ASSIST_API_KEY_SETTING: "sk-code-assist-old" }),
        &code_assist_profile,
    ));
    assert!(!gemini_code_assist_settings_match_profile(
        &serde_json::json!({ GEMINI_CODE_ASSIST_API_KEY_SETTING: "sk-code-assist-new" }),
        &code_assist_profile,
    ));

    let mut opencode_profile = test_profile("opencode", ProviderApplyMode::Config);
    opencode_profile.provider = "openrouter".to_string();
    opencode_profile.protocol = PROTOCOL_OPENAI_CHAT_COMPLETIONS.to_string();
    opencode_profile.model = "gpt-5.5".to_string();
    opencode_profile.base_url = "https://openrouter.ai/api/v1".to_string();
    opencode_profile.auth_ref = Some("keychain:test/opencode-same-url/api_key".to_string());
    store_test_profile_secret(&opencode_profile, "sk-opencode-old");
    assert!(opencode_config_matches_profile(
        &serde_json::json!({
            "model": "custom/gpt-5.5",
            "provider": {
                "custom": {
                    "options": {
                        "baseURL": "https://openrouter.ai/api/v1",
                        "apiKey": "sk-opencode-old"
                    }
                }
            }
        }),
        &opencode_profile,
    ));
    assert!(!opencode_config_matches_profile(
        &serde_json::json!({
            "model": "custom/gpt-5.5",
            "provider": {
                "custom": {
                    "options": {
                        "baseURL": "https://openrouter.ai/api/v1",
                        "apiKey": "sk-opencode-new"
                    }
                }
            }
        }),
        &opencode_profile,
    ));

    let mut openclaw_profile = test_profile("openclaw", ProviderApplyMode::Config);
    openclaw_profile.provider = "openrouter".to_string();
    openclaw_profile.protocol = PROTOCOL_OPENAI_CHAT_COMPLETIONS.to_string();
    openclaw_profile.model = "gpt-5.5".to_string();
    openclaw_profile.base_url = "https://openrouter.ai/api/v1".to_string();
    openclaw_profile.auth_ref = Some("keychain:test/openclaw-same-url/api_key".to_string());
    store_test_profile_secret(&openclaw_profile, "sk-openclaw-old");
    assert!(openclaw_config_matches_profile(
        &serde_json::json!({
            "agents": { "defaults": { "model": { "primary": "custom/gpt-5.5" } } },
            "models": {
                "providers": {
                    "custom": {
                        "baseUrl": "https://openrouter.ai/api/v1",
                        "apiKey": "sk-openclaw-old"
                    }
                }
            }
        }),
        &openclaw_profile,
    ));
    assert!(!openclaw_config_matches_profile(
        &serde_json::json!({
            "agents": { "defaults": { "model": { "primary": "custom/gpt-5.5" } } },
            "models": {
                "providers": {
                    "custom": {
                        "baseUrl": "https://openrouter.ai/api/v1",
                        "apiKey": "sk-openclaw-new"
                    }
                }
            }
        }),
        &openclaw_profile,
    ));

    let mut hermes_profile = test_profile("hermes", ProviderApplyMode::Config);
    hermes_profile.provider = "openrouter".to_string();
    hermes_profile.protocol = PROTOCOL_OPENAI_CHAT_COMPLETIONS.to_string();
    hermes_profile.model = "gpt-5.5".to_string();
    hermes_profile.base_url = "https://openrouter.ai/api/v1".to_string();
    hermes_profile.auth_ref = Some("keychain:test/hermes-same-url/api_key".to_string());
    store_test_profile_secret(&hermes_profile, "sk-hermes-old");
    let hermes_old = parse_yaml_or_empty(
        r#"
model:
  provider: custom
  base_url: https://openrouter.ai/api/v1
  api_key: sk-hermes-old
  api_mode: chat_completions
  default: gpt-5.5
"#,
        "Hermes config",
    )
    .expect("Hermes config should parse");
    assert!(hermes_config_matches_profile(&hermes_old, &hermes_profile));
    let hermes_new = parse_yaml_or_empty(
        r#"
model:
  provider: custom
  base_url: https://openrouter.ai/api/v1
  api_key: sk-hermes-new
  api_mode: chat_completions
  default: gpt-5.5
"#,
        "Hermes config",
    )
    .expect("Hermes config should parse");
    assert!(!hermes_config_matches_profile(&hermes_new, &hermes_profile));
}

#[test]
fn codex_direct_config_rewrites_legacy_inline_provider_to_custom_table() {
    let mut profile = test_profile("codex", ProviderApplyMode::Config);
    profile.provider = "compatible".to_string();
    profile.protocol = PROTOCOL_OPENAI_RESPONSES.to_string();
    profile.model = "gpt-5.5".to_string();
    profile.base_url = "https://api.apikey.fun/v1".to_string();

    let config = codex_direct_config_content(
            r#"
        model_provider = "custom"
        model_providers = { custom = { name = "APIKEY.FUN", wire_api = "responses", base_url = "https://api.apikey.fun/v1", requires_openai_auth = false } , openai = { name = "OpenAI", wire_api = "responses", requires_openai_auth = true } }
model_reasoning_effort = "xhigh"
"#,
            &profile,
        )
        .expect("config should render");
    let value: toml::Value = toml::from_str(&config).expect("config should parse");

    assert_eq!(
        read_toml_string(&value, "model_provider").as_deref(),
        Some("custom")
    );
    assert_eq!(
        read_toml_string(&value, "model").as_deref(),
        Some("gpt-5.5")
    );
    assert_eq!(
        toml_lookup(&value, "model_providers.custom.name").and_then(|item| item.as_str()),
        Some("compatible")
    );
    assert_eq!(
        toml_lookup(&value, "model_providers.custom.base_url").and_then(|item| item.as_str()),
        Some("https://api.apikey.fun/v1")
    );
    assert_eq!(
        toml_lookup(&value, "model_providers.custom.wire_api").and_then(|item| item.as_str()),
        Some("responses")
    );
    assert_codex_managed_provider_contract(&value, "custom");
    assert_eq!(
        read_toml_string(&value, "cli_auth_credentials_store").as_deref(),
        Some("file")
    );
    assert!(config.contains("[model_providers]\n"));
    assert!(config.contains("[model_providers.custom]\n"));
    assert!(!config.contains("model_providers = {"));
    assert!(!config.contains("model_provider = \"codestudio-"));
    assert!(!config.contains("[model_providers.codestudio-"));
    assert_codex_managed_provider_contract_lines(&config);
}

#[test]
fn direct_config_runtime_base_url_adds_v1_without_changing_profile_value() {
    let mut profile = test_profile("codex", ProviderApplyMode::Config);
    profile.provider = "compatible".to_string();
    profile.protocol = PROTOCOL_OPENAI_RESPONSES.to_string();
    profile.model = "gpt-5.5".to_string();
    profile.base_url = "https://api.apikey.fun/".to_string();

    let config = codex_direct_config_content("", &profile).expect("config should render");
    let value: toml::Value = toml::from_str(&config).expect("config should parse");

    assert_eq!(profile.base_url, "https://api.apikey.fun/");
    assert_eq!(
        toml_lookup(&value, "model_providers.custom.base_url").and_then(|item| item.as_str()),
        Some("https://api.apikey.fun/v1")
    );
    assert!(codex_direct_config_matches_profile_without_keychain(
        &value,
        Some(&serde_json::json!({ "OPENAI_API_KEY": "sk-present" })),
        &profile,
    ));
}

#[test]
fn direct_config_runtime_base_url_does_not_add_v1_for_non_openai_protocols() {
    let mut claude_profile = test_profile("claude", ProviderApplyMode::Config);
    claude_profile.provider = "anthropic-compatible".to_string();
    claude_profile.protocol = PROTOCOL_ANTHROPIC_MESSAGES.to_string();
    claude_profile.base_url = "https://api.anthropic.test".to_string();
    let claude_config = claude_config_content_with_api_key("{}", &claude_profile, "sk-test")
        .expect("Claude config should render");
    let claude_value = parse_json5_or_empty(&claude_config, "Claude settings")
        .expect("Claude config should parse");
    assert_eq!(
        json_string_lookup(&claude_value, &["env", "ANTHROPIC_BASE_URL"]).as_deref(),
        Some("https://api.anthropic.test/")
    );
}

#[test]
fn codex_gateway_config_uses_custom_provider_table() {
    let mut profile = test_profile("codex", ProviderApplyMode::Gateway);
    profile.model = "gpt-5.5".to_string();
    let config = codex_gateway_config_content(
            r#"
model_provider = "codestudio-local"
model_providers = { codestudio-local = { name = "CodeStudio Lite Local Gateway", wire_api = "responses", base_url = "http://127.0.0.1:43112/tools/codex/v1", requires_openai_auth = false } }
"#,
            &profile,
        )
        .expect("config should render");
    let value: toml::Value = toml::from_str(&config).expect("config should parse");

    assert_eq!(
        read_toml_string(&value, "model_provider").as_deref(),
        Some("custom")
    );
    assert_eq!(
        read_toml_string(&value, "model").as_deref(),
        Some("gpt-5.5")
    );
    assert_eq!(
        toml_lookup(&value, "model_providers.custom.wire_api").and_then(|item| item.as_str()),
        Some("responses")
    );
    assert_codex_managed_provider_contract(&value, "custom");
    assert_eq!(
        read_toml_string(&value, "cli_auth_credentials_store").as_deref(),
        Some("file")
    );
    assert_eq!(
        toml_lookup(&value, "model_providers.custom.base_url").and_then(|item| item.as_str()),
        Some("http://127.0.0.1:43112/tools/codex/v1")
    );
    assert!(config.contains("[model_providers]\n"));
    assert!(config.contains("[model_providers.custom]\n"));
    assert!(!config.contains("model_providers.codestudio-local"));
    assert!(!config.contains("model_providers = {"));
    assert_codex_managed_provider_contract_lines(&config);
}

#[test]
fn codex_review_model_is_written_and_blank_follows_primary_model() {
    let current = "review_model = \"old-review\"\n";

    let mut direct = test_profile("codex", ProviderApplyMode::Config);
    direct.provider = "compatible".to_string();
    direct.protocol = PROTOCOL_OPENAI_RESPONSES.to_string();
    direct.model = "gpt-5.5".to_string();
    direct.review_model = Some("  gpt-5.6-review  ".to_string());
    let direct_content =
        codex_direct_config_content(current, &direct).expect("direct config should render");
    let direct_value: toml::Value =
        toml::from_str(&direct_content).expect("direct config should parse");
    assert_eq!(
        read_toml_string(&direct_value, "review_model").as_deref(),
        Some("gpt-5.6-review")
    );
    direct.review_model = None;
    let direct_followed = codex_direct_config_content(&direct_content, &direct)
        .expect("direct config should follow the primary model");
    let direct_followed_value: toml::Value =
        toml::from_str(&direct_followed).expect("direct config should parse");
    assert_eq!(
        read_toml_string(&direct_followed_value, "review_model").as_deref(),
        Some("gpt-5.5")
    );

    let mut gateway = test_profile("codex", ProviderApplyMode::Gateway);
    gateway.model = "gpt-5.5".to_string();
    gateway.review_model = Some("gpt-5.6-review".to_string());
    let gateway_content =
        codex_gateway_config_content(current, &gateway).expect("gateway config should render");
    let gateway_value: toml::Value =
        toml::from_str(&gateway_content).expect("gateway config should parse");
    assert_eq!(
        read_toml_string(&gateway_value, "review_model").as_deref(),
        Some("gpt-5.6-review")
    );
    gateway.review_model = None;
    let gateway_followed = codex_gateway_config_content(&gateway_content, &gateway)
        .expect("gateway config should follow the primary model");
    let gateway_followed_value: toml::Value =
        toml::from_str(&gateway_followed).expect("gateway config should parse");
    assert_eq!(
        read_toml_string(&gateway_followed_value, "review_model").as_deref(),
        Some("gpt-5.5")
    );
    gateway.model.clear();
    let gateway_default = codex_gateway_config_content(&gateway_followed, &gateway)
        .expect("gateway config should follow its fallback model");
    let gateway_default_value: toml::Value =
        toml::from_str(&gateway_default).expect("gateway config should parse");
    assert_eq!(
        read_toml_string(&gateway_default_value, "review_model").as_deref(),
        Some(GATEWAY_FALLBACK_MODEL)
    );

    let mut official = builtin_official_profiles()
        .into_iter()
        .find(|profile| profile.app == "codex")
        .expect("codex official profile");
    official.model = "gpt-5.5".to_string();
    official.review_model = Some("gpt-5.6-review".to_string());
    let official_content =
        codex_official_config_content(current, &official).expect("official config should render");
    let official_value: toml::Value =
        toml::from_str(&official_content).expect("official config should parse");
    assert_eq!(
        read_toml_string(&official_value, "review_model").as_deref(),
        Some("gpt-5.6-review")
    );
    official.review_model = None;
    let official_followed = codex_official_config_content(&official_content, &official)
        .expect("official config should follow the primary model");
    let official_followed_value: toml::Value =
        toml::from_str(&official_followed).expect("official config should parse");
    assert_eq!(
        read_toml_string(&official_followed_value, "review_model").as_deref(),
        Some("gpt-5.5")
    );
    official.model.clear();
    let official_without_model = codex_official_config_content(&official_followed, &official)
        .expect("official config without a primary model should remove review model");
    let official_without_model_value: toml::Value =
        toml::from_str(&official_without_model).expect("official config should parse");
    assert!(read_toml_string(&official_without_model_value, "review_model").is_none());
}

#[test]
fn review_model_normalization_is_codex_only_and_blank_safe() {
    assert_eq!(
        normalize_profile_review_model("codex", Some("  gpt-5.6-review  ")),
        Some("gpt-5.6-review".to_string())
    );
    assert_eq!(
        normalize_profile_review_model("chatgpt-desktop", Some("gpt-5.6-review")),
        Some("gpt-5.6-review".to_string())
    );
    assert_eq!(normalize_profile_review_model("codex", Some("  ")), None);
    assert_eq!(
        normalize_profile_review_model("claude", Some("gpt-5.6-review")),
        None
    );
}

#[test]
fn codex_direct_config_without_model_does_not_write_legacy_default_model() {
    let mut profile = test_profile("codex", ProviderApplyMode::Config);
    profile.provider = "compatible".to_string();
    profile.protocol = PROTOCOL_OPENAI_RESPONSES.to_string();
    profile.model = String::new();

    let config = codex_direct_config_content(
        "model = \"old-model\"\nreview_model = \"old-review\"\n",
        &profile,
    )
    .expect("config should render");
    let value: toml::Value = toml::from_str(&config).expect("config should parse");

    assert!(read_toml_string(&value, "model").is_none());
    assert!(read_toml_string(&value, "review_model").is_none());
    assert!(!config.contains("codestudio-default"));
}

#[test]
fn gateway_configs_use_profile_model_instead_of_legacy_default_model() {
    let mut profile = test_profile("claude", ProviderApplyMode::Gateway);
    profile.model = "gpt-5.5".to_string();

    let claude = claude_gateway_config_content("{}", &profile).expect("Claude config");
    assert!(claude.contains("gpt-5.5"));
    assert!(!claude.contains("codestudio-default"));

    profile.app = "opencode".to_string();
    let opencode = opencode_gateway_config_content("{}", &profile).expect("OpenCode config");
    assert!(opencode.contains("custom/gpt-5.5"));
    assert!(!opencode.contains("codestudio-default"));

    profile.app = "openclaw".to_string();
    let openclaw = openclaw_gateway_config_content("{}", &profile).expect("OpenClaw config");
    assert!(openclaw.contains("custom/gpt-5.5"));
    assert!(!openclaw.contains("codestudio-default"));

    profile.app = "hermes".to_string();
    let hermes = hermes_gateway_config_content("", &profile).expect("Hermes config");
    let hermes_yaml = parse_yaml_or_empty(&hermes, "Hermes config").expect("Hermes YAML");
    assert_eq!(
        yaml_string_lookup(&hermes_yaml, &["model", "default"]).as_deref(),
        Some("gpt-5.5")
    );
    assert!(!hermes.contains("codestudio-default"));
}

#[test]
fn json_provider_configs_use_custom_provider_id() {
    let mut profile = test_profile("opencode", ProviderApplyMode::Config);
    profile.provider = "compatible".to_string();
    profile.protocol = PROTOCOL_OPENAI_RESPONSES.to_string();
    profile.model = "gpt-5.5".to_string();
    profile.base_url = "https://api.apikey.fun/v1".to_string();

    let config = opencode_config_content_with_api_key(
        r#"{"provider":{"custom":{"name":"old"}}}"#,
        &profile,
        "sk-new",
    )
    .expect("opencode config should render");
    let value = parse_json5_or_empty(&config, "OpenCode config").expect("json");
    assert_eq!(
        json_string_lookup(&value, &["model"]).as_deref(),
        Some("custom/gpt-5.5")
    );
    assert_eq!(
        json_string_lookup(&value, &["provider", "custom", "name"]).as_deref(),
        Some("compatible")
    );
    assert!(!config.contains("codestudio-"));

    profile.app = "openclaw".to_string();
    profile.protocol = PROTOCOL_OPENAI_CHAT_COMPLETIONS.to_string();
    let config = openclaw_config_content_with_api_key(
        r#"{"models":{"providers":{"custom":{"name":"old"}}}}"#,
        &profile,
        "sk-new",
    )
    .expect("openclaw config should render");
    let value = parse_json5_or_empty(&config, "OpenClaw config").expect("json");
    assert_eq!(
        json_string_lookup(&value, &["agents", "defaults", "model", "primary"]).as_deref(),
        Some("custom/gpt-5.5")
    );
    assert_eq!(
        json_string_lookup(&value, &["models", "providers", "custom", "name"]).as_deref(),
        Some("compatible")
    );
    assert!(!config.contains("codestudio-"));
}

#[test]
fn skips_official_and_local_gateway_native_profiles() {
    let official: toml::Value = toml::from_str(
        r#"
model_provider = "openai"

[model_providers.openai]
wire_api = "responses"
requires_openai_auth = true
"#,
    )
    .expect("config should parse");
    assert!(detect_codex_native_profile(&official).is_none());

    let gateway: toml::Value = toml::from_str(
        r#"
model_provider = "codestudio-local"
model = "codestudio-default"

[model_providers.codestudio-local]
base_url = "http://127.0.0.1:43112/tools/codex/v1"
wire_api = "responses"
"#,
    )
    .expect("config should parse");
    assert!(detect_codex_native_profile(&gateway).is_none());
}

#[test]
fn detects_json_env_native_profiles() {
    let claude = serde_json::json!({
        "model": "claude-sonnet-4-6",
        "env": {
            "ANTHROPIC_BASE_URL": "https://api.anthropic.test/v1",
            "ANTHROPIC_AUTH_TOKEN": "sk-claude"
        }
    });
    let detected = detect_claude_native_profile(&claude).expect("claude profile");
    assert_eq!(detected.app, "claude");
    assert_eq!(detected.protocol, PROTOCOL_ANTHROPIC_MESSAGES);
    assert_eq!(detected.model, "claude-sonnet-4-6");
    assert_eq!(detected.base_url, "https://api.anthropic.test/v1");
    assert_eq!(detected.api_key, "sk-claude");

    let gemini_code_assist =
        serde_json::json!({ GEMINI_CODE_ASSIST_API_KEY_SETTING: "sk-code-assist" });
    let detected = detect_gemini_code_assist_native_profile(&gemini_code_assist)
        .expect("gemini code assist profile");
    assert_eq!(detected.app, "gemini-code-assist");
    assert_eq!(detected.protocol, PROTOCOL_GOOGLE_GEMINI);
    assert_eq!(
        detected.base_url,
        "https://generativelanguage.googleapis.com/v1beta"
    );
}

#[test]
fn detects_json_provider_native_profiles() {
    let opencode = serde_json::json!({
        "model": "openrouter/gpt-5.5",
        "provider": {
            "openrouter": {
                "name": "OpenRouter",
                "options": {
                    "baseURL": "https://openrouter.ai/api/v1",
                    "apiKey": "sk-openrouter"
                }
            }
        }
    });
    let detected = detect_opencode_native_profile(&opencode).expect("opencode profile");
    assert_eq!(detected.app, "opencode");
    assert_eq!(detected.provider, "OpenRouter");
    assert_eq!(detected.protocol, PROTOCOL_OPENAI_CHAT_COMPLETIONS);
    assert_eq!(detected.model, "gpt-5.5");

    let openclaw = serde_json::json!({
        "agents": {
            "defaults": {
                "model": {
                    "primary": "openrouter/claude-sonnet"
                }
            }
        },
        "models": {
            "providers": {
                "openrouter": {
                    "name": "OpenRouter",
                    "baseUrl": "https://openrouter.ai/api/v1",
                    "apiKey": "sk-openrouter"
                }
            }
        }
    });
    let detected = detect_openclaw_native_profile(&openclaw).expect("openclaw profile");
    assert_eq!(detected.app, "openclaw");
    assert_eq!(detected.provider, "OpenRouter");
    assert_eq!(detected.model, "claude-sonnet");
}

#[test]
fn detects_hermes_native_profile() {
    let value = parse_yaml_or_empty(
        r#"
model:
  provider: custom
  base_url: https://openrouter.ai/api/v1
  api_key: sk-hermes
  api_mode: chat_completions
  default: gpt-5.5
"#,
        "Hermes config",
    )
    .expect("yaml should parse");
    let detected = detect_hermes_native_profile(&value).expect("hermes profile");

    assert_eq!(detected.app, "hermes");
    assert_eq!(detected.protocol, PROTOCOL_OPENAI_CHAT_COMPLETIONS);
    assert_eq!(detected.model, "gpt-5.5");
    assert_eq!(detected.base_url, "https://openrouter.ai/api/v1");
    assert_eq!(detected.api_key, "sk-hermes");
}

#[test]
fn codex_native_config_uses_auth_json_for_relay_injection() {
    let profile = test_profile("codex", ProviderApplyMode::Gateway);
    let config = codex_gateway_config_content("", &profile).expect("config should render");
    let value: toml::Value = toml::from_str(&config).expect("config should parse");

    assert_eq!(
        read_toml_string(&value, "model_provider").as_deref(),
        Some("custom")
    );
    assert_eq!(
        toml_lookup(&value, "model_providers.custom.wire_api").and_then(|item| item.as_str()),
        Some("responses")
    );
    assert_codex_managed_provider_contract(&value, "custom");
    assert_eq!(
        read_toml_string(&value, "cli_auth_credentials_store").as_deref(),
        Some("file")
    );
    assert_eq!(
        toml_lookup(&value, "model_providers.custom.base_url").and_then(|item| item.as_str()),
        Some("http://127.0.0.1:43112/tools/codex/v1")
    );
    assert_codex_managed_provider_contract_lines(&config);
}

#[test]
fn codex_official_config_uses_managed_auth_contract_without_base_url_override() {
    let profile = builtin_official_profiles()
        .into_iter()
        .find(|profile| profile.app == "codex")
        .expect("codex official profile");
    let config = codex_official_config_content(
        r#"
[auth]
api_key = "legacy-auth-key"

[model_providers.openai]
base_url = "https://example.invalid/v1"
"#,
        &profile,
    )
    .expect("config should render");
    let value: toml::Value = toml::from_str(&config).expect("config should parse");

    assert_eq!(
        read_toml_string(&value, "model_provider").as_deref(),
        Some("openai")
    );
    assert_codex_managed_provider_contract(&value, "openai");
    assert_eq!(
        read_toml_string(&value, "cli_auth_credentials_store").as_deref(),
        Some("file")
    );
    assert!(toml_lookup(&value, "model_providers.openai.base_url").is_none());
    assert!(toml_lookup(&value, "auth.api_key").is_none());
    assert!(config.contains("[model_providers.openai]"));
    assert!(!config.contains("base_url ="));
    assert_codex_managed_provider_contract_lines(&config);
}

#[test]
fn codex_auth_status_infers_chatgpt_cache_without_exposing_values() {
    let auth_path = Path::new("auth.json");
    let status = codex_auth_status_from_file_content(
        auth_path,
        "file",
        r#"{
  "tokens": {
    "access_token": "secret-access-token",
    "refresh_token": "secret-refresh-token"
  },
  "account_id": "acct-secret"
}"#,
    );

    assert!(status.available);
    assert!(matches!(status.method, CodexAuthMethod::ChatGpt));
    assert!(matches!(status.storage, CodexAuthStorage::AuthJson));
    assert!(!status.detail.contains("secret-access-token"));
    assert!(!status.detail.contains("secret-refresh-token"));
}

#[test]
fn codex_auth_status_infers_api_key_cache_without_exposing_values() {
    let auth_path = Path::new("auth.json");
    let status = codex_auth_status_from_file_content(
        auth_path,
        "file",
        r#"{
  "openai_api_key": "sk-secret"
}"#,
    );

    assert!(status.available);
    assert!(matches!(status.method, CodexAuthMethod::ApiKey));
    assert!(!status.detail.contains("sk-secret"));

    let upper_status = codex_auth_status_from_file_content(
        auth_path,
        "file",
        r#"{
  "OPENAI_API_KEY": "sk-secret"
}"#,
    );

    assert!(upper_status.available);
    assert!(matches!(upper_status.method, CodexAuthMethod::ApiKey));
    assert!(!upper_status.detail.contains("sk-secret"));

    let bearer_status = codex_auth_status_from_file_content(
        auth_path,
        "file",
        r#"{
  "experimental_bearer_token": "sk-secret"
}"#,
    );

    assert!(bearer_status.available);
    assert!(matches!(bearer_status.method, CodexAuthMethod::ApiKey));
    assert!(!bearer_status.detail.contains("sk-secret"));
}

#[test]
fn codex_auth_status_prefers_explicit_api_key_mode_with_preserved_oauth_tokens() {
    let status = codex_auth_status_from_file_content(
        Path::new("auth.json"),
        "file",
        r#"{
  "auth_mode": "apikey",
  "OPENAI_API_KEY": "sk-secret",
  "tokens": {
    "access_token": "oauth-access",
    "refresh_token": "oauth-refresh"
  }
}"#,
    );

    assert!(status.available);
    assert!(matches!(status.method, CodexAuthMethod::ApiKey));
}

#[test]
fn codex_auth_json_api_key_content_matches_cli_format_and_preserves_oauth_tokens() {
    let content = codex_auth_json_content_with_api_key(
        r#"{
  "auth_mode": "chatgpt",
  "OPENAI_API_KEY": "stale-uppercase-key",
  "openai_api_key": "stale-lowercase-key",
  "api_key": "stale-legacy-key",
  "tokens": {
    "access_token": "oauth-access",
    "refresh_token": "oauth-refresh"
  },
  "other": "keep"
}"#,
        "sk-current",
    )
    .expect("auth json should render");
    let value: serde_json::Value = serde_json::from_str(&content).expect("auth json should parse");

    assert_eq!(
        value.get("auth_mode").and_then(serde_json::Value::as_str),
        Some("apikey")
    );
    assert_eq!(
        value
            .get("experimental_bearer_token")
            .and_then(serde_json::Value::as_str),
        Some("sk-current")
    );
    assert!(value.get("OPENAI_API_KEY").is_none());
    assert!(value.get("openai_api_key").is_none());
    assert!(value.get("api_key").is_none());
    assert_eq!(
        value
            .pointer("/tokens/refresh_token")
            .and_then(serde_json::Value::as_str),
        Some("oauth-refresh")
    );
    assert_eq!(
        value.get("other").and_then(serde_json::Value::as_str),
        Some("keep")
    );
}

#[test]
fn codex_official_auth_json_restores_preserved_oauth_mode() {
    let content = codex_official_auth_json_content(
        r#"{
  "auth_mode": "apikey",
  "OPENAI_API_KEY": "codestudio-local-test",
  "experimental_bearer_token": "codestudio-local-test",
  "tokens": {
    "access_token": "oauth-access",
    "refresh_token": "oauth-refresh"
  },
  "other": "keep"
}"#,
    )
    .expect("official auth json should render")
    .expect("oauth markers should produce a restored payload");
    let value: serde_json::Value = serde_json::from_str(&content).expect("auth json should parse");

    assert_eq!(
        value.get("auth_mode").and_then(serde_json::Value::as_str),
        Some("chatgpt")
    );
    assert!(value.get("OPENAI_API_KEY").is_none());
    assert!(value.get("experimental_bearer_token").is_none());
    assert_eq!(
        value
            .pointer("/tokens/access_token")
            .and_then(serde_json::Value::as_str),
        Some("oauth-access")
    );
    assert_eq!(
        value.get("other").and_then(serde_json::Value::as_str),
        Some("keep")
    );
}

#[test]
fn codex_preserved_auth_repair_removes_legacy_openai_key_locations() {
    let mut document = r#"
[auth]
OPENAI_API_KEY = "auth-key"
api_key = "auth-key-legacy"

[env]
OPENAI_API_KEY = "env-key"
OTHER = "keep"
"#
    .parse::<toml_edit::DocumentMut>()
    .expect("config should parse");

    repair_codex_preserved_auth_config(&mut document);
    let value: toml::Value = toml::from_str(&document.to_string()).expect("config toml");

    assert!(toml_lookup(&value, "auth.OPENAI_API_KEY").is_none());
    assert!(toml_lookup(&value, "auth.api_key").is_none());
    assert!(toml_lookup(&value, "env.OPENAI_API_KEY").is_none());
    assert_eq!(
        toml_lookup(&value, "env.OTHER").and_then(|item| item.as_str()),
        Some("keep")
    );
}

#[test]
fn claude_desktop_profile_uses_3p_gateway_shape() {
    let value = claude_desktop_gateway_profile_value(
        "http://127.0.0.1:43112/tools/claude-desktop",
        "local-token",
        Some(&[ClaudeDesktopInferenceModelSpec {
            name: "claude-sonnet-4-6".to_string(),
            label_override: Some("Upstream Model".to_string()),
            supports_1m: true,
        }]),
    );

    assert_eq!(value["inferenceProvider"].as_str(), Some("gateway"));
    assert_eq!(
        value["inferenceGatewayBaseUrl"].as_str(),
        Some("http://127.0.0.1:43112/tools/claude-desktop")
    );
    assert_eq!(
        value["inferenceGatewayApiKey"].as_str(),
        Some("local-token")
    );
    assert_eq!(
        value["inferenceModels"][0]["name"].as_str(),
        Some("claude-sonnet-4-6")
    );
    assert_eq!(
        value["inferenceModels"][0]["labelOverride"].as_str(),
        Some("Upstream Model")
    );
    assert_eq!(
        value["inferenceModels"][0]["supports1m"].as_bool(),
        Some(true)
    );
}

#[test]
fn claude_desktop_meta_apply_and_restore_updates_managed_entry() {
    let applied = claude_desktop_meta_content(
        r#"{"entries":[{"id":"other","name":"Other"}],"appliedId":"other"}"#,
        true,
    )
    .expect("meta should render");
    let value = parse_json5_or_empty(&applied, "meta").expect("json");
    assert_eq!(
        json_string_lookup(&value, &["appliedId"]).as_deref(),
        Some(CLAUDE_DESKTOP_PROFILE_ID)
    );
    assert!(value["entries"].as_array().unwrap().iter().any(|entry| {
        entry.get("id").and_then(serde_json::Value::as_str) == Some(CLAUDE_DESKTOP_PROFILE_ID)
    }));

    let restored =
        claude_desktop_meta_content(&applied, false).expect("restore meta should render");
    let value = parse_json5_or_empty(&restored, "meta").expect("json");
    assert_ne!(
        json_string_lookup(&value, &["appliedId"]).as_deref(),
        Some(CLAUDE_DESKTOP_PROFILE_ID)
    );
    assert!(!value["entries"].as_array().unwrap().iter().any(|entry| {
        entry.get("id").and_then(serde_json::Value::as_str) == Some(CLAUDE_DESKTOP_PROFILE_ID)
    }));
}

#[test]
fn claude_desktop_deployment_mode_preserves_unrelated_config() {
    let config = claude_desktop_deployment_config_content(
        r#"{"foo":"bar","enterpriseConfig":{"inferenceProvider":"gateway","keep":"yes"}}"#,
        "1p",
        true,
    )
    .expect("deployment config should render");
    let value = parse_json5_or_empty(&config, "deployment").expect("json");

    assert_eq!(
        json_string_lookup(&value, &["deploymentMode"]).as_deref(),
        Some("1p")
    );
    assert_eq!(json_string_lookup(&value, &["foo"]).as_deref(), Some("bar"));
    assert_eq!(
        json_string_lookup(&value, &["enterpriseConfig", "keep"]).as_deref(),
        Some("yes")
    );
    assert!(json_string_lookup(&value, &["enterpriseConfig", "inferenceProvider"]).is_none());
}

#[test]
fn claude_desktop_developer_settings_enable_devtools_preserving_values() {
    let config =
        claude_desktop_developer_settings_content(r#"{"foo":"bar","allowDevTools":false}"#)
            .expect("developer settings should render");
    let value = parse_json5_or_empty(&config, "developer settings").expect("json");

    assert_eq!(json_string_lookup(&value, &["foo"]).as_deref(), Some("bar"));
    assert_eq!(
        value
            .get("allowDevTools")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert!(claude_desktop_developer_mode_enabled(&config).expect("settings should parse"));
}

#[test]
fn claude_desktop_developer_settings_plan_only_when_disabled() {
    let mut paths = claude_desktop_paths_from_dirs(
        PathBuf::from("C:/Users/example/AppData/Local/Claude"),
        PathBuf::from("C:/Users/example/AppData/Local/Claude-3p"),
        vec![PathBuf::from(
            "C:/Users/example/AppData/Roaming/Claude/developer_settings.json",
        )],
    );
    paths.developer_settings_paths = vec![paths
        .developer_settings_paths
        .first()
        .expect("path")
        .clone()];

    let plans = build_claude_desktop_developer_settings_plans(&paths).expect("plan should build");
    assert_eq!(plans.len(), 1);
    assert!(plans[0].content.contains("\"allowDevTools\": true"));
}

#[test]
fn claude_desktop_macos_developer_settings_cover_normal_and_threep_dirs() {
    let paths = macos_claude_desktop_developer_settings_paths(
        Path::new("/Users/example/Library/Application Support/Claude"),
        Path::new("/Users/example/Library/Application Support/Claude-3p"),
    );

    assert_eq!(
        paths,
        vec![
            PathBuf::from(
                "/Users/example/Library/Application Support/Claude/developer_settings.json"
            ),
            PathBuf::from(
                "/Users/example/Library/Application Support/Claude-3p/developer_settings.json"
            ),
        ]
    );
}

#[test]
fn claude_desktop_gateway_base_url_strips_v1_suffix() {
    assert_eq!(
        claude_desktop_gateway_profile_base_url("http://127.0.0.1:43112/tools/claude-desktop/v1"),
        "http://127.0.0.1:43112/tools/claude-desktop"
    );
}

#[test]
fn claude_desktop_safe_model_ids_match_desktop_routes() {
    assert!(claude_desktop_safe_model_id("claude-sonnet-4-6"));
    assert!(claude_desktop_safe_model_id("anthropic/claude-haiku-4-5"));
    assert!(!claude_desktop_safe_model_id("claude-sonnet-4-6[1m]"));
    assert!(!claude_desktop_safe_model_id("gpt-5.5"));
}

#[test]
fn native_config_paths_route_supported_tools() {
    let paths = test_paths();
    let mut profile = test_profile("claude", ProviderApplyMode::Config);
    assert_eq!(
        native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Config)
            .expect("path should resolve"),
        Some(paths.home_dir.join(".claude").join("settings.json"))
    );
    assert_eq!(
        native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Gateway)
            .expect("gateway path should resolve"),
        Some(paths.home_dir.join(".claude").join("settings.json"))
    );

    profile.app = "opencode".to_string();
    assert_eq!(
        native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Config)
            .expect("path should resolve"),
        Some(
            paths
                .home_dir
                .join(".config")
                .join("opencode")
                .join("opencode.json")
        )
    );
    assert_eq!(
        native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Gateway)
            .expect("gateway path should resolve"),
        Some(
            paths
                .home_dir
                .join(".config")
                .join("opencode")
                .join("opencode.json")
        )
    );

    profile.app = "hermes".to_string();
    assert_eq!(
        native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Config)
            .expect("path should resolve"),
        Some(paths.home_dir.join(".hermes").join("config.yaml"))
    );
    assert_eq!(
        native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Gateway)
            .expect("gateway path should resolve"),
        Some(paths.home_dir.join(".hermes").join("config.yaml"))
    );

    profile.app = "chatgpt-desktop".to_string();
    assert_eq!(
        native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Gateway)
            .expect("codex gateway path should resolve"),
        Some(paths.home_dir.join(".codex").join("config.toml"))
    );

    profile.app = "openclaw".to_string();
    assert_eq!(
        native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Gateway)
            .expect("gateway path should resolve"),
        Some(paths.home_dir.join(".openclaw").join("openclaw.json"))
    );

    profile.app = "claude-desktop".to_string();
    let claude_desktop_profile_path = claude_desktop_paths(&paths)
        .expect("claude desktop paths should resolve")
        .profile_path;
    assert_eq!(
        native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Config)
            .expect("claude desktop config path should resolve"),
        Some(claude_desktop_profile_path.clone())
    );
    assert_eq!(
        native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Gateway)
            .expect("claude desktop gateway path should resolve"),
        Some(claude_desktop_profile_path)
    );

    profile.app = "gemini-code-assist".to_string();
    assert_eq!(
        native_config_path_for_profile_mode(&profile, &paths, ProviderApplyMode::Gateway)
            .expect("unsupported gateway path should not resolve"),
        None
    );
}

#[test]
fn lifecycle_restore_targets_include_gateway_apps() {
    let mut active = ActiveProfilesByMode::default();
    active
        .config
        .insert("chatgpt-desktop".to_string(), "codex-config".to_string());
    active
        .gateway
        .insert("claude-vscode".to_string(), "claude-gateway".to_string());

    assert_eq!(
        lifecycle_target_apps(&active, ProviderApplyMode::Config, true),
        vec!["claude".to_string(), "codex".to_string()]
    );
    assert_eq!(
        lifecycle_target_apps(&active, ProviderApplyMode::Gateway, false),
        vec!["claude".to_string()]
    );
}

#[test]
fn chatgpt_desktop_active_profile_prefers_canonical_then_legacy_keys() {
    let mut active = HashMap::new();
    active.insert("codex-app".to_string(), "legacy-app".to_string());
    assert_eq!(
        active_profile_id_for_app(&active, "codex").map(String::as_str),
        Some("legacy-app")
    );

    active.insert(
        "chatgpt-desktop".to_string(),
        "chatgpt-desktop-profile".to_string(),
    );
    assert_eq!(
        active_profile_id_for_app(&active, "codex").map(String::as_str),
        Some("chatgpt-desktop-profile")
    );

    active.insert("codex".to_string(), "codex-profile".to_string());
    assert_eq!(
        active_profile_id_for_app(&active, "codex").map(String::as_str),
        Some("codex-profile")
    );
}

#[test]
fn official_cleanup_removes_only_gateway_fields() {
    let profile = test_profile("claude", ProviderApplyMode::Gateway);
    let gateway_content = claude_gateway_config_content(
        r#"{
  "env": {
    "OTHER_VALUE": "keep"
  }
}"#,
        &profile,
    )
    .expect("gateway config should render");
    let cleaned = claude_gateway_cleanup_config_content(&gateway_content, "claude")
        .expect("cleanup config should render");
    let value = parse_json5_or_empty(&cleaned, "Claude settings").expect("cleaned JSON");

    assert_eq!(
        json_string_lookup(&value, &["env", "OTHER_VALUE"]).as_deref(),
        Some("keep")
    );
    assert!(json_string_lookup(&value, &["env", "ANTHROPIC_BASE_URL"]).is_none());
    assert!(json_string_lookup(&value, &["env", "ANTHROPIC_AUTH_TOKEN"]).is_none());
    assert!(json_string_lookup(&value, &["env", "ANTHROPIC_MODEL"]).is_none());
    assert!(json_string_lookup(&value, &["model"]).is_none());

    let profile = test_profile("opencode", ProviderApplyMode::Gateway);
    let gateway_content =
        opencode_gateway_config_content("{}", &profile).expect("gateway config should render");
    let cleaned = opencode_gateway_cleanup_config_content(&gateway_content, "opencode")
        .expect("cleanup config should render");
    let value = parse_json5_or_empty(&cleaned, "OpenCode config").expect("cleaned JSON");

    assert!(json_lookup(&value, &["provider", "custom"]).is_none());
    assert!(json_string_lookup(&value, &["model"]).is_none());
}

#[test]
fn codex_native_previews_use_managed_actor_authorization_contract() {
    let paths = test_paths();
    let direct = test_profile("codex", ProviderApplyMode::Config);
    let direct_provider_id = codex_provider_id_for_profile(&direct);
    let gateway = test_profile("codex", ProviderApplyMode::Gateway);
    let official = builtin_official_profiles()
        .into_iter()
        .find(|profile| profile.app == "codex")
        .expect("codex official profile");
    let cases = [
        (direct, ProviderApplyMode::Config, direct_provider_id),
        (gateway, ProviderApplyMode::Gateway, "custom".to_string()),
        (official, ProviderApplyMode::Config, "openai".to_string()),
    ];

    for (profile, mode, provider_id) in cases {
        let preview = build_native_config_preview(&profile, None, &paths, mode)
            .expect("preview should build")
            .expect("Codex preview should be available");
        let requires_key = format!("model_providers.{provider_id}.requires_openai_auth");
        let headers_key = format!("model_providers.{provider_id}.http_headers");
        let requires_index = preview
            .changes
            .iter()
            .position(|change| change.key == requires_key)
            .expect("auth requirement diff should exist");
        let headers_index = preview
            .changes
            .iter()
            .position(|change| change.key == headers_key)
            .expect("actor header diff should exist");

        assert_eq!(headers_index, requires_index + 1);
        assert_eq!(
            preview.changes[requires_index].after.as_deref(),
            Some("false")
        );
        assert_eq!(
            preview.changes[headers_index].after.as_deref(),
            Some(CODEX_ACTOR_AUTHORIZATION_INLINE_TOML)
        );
    }
}

#[test]
fn codex_native_previews_include_review_model_override_and_primary_fallback() {
    let paths = test_paths();
    let config_path = paths.home_dir.join(".codex").join("config.toml");
    write_native_config(&config_path, "review_model = \"old-review\"\n")
        .expect("existing config should write");
    let mut direct = test_profile("codex", ProviderApplyMode::Config);
    direct.provider = "compatible".to_string();
    direct.protocol = PROTOCOL_OPENAI_RESPONSES.to_string();
    let gateway = test_profile("codex", ProviderApplyMode::Gateway);
    let official = builtin_official_profiles()
        .into_iter()
        .find(|profile| profile.app == "codex")
        .expect("codex official profile");

    for (mut profile, mode) in [
        (direct, ProviderApplyMode::Config),
        (gateway, ProviderApplyMode::Gateway),
        (official, ProviderApplyMode::Config),
    ] {
        profile.model = "gpt-5.5".to_string();
        profile.review_model = Some("gpt-5.6-review".to_string());
        let preview = build_native_config_preview(&profile, None, &paths, mode.clone())
            .expect("preview should build")
            .expect("Codex preview should be available");
        let change = preview
            .changes
            .iter()
            .find(|change| change.key == "review_model")
            .expect("review model set diff should exist");
        assert_eq!(change.after.as_deref(), Some("gpt-5.6-review"));

        profile.review_model = None;
        let preview = build_native_config_preview(&profile, None, &paths, mode)
            .expect("preview should build")
            .expect("Codex preview should be available");
        let change = preview
            .changes
            .iter()
            .find(|change| change.key == "review_model")
            .expect("review model fallback diff should exist");
        assert_eq!(change.action, "update");
        assert_eq!(change.after.as_deref(), Some("gpt-5.5"));
    }
}

#[test]
fn codex_config_verification_requires_the_review_model_to_match() {
    let paths = test_paths();
    let config_path = paths.home_dir.join(".codex").join("config.toml");

    let mut direct = test_profile("codex", ProviderApplyMode::Config);
    direct.provider = "compatible".to_string();
    direct.protocol = PROTOCOL_OPENAI_RESPONSES.to_string();
    direct.review_model = Some("gpt-5.6-review".to_string());
    let direct_content =
        codex_direct_config_content("", &direct).expect("direct config should render");
    write_native_config(&config_path, &direct_content).expect("direct config should write");
    assert!(verify_codex_direct_config(&config_path, &direct).expect("direct config should verify"));
    write_native_config(
        &config_path,
        &direct_content.replace("gpt-5.6-review", "gpt-5.6-review-other"),
    )
    .expect("mismatched direct config should write");
    assert!(!verify_codex_direct_config(&config_path, &direct)
        .expect("mismatched direct config should be readable"));

    let mut gateway = test_profile("codex", ProviderApplyMode::Gateway);
    gateway.review_model = Some("gpt-5.6-review".to_string());
    let gateway_content =
        codex_gateway_config_content("", &gateway).expect("gateway config should render");
    write_native_config(&config_path, &gateway_content).expect("gateway config should write");
    assert!(
        verify_codex_native_config(&config_path, &gateway).expect("gateway config should verify")
    );
    write_native_config(
        &config_path,
        &gateway_content.replace("gpt-5.6-review", "gpt-5.6-review-other"),
    )
    .expect("mismatched gateway config should write");
    assert!(!verify_codex_native_config(&config_path, &gateway)
        .expect("mismatched gateway config should be readable"));
}

#[test]
fn non_codex_native_preview_includes_redacted_content() {
    let paths = test_paths();
    let profile = test_profile("claude", ProviderApplyMode::Gateway);
    let preview = build_native_config_preview(&profile, None, &paths, ProviderApplyMode::Gateway)
        .expect("preview should build");
    let preview =
        attach_native_config_content_preview(preview, &profile, &paths, ProviderApplyMode::Gateway)
            .expect("preview should be available");
    let content = preview.content.expect("content preview should be included");

    assert!(content.contains("ANTHROPIC_BASE_URL"));
    assert!(content.contains("<redacted>"));
    assert!(!content.contains("codestudio-local-test"));
}

#[test]
fn non_codex_config_preview_includes_placeholder_content_without_keychain_secret() {
    let paths = test_paths();
    let mut profile = test_profile("claude", ProviderApplyMode::Config);
    profile.protocol = PROTOCOL_ANTHROPIC_MESSAGES.to_string();
    let preview = build_native_config_preview(&profile, None, &paths, ProviderApplyMode::Config)
        .expect("preview should build");
    let preview =
        attach_native_config_content_preview(preview, &profile, &paths, ProviderApplyMode::Config)
            .expect("preview should be available");
    let content = preview.content.expect("content preview should be included");

    assert!(content.contains("ANTHROPIC_BASE_URL"));
    assert!(content.contains("keychain:****"));
}

#[test]
fn official_claude_config_preview_includes_restore_content() {
    let paths = test_paths();
    let settings_path = paths.home_dir.join(".claude").join("settings.json");
    fs::create_dir_all(settings_path.parent().expect("settings parent"))
        .expect("settings parent should be created");
    fs::write(
        &settings_path,
        r#"{
  "env": {
    "ANTHROPIC_BASE_URL": "https://example.test/v1",
    "ANTHROPIC_AUTH_TOKEN": "sk-test",
    "ANTHROPIC_MODEL": "claude-test",
    "OTHER_VALUE": "keep"
  },
  "model": "claude-test"
}"#,
    )
    .expect("settings should be written");
    let mut profile = test_profile("claude", ProviderApplyMode::Config);
    profile.provider = "official".to_string();
    profile.auth_ref = None;
    profile.protocol = PROTOCOL_ANTHROPIC_MESSAGES.to_string();
    let preview = build_native_config_preview(&profile, None, &paths, ProviderApplyMode::Config)
        .expect("preview should build");
    let preview =
        attach_native_config_content_preview(preview, &profile, &paths, ProviderApplyMode::Config)
            .expect("preview should be available");
    assert!(preview
        .changes
        .iter()
        .any(|change| { change.key == "env.ANTHROPIC_BASE_URL" && change.action == "remove" }));
    let content = preview.content.expect("content preview should be included");

    assert!(content.contains("OTHER_VALUE"));
    assert!(!content.contains("ANTHROPIC_BASE_URL"));
    assert!(!content.contains("ANTHROPIC_AUTH_TOKEN"));
    assert!(!content.contains("keychain:****"));
}

#[test]
fn codex_direct_apply_plan_writes_auth_json_before_config() {
    let paths = test_paths();
    let auth_path = codex_auth_json_path(&paths);
    fs::create_dir_all(auth_path.parent().expect("auth parent"))
        .expect("auth parent should be created");
    fs::write(
        &auth_path,
        r#"{
  "auth_mode": "chatgpt",
  "tokens": {
    "access_token": "oauth-access",
    "refresh_token": "oauth-refresh"
  }
}"#,
    )
    .expect("existing auth should be written");

    let mut profile = test_profile("codex", ProviderApplyMode::Config);
    profile.provider = "compatible".to_string();
    profile.protocol = PROTOCOL_OPENAI_RESPONSES.to_string();
    profile.model = "gpt-5.5".to_string();
    profile.base_url = "https://api.apikey.fun/v1".to_string();
    profile.auth_ref = Some(format!(
        "keychain:test/codex-direct-auth-plan-{}/api_key",
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    store_test_profile_secret(&profile, "sk-direct-profile");

    let plans = build_native_apply_plan(&profile, &paths, &ProviderApplyMode::Config, false)
        .expect("direct plan should build");
    assert_eq!(plans.len(), 2);
    assert!(matches!(
        plans[0].kind,
        NativeConfigWriteKind::CodexAuthJson
    ));
    assert_eq!(plans[0].path, auth_path);
    assert!(matches!(
        plans[1].kind,
        NativeConfigWriteKind::ProfileConfig
    ));

    let auth: serde_json::Value =
        serde_json::from_str(&plans[0].content).expect("planned auth should parse");
    assert_eq!(
        auth.get("auth_mode").and_then(serde_json::Value::as_str),
        Some("apikey")
    );
    assert_eq!(
        auth.get("experimental_bearer_token")
            .and_then(serde_json::Value::as_str),
        Some("sk-direct-profile")
    );
    assert!(auth.get("OPENAI_API_KEY").is_none());
    assert_eq!(
        auth.pointer("/tokens/refresh_token")
            .and_then(serde_json::Value::as_str),
        Some("oauth-refresh")
    );

    let config: toml::Value =
        toml::from_str(&plans[1].content).expect("planned config should parse");
    assert_eq!(
        read_toml_string(&config, "cli_auth_credentials_store").as_deref(),
        Some("file")
    );
    assert_codex_managed_provider_contract(&config, "custom");
    assert_codex_managed_provider_contract_lines(&plans[1].content);
    assert!(!plans[1].content.contains("sk-direct-profile"));

    apply_native_config_write_plan(&plans[0]).expect("auth plan should apply");
    assert!(
        verify_native_config_write(&plans[0], &profile, &ProviderApplyMode::Config)
            .expect("auth plan should verify")
    );
    fs::write(&plans[0].path, "{}\n").expect("auth plan should be tampered");
    assert!(
        !verify_native_config_write(&plans[0], &profile, &ProviderApplyMode::Config)
            .expect("tampered auth plan should be rejected")
    );

    apply_native_config_write_plan(&plans[1]).expect("config plan should apply");
    assert!(
        verify_native_config_write(&plans[1], &profile, &ProviderApplyMode::Config)
            .expect("config plan should verify")
    );
    fs::write(
        &plans[1].path,
        plans[1]
            .content
            .replace(CODEX_ACTOR_AUTHORIZATION_VALUE, "unexpected-actor"),
    )
    .expect("config header should be tampered");
    assert!(
        !verify_native_config_write(&plans[1], &profile, &ProviderApplyMode::Config)
            .expect("tampered config header should be rejected")
    );
}

#[test]
fn codex_gateway_apply_plan_writes_local_token_to_auth_json_before_config() {
    let paths = test_paths();
    let auth_path = codex_auth_json_path(&paths);
    fs::create_dir_all(auth_path.parent().expect("auth parent"))
        .expect("auth parent should be created");
    fs::write(
        &auth_path,
        r#"{
  "auth_mode": "chatgpt",
  "tokens": {
    "access_token": "oauth-access",
    "refresh_token": "oauth-refresh"
  }
}"#,
    )
    .expect("existing auth should be written");

    let profile = test_profile("codex", ProviderApplyMode::Gateway);
    let client = gateway::client_config_for_tool("codex").expect("gateway client config");
    let plans = build_native_apply_plan(&profile, &paths, &ProviderApplyMode::Gateway, false)
        .expect("gateway plan should build");
    assert_eq!(plans.len(), 2);
    assert!(matches!(
        plans[0].kind,
        NativeConfigWriteKind::CodexAuthJson
    ));
    assert_eq!(plans[0].path, auth_path);
    assert!(matches!(
        plans[1].kind,
        NativeConfigWriteKind::ProfileConfig
    ));

    let auth: serde_json::Value =
        serde_json::from_str(&plans[0].content).expect("planned auth should parse");
    assert_eq!(
        auth.get("auth_mode").and_then(serde_json::Value::as_str),
        Some("apikey")
    );
    assert_eq!(
        auth.get("experimental_bearer_token")
            .and_then(serde_json::Value::as_str),
        Some(client.token.as_str())
    );
    assert!(auth.get("OPENAI_API_KEY").is_none());
    assert_eq!(
        auth.pointer("/tokens/access_token")
            .and_then(serde_json::Value::as_str),
        Some("oauth-access")
    );

    let config: toml::Value =
        toml::from_str(&plans[1].content).expect("planned config should parse");
    assert_eq!(
        read_toml_string(&config, "cli_auth_credentials_store").as_deref(),
        Some("file")
    );
    assert_codex_managed_provider_contract(&config, "custom");
    assert_codex_managed_provider_contract_lines(&plans[1].content);
    assert!(!plans[1].content.contains(&client.token));

    apply_native_config_write_plan(&plans[1]).expect("config plan should apply");
    assert!(
        verify_native_config_write(&plans[1], &profile, &ProviderApplyMode::Gateway)
            .expect("config plan should verify")
    );
    fs::write(
        &plans[1].path,
        plans[1]
            .content
            .replace(CODEX_ACTOR_AUTHORIZATION_VALUE, "unexpected-actor"),
    )
    .expect("config header should be tampered");
    assert!(
        !verify_native_config_write(&plans[1], &profile, &ProviderApplyMode::Gateway)
            .expect("tampered config header should be rejected")
    );
}

#[test]
fn json5_preview_parser_accepts_comments() {
    let value = parse_json5_or_empty(
        r#"
            {
              // comment from a JSONC-style config
              provider: {
                codestudio_openai: {
                  options: {
                    baseURL: "https://example.test/v1",
                  },
                },
              },
            }
            "#,
        "test config",
    )
    .expect("json5 should parse");

    assert_eq!(
        json_string_lookup(
            &value,
            &["provider", "codestudio_openai", "options", "baseURL"]
        )
        .as_deref(),
        Some("https://example.test/v1")
    );
}

#[test]
fn legacy_protocol_alias_is_rejected() {
    assert!(normalize_protocol(Some("openai-compatible")).is_err());
    assert!(normalize_protocol(Some("claude-messages")).is_err());
    assert!(normalize_protocol(None).is_err());
    assert_eq!(
        normalize_protocol(Some("openai-responses")).as_deref(),
        Ok(PROTOCOL_OPENAI_RESPONSES)
    );
    assert_eq!(
        normalize_protocol(Some("anthropic-messages")).as_deref(),
        Ok(PROTOCOL_ANTHROPIC_MESSAGES)
    );
}

#[test]
fn builtin_official_profiles_use_tool_native_protocols() {
    let profiles = builtin_official_profiles();

    assert_eq!(
        profiles
            .iter()
            .find(|profile| profile.app == "codex")
            .map(|profile| profile.protocol.as_str()),
        Some(PROTOCOL_OPENAI_RESPONSES)
    );
    assert_eq!(
        profiles
            .iter()
            .find(|profile| profile.app == "claude")
            .map(|profile| profile.protocol.as_str()),
        Some(PROTOCOL_ANTHROPIC_MESSAGES)
    );
    assert_eq!(
        profiles
            .iter()
            .find(|profile| profile.app == "claude-desktop")
            .map(|profile| profile.protocol.as_str()),
        Some(PROTOCOL_ANTHROPIC_MESSAGES)
    );
    assert_eq!(
        profiles
            .iter()
            .any(|profile| profile.app == "claude-vscode"),
        false
    );
    assert_eq!(
        profiles
            .iter()
            .find(|profile| profile.app == "gemini-code-assist")
            .map(|profile| profile.protocol.as_str()),
        Some(PROTOCOL_GOOGLE_GEMINI)
    );
    assert_eq!(
        profiles
            .iter()
            .find(|profile| profile.app == "openclaw")
            .map(|profile| profile.protocol.as_str()),
        Some(PROTOCOL_OPENAI_CHAT_COMPLETIONS)
    );
    assert_eq!(
        profiles
            .iter()
            .find(|profile| profile.app == "hermes")
            .map(|profile| profile.protocol.as_str()),
        Some(PROTOCOL_OPENAI_CHAT_COMPLETIONS)
    );
}

#[test]
fn claude_desktop_restart_uses_packaged_app_fallback() {
    let targets = restart_targets_for_app("claude-desktop", RestartContext::default());

    assert_eq!(targets.len(), 1);
    if cfg!(target_os = "windows") {
        assert!(matches!(
            targets[0].launch,
            RestartLaunch::MsixPackage {
                package_identities: &["Claude", "Anthropic.Claude"]
            }
        ));
    } else {
        assert!(matches!(
            targets[0].launch,
            RestartLaunch::ExistingProcessPath {
                fallback_command: "Claude",
                hidden: false
            }
        ));
    }
}

#[test]
fn codex_restart_targets_cover_client_cli_and_vscode_backend() {
    let targets = restart_targets_for_app("codex", RestartContext::default());

    assert_eq!(targets.len(), 3);
    assert_eq!(targets[0].label, "Codex");
    assert!(matches!(targets[0].launch, RestartLaunch::ChatGptDesktop));
    assert!(targets[0].process_names.contains(&"ChatGPT.exe"));
    assert!(targets[0].process_names.contains(&"Codex.exe"));
    assert_eq!(targets[1].label, "Codex VS Code extension backend");
    assert!(matches!(targets[1].launch, RestartLaunch::CloseOnly));
    assert!(targets[1]
        .command_markers
        .iter()
        .any(|marker| marker.contains("openai.chatgpt")));
    assert_eq!(targets[2].label, "Codex CLI");
    assert!(matches!(
        targets[2].launch,
        RestartLaunch::Command {
            command: "codex",
            hidden: true
        }
    ));
}

#[test]
fn windows_restart_script_falls_back_only_for_safe_name_targets() {
    let codex_targets = restart_targets_for_app("codex", RestartContext::default());
    let desktop_script = windows_restart_process_script(codex_targets[0]);
    let cli_script = windows_restart_process_script(codex_targets[2]);

    assert!(desktop_script.contains("Get-CimInstance Win32_Process -ErrorAction Stop"));
    assert!(desktop_script.contains("Get-Process -Name $clean -ErrorAction SilentlyContinue"));
    assert!(desktop_script.contains("$ExcludeMarkers.Count -eq 0"));
    assert!(!codex_targets[2].exclude_command_markers.is_empty());
    assert!(cli_script.contains("$ExcludeMarkers = @("));
}

#[cfg(windows)]
#[test]
fn windows_restart_script_safely_returns_no_match_when_cim_is_unavailable() {
    const NAMES: &[&str] = &["codestudio-restart-test-no-match.exe"];
    const EMPTY: &[&str] = &[];
    let target = RestartTarget {
        label: "CodeStudio restart test",
        process_names: NAMES,
        command_markers: EMPTY,
        exclude_command_markers: EMPTY,
        require_window: false,
        reject_window: false,
        launch: RestartLaunch::CloseOnly,
    };

    let json = run_powershell(&windows_restart_process_script(target)).unwrap();
    let result: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(
        result.get("total").and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert_eq!(
        result.get("remaining").and_then(serde_json::Value::as_u64),
        Some(0)
    );
}

#[cfg(windows)]
#[test]
fn windows_restart_script_falls_back_to_process_tree_termination() {
    use std::fs;
    use std::process::{Command, Stdio};
    use std::time::{SystemTime, UNIX_EPOCH};

    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let process_name = format!("csl-restart-{suffix}.exe");
    let executable = std::env::temp_dir().join(&process_name);
    fs::copy(r"C:\Windows\System32\ping.exe", &executable).unwrap();
    let mut child = Command::new(&executable)
        .args(["127.0.0.1", "-n", "60"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let leaked_name: &'static str = Box::leak(process_name.into_boxed_str());
    let leaked_names: &'static [&'static str] = Box::leak(vec![leaked_name].into_boxed_slice());
    const EMPTY: &[&str] = &[];
    let target = RestartTarget {
        label: "CodeStudio restart fallback test",
        process_names: leaked_names,
        command_markers: EMPTY,
        exclude_command_markers: EMPTY,
        require_window: false,
        reject_window: false,
        launch: RestartLaunch::CloseOnly,
    };
    let script = format!(
        "function Stop-Process {{ throw 'simulated primary termination failure' }}\n{}",
        windows_restart_process_script(target)
    );

    let result = run_powershell(&script).and_then(|json| {
        serde_json::from_str::<serde_json::Value>(&json).map_err(|err| err.to_string())
    });
    let _ = child.kill();
    let _ = child.wait();
    let _ = fs::remove_file(&executable);

    let result = result.unwrap();
    assert_eq!(
        result.get("total").and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        result.get("remaining").and_then(serde_json::Value::as_u64),
        Some(0)
    );
}

#[test]
fn claude_restart_targets_only_include_vscode_backend_when_synced() {
    let base_targets = restart_targets_for_app("claude", RestartContext::default());
    assert_eq!(base_targets.len(), 1);
    assert_eq!(base_targets[0].label, "Claude Code");

    let synced_targets = restart_targets_for_app(
        "claude",
        RestartContext {
            sync_claude_vs_code: true,
        },
    );
    assert_eq!(synced_targets.len(), 2);
    assert_eq!(synced_targets[0].label, "Claude Code");
    assert_eq!(synced_targets[1].label, "Claude VS Code extension backend");
    assert!(matches!(synced_targets[1].launch, RestartLaunch::CloseOnly));
    assert!(synced_targets[1]
        .command_markers
        .iter()
        .any(|marker| marker.contains("anthropic.claude-code")));
}

#[test]
fn custom_codex_oauth_profile_write_plan_is_allowed_without_api_key() {
    let plan = build_profile_write_plan(
        "Codex OAuth Test",
        "codex",
        Some(&ProviderApplyMode::Config),
        "official",
        Some(PROTOCOL_OPENAI_RESPONSES),
        "",
        "",
        false,
    )
    .expect("codex oauth profile should be allowed");

    assert_eq!(plan.app, "codex");
    assert_eq!(plan.provider, "official");
    assert_eq!(plan.mode, ProviderApplyMode::Config);
    assert_eq!(plan.secret_status, "oauth");
    assert!(plan.auth_ref.is_none());
}

#[test]
fn custom_official_profile_write_plan_rejects_non_codex_tools() {
    let result = build_profile_write_plan(
        "Claude Official Copy",
        "claude",
        Some(&ProviderApplyMode::Config),
        "official",
        Some(PROTOCOL_ANTHROPIC_MESSAGES),
        "",
        "",
        false,
    );
    let error = match result {
        Ok(_) => panic!("non-codex official profiles should remain built-in only"),
        Err(error) => error,
    };

    assert_eq!(
        error,
        "Only Codex OAuth profiles can be saved as custom official profiles."
    );
}

#[test]
fn profile_icon_normalization_accepts_short_text_and_image_data() {
    assert_eq!(
        normalize_profile_icon(Some(" API ")).expect("short icon should be accepted"),
        Some("API".to_string())
    );
    assert_eq!(
        normalize_profile_icon(Some(" data:image/png;base64,abcd "))
            .expect("image data url should be accepted"),
        Some("data:image/png;base64,abcd".to_string())
    );
    assert_eq!(
        normalize_profile_icon(Some("")).expect("blank icon should clear"),
        None
    );
    assert!(normalize_profile_icon(Some("TOO-LONG")).is_err());
}

#[test]
fn replace_deleted_active_profile_uses_official_for_config_mode() {
    let mut config = test_app_config();
    config
        .active_profiles_by_mode
        .config
        .insert("codex".to_string(), "delete-me".to_string());
    config
        .active_profiles_by_mode
        .gateway
        .insert("codex".to_string(), "delete-me".to_string());
    config
        .active_profiles_by_mode
        .gateway
        .insert("hermes".to_string(), "keep-me".to_string());

    assert!(replace_deleted_active_profile_with_official(
        &mut config,
        "codex",
        "delete-me"
    ));
    assert_eq!(
        config.active_profiles_by_mode.config.get("codex"),
        Some(&builtin_official_profile_id("codex"))
    );
    assert!(!config.active_profiles_by_mode.gateway.contains_key("codex"));
    assert_eq!(
        config.active_profiles_by_mode.gateway.get("hermes"),
        Some(&"keep-me".to_string())
    );
}

#[test]
fn claude_vscode_alias_uses_claude_profile_category() {
    assert_eq!(canonical_profile_app("claude-vscode"), "claude");
    assert_eq!(canonical_profile_app("claude-code-vscode"), "claude");
    assert_eq!(
        builtin_official_profile_id("claude-vscode"),
        "builtin-official-claude"
    );
}

#[test]
fn claude_gateway_config_model_uses_first_mapping_alias() {
    let mut profile = test_profile("claude", ProviderApplyMode::Gateway);
    profile.model = "provider-default".to_string();
    profile.model_mappings = vec![ProfileModelMapping {
        alias: "claude-sonnet-4-6".to_string(),
        model: "provider-sonnet".to_string(),
        supports_1m: true,
        description: None,
    }];

    assert_eq!(
        gateway_config_model_for_profile(&profile),
        "claude-sonnet-4-6"
    );
}

fn test_profile(app: &str, mode: ProviderApplyMode) -> ProfileDraft {
    ProfileDraft {
        id: format!("{app}-custom"),
        name: "Custom".to_string(),
        icon: None,
        remark: None,
        app: app.to_string(),
        is_builtin: false,
        mode,
        provider: "openai".to_string(),
        protocol: PROTOCOL_OPENAI_CHAT_COMPLETIONS.to_string(),
        model: String::new(),
        review_model: None,
        model_mappings: Vec::new(),
        base_url: "https://example.test/v1".to_string(),
        auth_ref: Some(format!("keychain:test/{app}/api_key")),
        created_at: None,
        updated_at: None,
        last_test_status: None,
        usage_enabled: false,
        sort_order: 0,
    }
}

fn store_test_profile_secret(profile: &ProfileDraft, secret: &str) {
    let auth_ref = profile.auth_ref.as_deref().expect("auth ref");
    credentials::store_keychain_secret(auth_ref, secret).expect("test key should store");
}

fn test_paths() -> crate::core::app_paths::AppPaths {
    let root = env::temp_dir().join(format!(
        "codestudio-lite-profile-test-{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    crate::core::app_paths::AppPaths {
        home_dir: root.clone(),
        config_dir: root.join(".codestudio-lite"),
        downloads_dir: root.join(".codestudio-lite").join("downloads"),
        database_file: root.join(".codestudio-lite").join("app_state.sqlite"),
    }
}
