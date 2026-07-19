use super::*;
use reqwest::StatusCode;
use std::io::Write;

fn installed(source: &str) -> InstalledChatGptDesktop {
    InstalledChatGptDesktop {
        path: "C:\\Program Files\\WindowsApps\\OpenAI.Codex".to_string(),
        version: "1.0.0.0".to_string(),
        arch: None,
        source: source.to_string(),
        generation: ChatGptDesktopProductGeneration::Current,
        package_family_name: if source == "msix" {
            Some("OpenAI.Codex_abc".to_string())
        } else {
            None
        },
        installed_at: None,
    }
}

#[test]
fn windows_generation_prefers_chatgpt_executable_and_defaults_unknown_to_current() {
    let root = std::env::temp_dir().join(format!(
        "codestudio-lite-chatgpt-generation-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("app")).unwrap();
    std::fs::write(root.join("Codex.exe"), []).unwrap();

    assert_eq!(
        chatgpt_desktop_generation_from_windows_root(&root),
        ChatGptDesktopProductGeneration::Legacy
    );

    std::fs::write(root.join("app").join("ChatGPT.exe"), []).unwrap();
    assert_eq!(
        chatgpt_desktop_generation_from_windows_root(&root),
        ChatGptDesktopProductGeneration::Current
    );

    let unknown = root.join("unknown");
    std::fs::create_dir_all(&unknown).unwrap();
    assert_eq!(
        chatgpt_desktop_generation_from_windows_root(&unknown),
        ChatGptDesktopProductGeneration::Current
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn macos_generation_uses_bundle_executable_before_app_name_fallback() {
    assert_eq!(
        chatgpt_desktop_generation_from_macos_identity(
            Some("ChatGPT"),
            std::path::Path::new("/Applications/Codex.app")
        ),
        ChatGptDesktopProductGeneration::Current
    );
    assert_eq!(
        chatgpt_desktop_generation_from_macos_identity(
            Some("Codex"),
            std::path::Path::new("/Applications/ChatGPT.app")
        ),
        ChatGptDesktopProductGeneration::Legacy
    );
    assert_eq!(
        chatgpt_desktop_generation_from_macos_identity(
            None,
            std::path::Path::new("/Applications/OpenAI Codex.app")
        ),
        ChatGptDesktopProductGeneration::Legacy
    );
    assert_eq!(
        chatgpt_desktop_generation_from_macos_identity(
            None,
            std::path::Path::new("/Applications/Unknown.app")
        ),
        ChatGptDesktopProductGeneration::Current
    );
}

#[test]
fn macos_download_architecture_prefers_native_apple_silicon_over_rosetta_process_architecture() {
    assert_eq!(macos_arch_for_runtime("aarch64", false).unwrap(), "arm64");
    assert_eq!(macos_arch_for_runtime("x86_64", true).unwrap(), "arm64");
    assert_eq!(macos_arch_for_runtime("x86_64", false).unwrap(), "x64");
    assert!(macos_arch_for_runtime("powerpc", false).is_err());

    let macos = MacosSources {
        arm64: Some(MacosSource {
            url: Some("https://example.test/ChatGPT-arm64.dmg".to_string()),
            content_length: None,
            etag: None,
            sha256: None,
            bundle_short_version: None,
            bundle_version: None,
            bundle_identifier: None,
        }),
        x64: Some(MacosSource {
            url: Some("https://example.test/ChatGPT-x64.dmg".to_string()),
            content_length: None,
            etag: None,
            sha256: None,
            bundle_short_version: None,
            bundle_version: None,
            bundle_identifier: None,
        }),
    };

    let (arm64, arm64_label) = macos_source_for_arch(&macos, "arm64").unwrap();
    let (x64, x64_label) = macos_source_for_arch(&macos, "x64").unwrap();
    assert_eq!(arm64_label, "arm64");
    assert_eq!(
        arm64.url.as_deref(),
        Some("https://example.test/ChatGPT-arm64.dmg")
    );
    assert_eq!(x64_label, "x64");
    assert_eq!(
        x64.url.as_deref(),
        Some("https://example.test/ChatGPT-x64.dmg")
    );
}

#[test]
fn windows_download_architecture_selects_the_native_manifest_entry() {
    let windows: WindowsSource = serde_json::from_value(serde_json::json!({
        "version": "26.707.12708.0",
        "packageMoniker": "OpenAI.Codex_26.707.12708.0_x64__publisher",
        "architecture": "x64",
        "contentLength": 728695101,
        "architectures": {
            "arm64": {
                "version": "26.707.12708.0",
                "packageMoniker": "OpenAI.Codex_26.707.12708.0_arm64__publisher",
                "architecture": "arm64",
                "contentLength": 724464899
            },
            "x64": {
                "version": "26.707.12708.0",
                "packageMoniker": "OpenAI.Codex_26.707.12708.0_x64__publisher",
                "architecture": "x64",
                "contentLength": 728695101
            }
        }
    }))
    .unwrap();

    let arm64 = windows_source_for_arch(&windows, "arm64").unwrap();
    let x64 = windows_source_for_arch(&windows, "x64").unwrap();
    assert_eq!(
        arm64.package_moniker,
        "OpenAI.Codex_26.707.12708.0_arm64__publisher"
    );
    assert_eq!(arm64.content_length, Some(724464899));
    assert_eq!(
        x64.package_moniker,
        "OpenAI.Codex_26.707.12708.0_x64__publisher"
    );
    assert_eq!(
        windows_package_url("https://mirror.test", "arm64"),
        "https://mirror.test/latest/win-arm64"
    );
    assert_eq!(
        windows_package_url("https://mirror.test", "x64"),
        "https://mirror.test/latest/win"
    );
}

#[test]
fn cached_desktop_generation_defaults_to_current_when_legacy_cache_omits_it() {
    let installed: InstalledChatGptDesktop = serde_json::from_value(serde_json::json!({
        "path": "C:\\Program Files\\WindowsApps\\OpenAI.Codex",
        "version": "1.0.0.0",
        "arch": null,
        "source": "msix",
        "packageFamilyName": "OpenAI.Codex_abc",
        "installedAt": null
    }))
    .unwrap();
    assert_eq!(
        installed.generation,
        ChatGptDesktopProductGeneration::Current
    );

    let snapshot: crate::core::types::DetectionSnapshot =
        serde_json::from_value(serde_json::json!({
            "generatedAt": "2026-07-10T00:00:00Z",
            "source": "cached",
            "platform": "windows",
            "homeDir": "C:\\Users\\test",
            "appConfigDir": "C:\\Users\\test\\.codestudio-lite",
            "activeProfile": null,
            "activeProfileName": null,
            "codexAuth": {
                "available": false,
                "method": "none",
                "storage": "none",
                "path": null,
                "detail": ""
            },
            "tools": [],
            "system": [],
            "problems": []
        }))
        .unwrap();
    assert_eq!(
        snapshot.chatgpt_desktop_product_generation,
        ChatGptDesktopProductGeneration::Current
    );
}

#[test]
fn removed_gpt56_launch_setting_is_ignored_for_legacy_settings() {
    let defaults = ChatGptDesktopSettings::default();
    let mut value = serde_json::to_value(defaults).unwrap();
    value.as_object_mut().unwrap().insert(
        "gpt56OfficialEntryOnLaunch".to_string(),
        serde_json::Value::Bool(true),
    );
    let restored: ChatGptDesktopSettings = serde_json::from_value(value).unwrap();
    let serialized = serde_json::to_value(restored).unwrap();
    assert!(!serialized
        .as_object()
        .unwrap()
        .contains_key("gpt56OfficialEntryOnLaunch"));
}

#[test]
fn existing_msix_update_keeps_msix_route() {
    let mut settings = ChatGptDesktopSettings::default();
    settings.windows_install_mode = "portable".to_string();
    let installed = installed("msix");

    assert_eq!(
        select_install_route(&settings, Some(&installed)),
        "msix-sideload"
    );
}

#[test]
fn existing_portable_update_keeps_portable_route() {
    let settings = ChatGptDesktopSettings::default();
    let installed = installed("portable");

    assert_eq!(
        select_install_route(&settings, Some(&installed)),
        "portable-fallback"
    );
}

#[test]
fn default_windows_install_stays_msix_without_capability_fallback() {
    let settings = ChatGptDesktopSettings::default();

    assert_eq!(select_install_route(&settings, None), "msix-sideload");
}

#[test]
fn latest_version_cache_failure_preserves_previous_version() {
    let mut cache = ChatGptDesktopLatestCache {
        version: Some("26.616.9593".to_string()),
        checked_at: Some(
            Instant::now() - CHATGPT_DESKTOP_LATEST_CACHE_TTL - Duration::from_secs(1),
        ),
        in_progress: true,
    };
    let checked_at = cache.checked_at;

    finish_latest_cache(&mut cache, None);

    assert_eq!(cache.version.as_deref(), Some("26.616.9593"));
    assert_eq!(cache.checked_at, checked_at);
    assert!(!cache.in_progress);
}

#[test]
fn latest_version_cache_success_updates_timestamp_and_value() {
    let mut cache = ChatGptDesktopLatestCache {
        version: Some("26.616.9593".to_string()),
        checked_at: None,
        in_progress: true,
    };

    finish_latest_cache(&mut cache, Some("26.630.12135".to_string()));

    assert_eq!(cache.version.as_deref(), Some("26.630.12135"));
    assert!(cache.checked_at.is_some());
    assert!(!cache.in_progress);
}

#[test]
fn windows_checksum_matches_package_moniker_not_first_msix() {
    let checksums = "\
744d1b7500ae59ddb24c60aeaa9a861b33aaf0a72fa4f90f5a26e3017d6cd408  OpenAI.Codex_26.616.9593.0_arm64__2p2nqsd0c76g0.Msix
d0d57caccde4e95b6326c8ab8f5ebb610cbbaff4f80197d34e586432d57ad84d  OpenAI.Codex_26.616.9593.0_x64__2p2nqsd0c76g0.Msix
";

    assert_eq!(
        checksum_for_windows(checksums, "OpenAI.Codex_26.616.9593.0_x64__2p2nqsd0c76g0").as_deref(),
        Some("d0d57caccde4e95b6326c8ab8f5ebb610cbbaff4f80197d34e586432d57ad84d")
    );
}

#[test]
fn stale_staged_package_falls_back_when_canonical_path_is_not_removable() {
    let root = std::env::temp_dir().join(format!(
        "codestudio-lite-chatgpt-desktop-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let canonical = root.join("OpenAI.Codex_26.616.9593.0_x64__2p2nqsd0c76g0.Msix");
    fs::create_dir(&canonical).unwrap();

    let target = staged_package_target(
        &canonical,
        "d0d57caccde4e95b6326c8ab8f5ebb610cbbaff4f80197d34e586432d57ad84d",
    )
    .unwrap();
    let StagedPackageTarget::Download(fallback) = target else {
        panic!("expected a fallback download target");
    };

    assert_ne!(fallback, canonical);
    assert_eq!(
        fallback.file_name().and_then(|name| name.to_str()),
        Some("OpenAI.Codex_26.616.9593.0_x64__2p2nqsd0c76g0-d0d57cac.Msix")
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn download_temp_path_is_unique_for_the_same_target() {
    let target = PathBuf::from(r"C:\Temp\OpenAI.Codex_26.616.9593.0_x64__2p2nqsd0c76g0.Msix");

    let first = download_temp_path(&target);
    let second = download_temp_path(&target);

    assert_ne!(first, target);
    assert_ne!(first, second);
    assert_eq!(first.parent(), target.parent());
    assert!(first
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(
            |name| name.starts_with("OpenAI.Codex_26.616.9593.0_x64__2p2nqsd0c76g0.Msix.download.")
        ));
}

#[test]
fn mirror_retry_policy_is_bounded_and_only_retries_transient_http_statuses() {
    assert_eq!(download_http::DOWNLOAD_HTTP_MAX_ATTEMPTS, 4);
    assert!(download_http::download_http_should_retry_status(
        StatusCode::REQUEST_TIMEOUT
    ));
    assert!(download_http::download_http_should_retry_status(
        StatusCode::TOO_MANY_REQUESTS
    ));
    assert!(download_http::download_http_should_retry_status(
        StatusCode::BAD_GATEWAY
    ));
    assert!(!download_http::download_http_should_retry_status(
        StatusCode::NOT_FOUND
    ));
}

#[test]
fn resumed_download_response_modes_never_append_a_full_response() {
    assert_eq!(
        download_http::download_response_mode(StatusCode::OK, 1024, Some(2048)).unwrap(),
        download_http::DownloadResponseMode::Truncate
    );
    assert_eq!(
        download_http::download_response_mode(StatusCode::PARTIAL_CONTENT, 1024, Some(2048))
            .unwrap(),
        download_http::DownloadResponseMode::Append
    );
    assert_eq!(
        download_http::download_response_mode(StatusCode::RANGE_NOT_SATISFIABLE, 2048, Some(2048))
            .unwrap(),
        download_http::DownloadResponseMode::Complete
    );
    assert!(download_http::download_response_mode(StatusCode::NOT_FOUND, 0, Some(2048)).is_err());
}

#[test]
fn mirror_transfers_use_shared_proxy_fallback_without_system_curl() {
    let source = include_str!("chatgpt_desktop.rs");
    let shared = include_str!("download_http.rs");
    let fetch_body = source
        .split("fn fetch_text")
        .nth(1)
        .and_then(|body| body.split("fn download_to_file").next())
        .expect("fetch_text should exist");
    let download_body = source
        .split("fn download_to_file")
        .nth(1)
        .and_then(|body| body.split("fn emit_step_progress").next())
        .expect("download_to_file should exist");

    assert!(shared.contains("builder.http1_only()"));
    assert!(shared.contains("builder.no_proxy()"));
    assert!(shared.contains("DownloadHttpTransport::MacosSystemProxy"));
    assert!(fetch_body.contains("download_http::fetch_text"));
    assert!(!fetch_body.contains("hidden_command(\"curl\")"));
    assert!(download_body.contains("download_http::download_to_file"));
    assert!(!download_body.contains("hidden_command(\"curl\")"));
}

fn read_test_http_request(stream: &mut std::net::TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut buffer).unwrap();
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
    }
    String::from_utf8(request).unwrap()
}

#[test]
fn mirror_metadata_retries_an_interrupted_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut first, _) = listener.accept().unwrap();
        let first_request = read_test_http_request(&mut first);
        assert!(first_request.starts_with("GET /latest/manifest HTTP/1.1"));
        first
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\nmanif")
            .unwrap();
        drop(first);

        let (mut second, _) = listener.accept().unwrap();
        let second_request = read_test_http_request(&mut second);
        assert!(second_request.starts_with("GET /latest/manifest HTTP/1.1"));
        second
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\nmanifest-ok",
            )
            .unwrap();
    });

    let text = fetch_text(&format!("http://{address}/latest/manifest")).unwrap();
    assert_eq!(text, "manifest-ok");
    server.join().unwrap();
}

#[test]
fn mirror_package_download_resumes_after_an_interrupted_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut first, _) = listener.accept().unwrap();
        let first_request = read_test_http_request(&mut first).to_ascii_lowercase();
        assert!(first_request.starts_with("get /package.dmg http/1.1"));
        assert!(!first_request.contains("range:"));
        first
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nhello")
            .unwrap();
        drop(first);

        let (mut second, _) = listener.accept().unwrap();
        let second_request = read_test_http_request(&mut second).to_ascii_lowercase();
        assert!(second_request.starts_with("get /package.dmg http/1.1"));
        assert!(second_request.contains("range: bytes=5-"));
        second
            .write_all(
                b"HTTP/1.1 206 Partial Content\r\nContent-Length: 5\r\nContent-Range: bytes 5-9/10\r\nConnection: close\r\n\r\nworld",
            )
            .unwrap();
    });

    let root = std::env::temp_dir().join(format!(
        "codestudio-lite-chatgpt-download-retry-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let target = root.join("package.dmg");
    download_to_file(
        &format!("http://{address}/package.dmg"),
        &target,
        Some(10),
        "macos",
        &|_| {},
    )
    .unwrap();

    assert_eq!(fs::read(&target).unwrap(), b"helloworld");
    server.join().unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn macos_app_candidates_prioritize_chatgpt_and_preserve_legacy_aliases() {
    let home = Path::new("/Users/tester");
    let candidates = macos_app_candidates_for_home(Some(home));

    assert_eq!(
        candidates,
        vec![
            PathBuf::from("/Applications/ChatGPT.app"),
            PathBuf::from("/Applications/Codex.app"),
            PathBuf::from("/Applications/OpenAI Codex.app"),
            PathBuf::from("/Applications/OpenAI.Codex.app"),
            PathBuf::from("/Users/tester/Applications/ChatGPT.app"),
            PathBuf::from("/Users/tester/Applications/Codex.app"),
            PathBuf::from("/Users/tester/Applications/OpenAI Codex.app"),
            PathBuf::from("/Users/tester/Applications/OpenAI.Codex.app"),
        ]
    );
    assert_eq!(default_macos_install_root(), "/Applications/ChatGPT.app");
}

#[test]
fn macos_bundle_executable_drives_process_and_tool_identity() {
    let root = std::env::temp_dir().join(format!(
        "codestudio-lite-chatgpt-macos-bundle-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let app = root.join("ChatGPT.app");
    fs::create_dir_all(app.join("Contents")).unwrap();
    fs::write(
        app.join("Contents").join("Info.plist"),
        r#"<plist><dict>
<key>CFBundleIdentifier</key><string>com.openai.codex</string>
<key>CFBundleExecutable</key><string>ChatGPT</string>
</dict></plist>"#,
    )
    .unwrap();
    let installed = InstalledChatGptDesktop {
        path: app.to_string_lossy().to_string(),
        version: "1.0.0".to_string(),
        arch: None,
        source: "macos".to_string(),
        generation: ChatGptDesktopProductGeneration::Current,
        package_family_name: Some("com.openai.codex".to_string()),
        installed_at: None,
    };

    assert_eq!(macos_process_name_for_installed(&installed), "ChatGPT");
    assert_eq!(macos_tool_command(Some(&installed)), "ChatGPT.app");
    assert_eq!(
        macos_open_command(&installed, &["--remote-debugging-port=9229".to_string()]),
        vec![
            "open".to_string(),
            "-a".to_string(),
            app.to_string_lossy().to_string(),
            "--args".to_string(),
            "--remote-debugging-port=9229".to_string(),
        ]
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn launch_restart_closes_selected_macos_chatgpt_bundle_before_opening() {
    let source = include_str!("chatgpt_desktop.rs");
    let terminate_body = source
        .split("fn close_chatgpt_desktop_processes")
        .nth(1)
        .and_then(|body| body.split("fn macos_process_name_for_installed").next())
        .expect("terminate function should exist");

    assert!(terminate_body.contains("target_os = \"macos\""));
    assert!(terminate_body.contains("quit_macos_app_bundle"));
    assert!(terminate_body.contains("close_appx_package_for_update"));
    assert!(terminate_body.contains("close_processes_for_update"));
    assert!(!terminate_body.contains("Get-Process -Name Codex"));
    assert!(source.contains("install_macos_dmg_with_app_candidates"));
    assert!(source.contains("CHATGPT_MACOS_APP_CANDIDATES"));
    assert!(source.contains("package::macos_app_running"));
    assert!(!source.contains("args([\"-x\", \"Codex\"])"));
}

#[test]
fn enhancement_injection_keeps_watching_for_recreated_cdp_targets() {
    let source = include_str!("chatgpt_desktop.rs");

    assert!(source.contains("CODEX_PATCH_WATCHDOG_POLL_MS"));
    assert!(source.contains("CODEX_PATCH_WATCHDOG_MAX_MISSES"));
    assert!(source.contains("watch_codex_enhancement_target"));
    assert!(source.contains(".web_socket_debugger_url"));
    assert!(source.contains("websocket_url != active_websocket_url"));
    assert!(source.contains("Page.addScriptToEvaluateOnNewDocument"));
    assert!(source.contains("Runtime.evaluate"));
}

#[test]
fn cdp_target_picker_recognizes_chatgpt_desktop_and_rejects_unrelated_pages() {
    let unrelated = CdpTarget {
        target_type: "page".to_string(),
        title: "Other App".to_string(),
        url: "https://example.test".to_string(),
        web_socket_debugger_url: Some("ws://other".to_string()),
    };
    let chatgpt = CdpTarget {
        target_type: "page".to_string(),
        title: "ChatGPT".to_string(),
        url: "https://chatgpt.com/".to_string(),
        web_socket_debugger_url: Some("ws://chatgpt".to_string()),
    };
    let error_page = CdpTarget {
        target_type: "page".to_string(),
        title: "ChatGPT".to_string(),
        url: "data:text/html;charset=utf-8,%3Ctitle%3EChatGPT%3C/title%3E".to_string(),
        web_socket_debugger_url: Some("ws://chatgpt-error".to_string()),
    };

    assert_eq!(
        pick_cdp_target_from_targets(&[unrelated.clone(), chatgpt])
            .unwrap()
            .web_socket_debugger_url
            .as_deref(),
        Some("ws://chatgpt")
    );
    assert_eq!(
        pick_cdp_target_from_targets(&[error_page])
            .unwrap()
            .web_socket_debugger_url
            .as_deref(),
        Some("ws://chatgpt-error")
    );
    assert!(pick_cdp_target_from_targets(&[unrelated]).is_err());
}

#[test]
fn remote_plugin_marketplace_is_expanded_for_renderer_injection() {
    let root = std::env::temp_dir().join(format!(
        "codestudio-lite-plugin-marketplace-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let marketplace_dir = root
        .join(".tmp")
        .join("plugins-remote")
        .join(".agents")
        .join("plugins");
    let plugin_dir = root
        .join(".tmp")
        .join("plugins-remote")
        .join("plugins")
        .join("product-design")
        .join(".codex-plugin");
    fs::create_dir_all(&marketplace_dir).unwrap();
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::write(
        marketplace_dir.join("marketplace.json"),
        r#"{"name":"openai-curated-remote","plugins":[{"name":"product-design","remotePluginId":"Plugin_test"}]}"#,
    )
    .unwrap();
    fs::write(
        plugin_dir.join("plugin.json"),
        r#"{"interface":{"displayName":"Product Design"},"version":"1.0.0"}"#,
    )
    .unwrap();

    let marketplaces = codex_plugin_marketplaces_for_injection_from_home(&root);
    let marketplace = &marketplaces[0];
    let plugin = &marketplace["plugins"][0];

    assert_eq!(marketplace["name"], "openai-curated-remote");
    assert_eq!(plugin["id"], "product-design@openai-curated-remote");
    assert_eq!(plugin["marketplaceName"], "openai-curated-remote");
    assert_eq!(plugin["marketplacePath"], "openai-curated-remote");
    assert_eq!(plugin["remotePluginId"], "Plugin_test");
    assert_eq!(plugin["interface"]["displayName"], "Product Design");

    fs::remove_dir_all(root).unwrap();
}
