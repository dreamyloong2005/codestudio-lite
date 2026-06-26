use super::*;

fn installed(source: &str) -> InstalledCodexClient {
    InstalledCodexClient {
        path: "C:\\Program Files\\WindowsApps\\OpenAI.Codex".to_string(),
        version: "1.0.0.0".to_string(),
        arch: None,
        source: source.to_string(),
        package_family_name: if source == "msix" {
            Some("OpenAI.Codex_abc".to_string())
        } else {
            None
        },
        installed_at: None,
    }
}

#[test]
fn existing_msix_update_keeps_msix_route() {
    let mut settings = CodexClientSettings::default();
    settings.windows_install_mode = "portable".to_string();
    let installed = installed("msix");

    assert_eq!(
        select_install_route(&settings, Some(&installed)),
        "msix-sideload"
    );
}

#[test]
fn existing_portable_update_keeps_portable_route() {
    let settings = CodexClientSettings::default();
    let installed = installed("portable");

    assert_eq!(
        select_install_route(&settings, Some(&installed)),
        "portable-fallback"
    );
}

#[test]
fn default_windows_install_stays_msix_without_capability_fallback() {
    let settings = CodexClientSettings::default();

    assert_eq!(select_install_route(&settings, None), "msix-sideload");
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
        "codestudio-lite-codex-client-test-{}-{}",
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
fn launch_restart_closes_macos_codex_before_opening() {
    let source = include_str!("codex_client.rs");
    let terminate_body = source
        .split("fn terminate_codex_process_for_restart")
        .nth(1)
        .and_then(|body| body.split("fn launch_installed_codex").next())
        .expect("terminate function should exist");

    assert!(terminate_body.contains("target_os = \"macos\""));
    assert!(terminate_body.contains("terminate_macos_codex_process_for_restart"));
    assert!(source.contains("pgrep"));
    assert!(source.contains("kill"));
    assert!(source.contains("A Codex desktop process is still running; restart was not continued."));
}
