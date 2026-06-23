use crate::core::app_paths::{app_paths, ensure_dirs};
use crate::core::codex_client::CodexClientState;
use crate::core::privacy_filter::{PrivacyFilterAction, PrivacyFilterMode};
use crate::core::types::{
    ActiveProfilesByMode, ActivityEvent, BackupManifest, DetectionSnapshot, DetectionSource,
    GatewayRequestLogEntry, ProfileDraft, ProviderApplyMode, Severity, UsageQueryResult,
    UsageScriptConfig, UsageScriptTemplateType,
};
use rusqlite::{params, Connection, OptionalExtension};

const SCHEMA_VERSION: i64 = 5;

#[derive(Debug, Clone)]
pub struct StoredAppConfig {
    pub active_profiles_by_mode: ActiveProfilesByMode,
    pub theme: String,
    pub language: String,
    pub language_set_by_user: bool,
    pub backup_before_write: bool,
    pub redact_secrets: bool,
    pub confirm_install_commands: bool,
    pub confirm_config_writes: bool,
    pub preserve_codex_official_auth: bool,
}

pub fn ensure_initialized() -> Result<(), String> {
    let _ = connection()?;
    Ok(())
}

pub fn load_app_config() -> Result<StoredAppConfig, String> {
    let conn = connection()?;
    let active_profiles_by_mode = load_active_profiles_with_conn(&conn)?;
    let language_set_by_user = setting_bool(&conn, "ui.language_set_by_user")?.unwrap_or(false);
    Ok(StoredAppConfig {
        active_profiles_by_mode,
        theme: setting_string(&conn, "ui.theme")?.unwrap_or_else(|| "system".to_string()),
        language: load_or_initialize_language_with_conn(
            &conn,
            language_set_by_user,
            detect_system_locale_name,
        )?,
        language_set_by_user,
        backup_before_write: setting_bool(&conn, "security.backup_before_write")?.unwrap_or(true),
        redact_secrets: setting_bool(&conn, "security.redact_secrets")?.unwrap_or(true),
        confirm_install_commands: setting_bool(&conn, "security.confirm_install_commands")?
            .unwrap_or(true),
        confirm_config_writes: setting_bool(&conn, "security.confirm_config_writes")?
            .unwrap_or(true),
        preserve_codex_official_auth: setting_bool(&conn, "security.preserve_codex_official_auth")?
            .unwrap_or(true),
    })
}

pub fn save_app_config(config: &StoredAppConfig) -> Result<(), String> {
    let conn = connection()?;
    let tx = conn
        .unchecked_transaction()
        .map_err(|err| err.to_string())?;
    save_setting(&tx, "ui.theme", &config.theme)?;
    save_setting(&tx, "ui.language", &config.language)?;
    save_setting(
        &tx,
        "ui.language_set_by_user",
        bool_value(config.language_set_by_user),
    )?;
    save_setting(
        &tx,
        "security.backup_before_write",
        bool_value(config.backup_before_write),
    )?;
    save_setting(
        &tx,
        "security.redact_secrets",
        bool_value(config.redact_secrets),
    )?;
    save_setting(
        &tx,
        "security.confirm_install_commands",
        bool_value(config.confirm_install_commands),
    )?;
    save_setting(
        &tx,
        "security.confirm_config_writes",
        bool_value(config.confirm_config_writes),
    )?;
    save_setting(
        &tx,
        "security.preserve_codex_official_auth",
        bool_value(config.preserve_codex_official_auth),
    )?;
    replace_active_profiles_with_conn(&tx, &config.active_profiles_by_mode)?;
    tx.commit().map_err(|err| err.to_string())
}

pub fn load_profiles() -> Result<Vec<ProfileDraft>, String> {
    let conn = connection()?;
    let mut statement = conn
        .prepare(
            "SELECT id, name, icon, remark, app, mode, provider, protocol, model, base_url, auth_ref,
                    created_at, updated_at, last_test_status, sort_order
             FROM profiles
             ORDER BY app ASC, mode ASC, sort_order ASC, name ASC",
        )
        .map_err(|err| err.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(ProfileDraft {
                id: row.get(0)?,
                name: row.get(1)?,
                icon: row.get(2)?,
                remark: row.get(3)?,
                app: row.get(4)?,
                is_builtin: false,
                mode: mode_from_storage(row.get::<_, String>(5)?.as_str()),
                provider: row.get(6)?,
                protocol: row.get(7)?,
                model: row.get(8)?,
                base_url: row.get(9)?,
                auth_ref: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
                last_test_status: row.get(13)?,
                usage_enabled: false,
                sort_order: row.get(14)?,
            })
        })
        .map_err(|err| err.to_string())?;
    let mut profiles = Vec::new();
    for row in rows {
        profiles.push(row.map_err(|err| err.to_string())?);
    }
    Ok(profiles)
}

pub fn save_profile(profile: &ProfileDraft) -> Result<(), String> {
    let conn = connection()?;
    conn.execute(
        "INSERT INTO profiles (
            id, name, icon, remark, app, mode, provider, protocol, model, base_url, auth_ref,
            created_at, updated_at, last_test_status, sort_order
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(id) DO UPDATE SET
            name=excluded.name,
            icon=excluded.icon,
            remark=excluded.remark,
            app=excluded.app,
            mode=excluded.mode,
            provider=excluded.provider,
            protocol=excluded.protocol,
            model=excluded.model,
            base_url=excluded.base_url,
            auth_ref=excluded.auth_ref,
            created_at=excluded.created_at,
            updated_at=excluded.updated_at,
            last_test_status=excluded.last_test_status,
            sort_order=excluded.sort_order",
        params![
            profile.id,
            profile.name,
            profile.icon,
            profile.remark,
            profile.app,
            mode_to_storage(&profile.mode),
            profile.provider,
            profile.protocol,
            profile.model,
            profile.base_url,
            profile.auth_ref,
            profile.created_at,
            profile.updated_at,
            profile.last_test_status,
            profile.sort_order,
        ],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn next_profile_sort_order(app: &str, mode: &ProviderApplyMode) -> Result<i64, String> {
    let conn = connection()?;
    conn.query_row(
        "SELECT MAX(value) FROM (
            SELECT sort_order AS value FROM profiles WHERE app = ?1 AND mode = ?2
            UNION ALL
            SELECT sort_order AS value FROM profile_order WHERE app = ?1 AND mode = ?2
         )",
        params![app, mode_to_storage(mode)],
        |row| {
            let max_order = row.get::<_, Option<i64>>(0)?;
            Ok(max_order.unwrap_or(-1) + 1)
        },
    )
    .map_err(|err| err.to_string())
}

pub fn reorder_profiles(
    app: &str,
    mode: &ProviderApplyMode,
    profile_ids: &[String],
) -> Result<(), String> {
    let conn = connection()?;
    reorder_profiles_with_conn(&conn, app, mode, profile_ids)
}

fn reorder_profiles_with_conn(
    conn: &Connection,
    app: &str,
    mode: &ProviderApplyMode,
    profile_ids: &[String],
) -> Result<(), String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|err| err.to_string())?;
    let mode_value = mode_to_storage(mode);
    tx.execute(
        "DELETE FROM profile_order WHERE app = ?1 AND mode = ?2",
        params![app, mode_value],
    )
    .map_err(|err| err.to_string())?;

    for (index, profile_id) in profile_ids.iter().enumerate() {
        let sort_order = index as i64;
        tx.execute(
            "INSERT INTO profile_order (app, mode, profile_id, sort_order)
             VALUES (?1, ?2, ?3, ?4)",
            params![app, mode_value, profile_id, sort_order],
        )
        .map_err(|err| err.to_string())?;
        tx.execute(
            "UPDATE profiles
             SET sort_order = ?1
             WHERE id = ?2 AND app = ?3 AND mode = ?4",
            params![sort_order, profile_id, app, mode_value],
        )
        .map_err(|err| err.to_string())?;
    }

    tx.commit().map_err(|err| err.to_string())
}

pub fn load_profile_order(app: &str, mode: &ProviderApplyMode) -> Result<Vec<String>, String> {
    let conn = connection()?;
    load_profile_order_with_conn(&conn, app, mode)
}

fn load_profile_order_with_conn(
    conn: &Connection,
    app: &str,
    mode: &ProviderApplyMode,
) -> Result<Vec<String>, String> {
    let mut statement = conn
        .prepare(
            "SELECT profile_id FROM profile_order
             WHERE app = ?1 AND mode = ?2
             ORDER BY sort_order ASC, profile_id ASC",
        )
        .map_err(|err| err.to_string())?;
    let rows = statement
        .query_map(params![app, mode_to_storage(mode)], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|err| err.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;
    Ok(rows)
}

pub fn delete_profile(profile_id: &str) -> Result<bool, String> {
    let conn = connection()?;
    let deleted = conn
        .execute("DELETE FROM profiles WHERE id = ?1", params![profile_id])
        .map_err(|err| err.to_string())?;
    conn.execute(
        "DELETE FROM active_profiles WHERE profile_id = ?1",
        params![profile_id],
    )
    .map_err(|err| err.to_string())?;
    Ok(deleted > 0)
}

pub fn save_codex_oauth_profile(profile_id: &str, auth_json: &str) -> Result<(), String> {
    let conn = connection()?;
    save_codex_oauth_profile_with_conn(&conn, profile_id, auth_json)
}

pub fn load_codex_oauth_profile(profile_id: &str) -> Result<Option<String>, String> {
    let conn = connection()?;
    load_codex_oauth_profile_with_conn(&conn, profile_id)
}

pub fn copy_codex_oauth_profile(
    source_profile_id: &str,
    target_profile_id: &str,
) -> Result<bool, String> {
    let conn = connection()?;
    copy_codex_oauth_profile_with_conn(&conn, source_profile_id, target_profile_id)
}

pub fn delete_codex_oauth_profile(profile_id: &str) -> Result<(), String> {
    let conn = connection()?;
    delete_codex_oauth_profile_with_conn(&conn, profile_id)
}

pub fn save_state_json(key: &str, json: &str) -> Result<(), String> {
    let conn = connection()?;
    save_setting(&conn, key, json)
}

pub fn load_state_json(key: &str) -> Result<Option<String>, String> {
    let conn = connection()?;
    setting_string(&conn, key)
}

pub fn delete_state_json(key: &str) -> Result<(), String> {
    let conn = connection()?;
    conn.execute("DELETE FROM settings WHERE key = ?1", params![key])
        .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn load_active_profiles() -> Result<ActiveProfilesByMode, String> {
    let conn = connection()?;
    load_active_profiles_with_conn(&conn)
}

pub fn replace_active_profiles(active: &ActiveProfilesByMode) -> Result<(), String> {
    let conn = connection()?;
    replace_active_profiles_with_conn(&conn, active)
}

pub fn store_detection_cache(snapshot: &DetectionSnapshot) -> Result<(), String> {
    let conn = connection()?;
    let json = serde_json::to_string(snapshot).map_err(|err| err.to_string())?;
    conn.execute(
        "INSERT INTO detection_cache (id, generated_at, snapshot_json)
         VALUES (1, ?1, ?2)
         ON CONFLICT(id) DO UPDATE SET
            generated_at=excluded.generated_at,
            snapshot_json=excluded.snapshot_json",
        params![snapshot.generated_at, json],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn load_detection_cache() -> Result<Option<DetectionSnapshot>, String> {
    let conn = connection()?;
    let json = conn
        .query_row(
            "SELECT snapshot_json FROM detection_cache WHERE id = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|err| err.to_string())?;
    let Some(json) = json else {
        return Ok(None);
    };
    let mut snapshot =
        serde_json::from_str::<DetectionSnapshot>(&json).map_err(|err| err.to_string())?;
    snapshot.source = DetectionSource::Cached;
    Ok(Some(snapshot))
}

pub fn store_codex_client_state(state: &CodexClientState) -> Result<(), String> {
    let conn = connection()?;
    let json = serde_json::to_string(state).map_err(|err| err.to_string())?;
    conn.execute(
        "INSERT INTO codex_client_state (id, generated_at, state_json)
         VALUES (1, ?1, ?2)
         ON CONFLICT(id) DO UPDATE SET
            generated_at=excluded.generated_at,
            state_json=excluded.state_json",
        params![state.generated_at, json],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn load_codex_client_state() -> Result<Option<CodexClientState>, String> {
    let conn = connection()?;
    let json = conn
        .query_row(
            "SELECT state_json FROM codex_client_state WHERE id = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|err| err.to_string())?;
    let Some(json) = json else {
        return Ok(None);
    };
    let state = serde_json::from_str::<CodexClientState>(&json).map_err(|err| err.to_string())?;
    Ok(Some(state))
}

pub fn append_activity_event(event: &ActivityEvent) -> Result<(), String> {
    let conn = connection()?;
    conn.execute(
        "INSERT OR REPLACE INTO activity_events (id, level, message, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            event.id,
            severity_to_storage(&event.level),
            event.message,
            event.created_at,
        ],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn load_recent_activity(limit: usize) -> Result<Vec<ActivityEvent>, String> {
    let conn = connection()?;
    let mut statement = conn
        .prepare(
            "SELECT id, level, message, created_at
             FROM activity_events
             ORDER BY created_at DESC, rowid DESC
             LIMIT ?1",
        )
        .map_err(|err| err.to_string())?;
    let rows = statement
        .query_map(params![limit as i64], |row| {
            Ok(ActivityEvent {
                id: row.get(0)?,
                level: severity_from_storage(row.get::<_, String>(1)?.as_str()),
                message: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|err| err.to_string())?;
    let mut events = Vec::new();
    for row in rows {
        events.push(row.map_err(|err| err.to_string())?);
    }
    Ok(events)
}

pub fn append_gateway_request(entry: &GatewayRequestLogEntry) -> Result<(), String> {
    let conn = connection()?;
    conn.execute(
        "INSERT OR REPLACE INTO gateway_request_logs (
            id, timestamp, client, method, path, provider, model, status, latency_ms, error_summary,
            privacy_filter_mode, privacy_filter_hit_count, privacy_filter_action
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            entry.id,
            entry.timestamp,
            entry.client,
            entry.method,
            entry.path,
            entry.provider,
            entry.model,
            i64::from(entry.status),
            entry.latency_ms.to_string(),
            entry.error_summary,
            privacy_filter_mode_to_storage(&entry.privacy_filter_mode),
            entry.privacy_filter_hit_count as i64,
            privacy_filter_action_to_storage(&entry.privacy_filter_action),
        ],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn load_recent_gateway_requests(limit: usize) -> Result<Vec<GatewayRequestLogEntry>, String> {
    let conn = connection()?;
    let mut statement = conn
        .prepare(
            "SELECT id, timestamp, client, method, path, provider, model, status, latency_ms, error_summary,
                    privacy_filter_mode, privacy_filter_hit_count, privacy_filter_action
             FROM gateway_request_logs
             ORDER BY timestamp DESC, rowid DESC
             LIMIT ?1",
        )
        .map_err(|err| err.to_string())?;
    let rows = statement
        .query_map(params![limit as i64], |row| {
            let latency: String = row.get(8)?;
            Ok(GatewayRequestLogEntry {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                client: row.get(2)?,
                method: row.get(3)?,
                path: row.get(4)?,
                provider: row.get(5)?,
                model: row.get(6)?,
                status: row.get::<_, i64>(7)? as u16,
                latency_ms: latency.parse::<u128>().unwrap_or(0),
                error_summary: row.get(9)?,
                privacy_filter_mode: privacy_filter_mode_from_storage(
                    row.get::<_, String>(10)?.as_str(),
                ),
                privacy_filter_hit_count: row.get::<_, i64>(11)?.max(0) as usize,
                privacy_filter_action: privacy_filter_action_from_storage(
                    row.get::<_, String>(12)?.as_str(),
                ),
            })
        })
        .map_err(|err| err.to_string())?;
    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(|err| err.to_string())?);
    }
    Ok(entries)
}

pub fn save_backup_manifest(manifest: &BackupManifest) -> Result<(), String> {
    let conn = connection()?;
    let changed_files =
        serde_json::to_string(&manifest.changed_files).map_err(|err| err.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO backup_manifests (id, reason, profile, changed_files_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            manifest.id,
            manifest.reason,
            manifest.profile,
            changed_files,
            manifest.created_at,
        ],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn save_backup_file(
    backup_id: &str,
    target_path: &str,
    content: Option<&[u8]>,
) -> Result<(), String> {
    let conn = connection()?;
    save_backup_file_with_conn(&conn, backup_id, target_path, content)
}

pub fn load_backup_file(backup_id: &str, target_path: &str) -> Result<Option<Vec<u8>>, String> {
    let conn = connection()?;
    load_backup_file_with_conn(&conn, backup_id, target_path)
}

pub fn load_backup_manifests() -> Result<Vec<BackupManifest>, String> {
    let conn = connection()?;
    let mut statement = conn
        .prepare(
            "SELECT id, reason, profile, changed_files_json, created_at
             FROM backup_manifests
             ORDER BY created_at DESC, rowid DESC",
        )
        .map_err(|err| err.to_string())?;
    let rows = statement
        .query_map([], |row| {
            let changed_files_json: String = row.get(3)?;
            let changed_files =
                serde_json::from_str::<Vec<String>>(&changed_files_json).unwrap_or_default();
            Ok(BackupManifest {
                id: row.get(0)?,
                reason: row.get(1)?,
                profile: row.get(2)?,
                changed_files,
                created_at: row.get(4)?,
            })
        })
        .map_err(|err| err.to_string())?;
    let mut manifests = Vec::new();
    for row in rows {
        manifests.push(row.map_err(|err| err.to_string())?);
    }
    Ok(manifests)
}

pub fn load_backup_manifest(id: &str) -> Result<Option<BackupManifest>, String> {
    let conn = connection()?;
    conn.query_row(
        "SELECT id, reason, profile, changed_files_json, created_at
         FROM backup_manifests
         WHERE id = ?1",
        params![id],
        |row| {
            let changed_files_json: String = row.get(3)?;
            let changed_files =
                serde_json::from_str::<Vec<String>>(&changed_files_json).unwrap_or_default();
            Ok(BackupManifest {
                id: row.get(0)?,
                reason: row.get(1)?,
                profile: row.get(2)?,
                changed_files,
                created_at: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(|err| err.to_string())
}

pub fn save_usage_script(config: &UsageScriptConfig) -> Result<(), String> {
    let conn = connection()?;
    conn.execute(
        "INSERT OR REPLACE INTO usage_scripts (
            profile_id, enabled, template_type, code, api_key_ref, base_url,
            access_token_ref, user_id, timeout_seconds, auto_query_interval_minutes, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            config.profile_id,
            bool_value(config.enabled),
            usage_template_to_storage(&config.template_type),
            config.code,
            config.api_key,
            config.base_url,
            config.access_token,
            config.user_id,
            i64::from(config.timeout_seconds),
            i64::from(config.auto_query_interval_minutes),
            config.updated_at,
        ],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn load_usage_script(profile_id: &str) -> Result<Option<UsageScriptConfig>, String> {
    let conn = connection()?;
    conn.query_row(
        "SELECT profile_id, enabled, template_type, code, api_key_ref, base_url,
                access_token_ref, user_id, timeout_seconds, auto_query_interval_minutes, updated_at
         FROM usage_scripts
         WHERE profile_id = ?1",
        params![profile_id],
        |row| {
            Ok(UsageScriptConfig {
                profile_id: row.get(0)?,
                enabled: storage_bool(row.get::<_, String>(1)?.as_str()),
                template_type: usage_template_from_storage(row.get::<_, String>(2)?.as_str()),
                code: row.get(3)?,
                api_key: row.get(4)?,
                base_url: row.get(5)?,
                access_token: row.get(6)?,
                user_id: row.get(7)?,
                timeout_seconds: row.get::<_, i64>(8)? as u16,
                auto_query_interval_minutes: row.get::<_, i64>(9)? as u16,
                updated_at: row.get(10)?,
            })
        },
    )
    .optional()
    .map_err(|err| err.to_string())
}

pub fn load_usage_enabled_profile_ids() -> Result<std::collections::HashSet<String>, String> {
    let conn = connection()?;
    let mut statement = conn
        .prepare("SELECT profile_id FROM usage_scripts WHERE enabled = 'true'")
        .map_err(|err| err.to_string())?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|err| err.to_string())?;
    let mut ids = std::collections::HashSet::new();
    for row in rows {
        ids.insert(row.map_err(|err| err.to_string())?);
    }
    Ok(ids)
}

pub fn delete_usage_script(profile_id: &str) -> Result<(), String> {
    let conn = connection()?;
    conn.execute(
        "DELETE FROM usage_scripts WHERE profile_id = ?1",
        params![profile_id],
    )
    .map_err(|err| err.to_string())?;
    conn.execute(
        "DELETE FROM usage_results WHERE profile_id = ?1",
        params![profile_id],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn save_usage_result(profile_id: &str, result: &UsageQueryResult) -> Result<(), String> {
    let conn = connection()?;
    let json = serde_json::to_string(result).map_err(|err| err.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO usage_results (profile_id, queried_at, result_json)
         VALUES (?1, ?2, ?3)",
        params![profile_id, result.queried_at, json],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn load_usage_result(profile_id: &str) -> Result<Option<UsageQueryResult>, String> {
    let conn = connection()?;
    let json = conn
        .query_row(
            "SELECT result_json FROM usage_results WHERE profile_id = ?1",
            params![profile_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|err| err.to_string())?;
    let Some(json) = json else {
        return Ok(None);
    };
    serde_json::from_str::<UsageQueryResult>(&json)
        .map(Some)
        .map_err(|err| err.to_string())
}

fn connection() -> Result<Connection, String> {
    let paths = app_paths().map_err(|err| err.to_string())?;
    ensure_dirs(&paths).map_err(|err| err.to_string())?;
    let conn = Connection::open(&paths.database_file).map_err(|err| err.to_string())?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|err| err.to_string())?;
    initialize_schema(&conn)?;
    Ok(conn)
}

fn initialize_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS meta (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS settings (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS profiles (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          icon TEXT,
          remark TEXT,
          app TEXT NOT NULL,
          mode TEXT NOT NULL,
          provider TEXT NOT NULL,
          protocol TEXT NOT NULL,
          model TEXT NOT NULL,
          base_url TEXT NOT NULL,
          auth_ref TEXT,
          created_at TEXT,
          updated_at TEXT,
          last_test_status TEXT,
          sort_order INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS active_profiles (
          mode TEXT NOT NULL,
          app TEXT NOT NULL,
          profile_id TEXT NOT NULL,
          PRIMARY KEY (mode, app)
        );
        CREATE TABLE IF NOT EXISTS profile_order (
          app TEXT NOT NULL,
          mode TEXT NOT NULL,
          profile_id TEXT NOT NULL,
          sort_order INTEGER NOT NULL,
          PRIMARY KEY (app, mode, profile_id)
        );
        CREATE TABLE IF NOT EXISTS detection_cache (
          id INTEGER PRIMARY KEY CHECK (id = 1),
          generated_at TEXT NOT NULL,
          snapshot_json TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS codex_client_state (
          id INTEGER PRIMARY KEY CHECK (id = 1),
          generated_at TEXT NOT NULL,
          state_json TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS activity_events (
          id TEXT PRIMARY KEY,
          level TEXT NOT NULL,
          message TEXT NOT NULL,
          created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS gateway_request_logs (
          id TEXT PRIMARY KEY,
          timestamp TEXT NOT NULL,
          client TEXT NOT NULL,
          method TEXT NOT NULL,
          path TEXT NOT NULL,
          provider TEXT,
          model TEXT,
          status INTEGER NOT NULL,
          latency_ms TEXT NOT NULL,
          error_summary TEXT,
          privacy_filter_mode TEXT NOT NULL DEFAULT 'off',
          privacy_filter_hit_count INTEGER NOT NULL DEFAULT 0,
          privacy_filter_action TEXT NOT NULL DEFAULT 'none'
        );
        CREATE TABLE IF NOT EXISTS backup_manifests (
          id TEXT PRIMARY KEY,
          reason TEXT NOT NULL,
          profile TEXT,
          changed_files_json TEXT NOT NULL,
          created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS backup_files (
          backup_id TEXT NOT NULL,
          target_path TEXT NOT NULL,
          content BLOB,
          PRIMARY KEY (backup_id, target_path)
        );
        CREATE TABLE IF NOT EXISTS usage_scripts (
          profile_id TEXT PRIMARY KEY,
          enabled TEXT NOT NULL,
          template_type TEXT NOT NULL,
          code TEXT NOT NULL,
          api_key_ref TEXT,
          base_url TEXT,
          access_token_ref TEXT,
          user_id TEXT,
          timeout_seconds INTEGER NOT NULL,
          auto_query_interval_minutes INTEGER NOT NULL,
          updated_at TEXT
        );
        CREATE TABLE IF NOT EXISTS usage_results (
          profile_id TEXT PRIMARY KEY,
          queried_at TEXT NOT NULL,
          result_json TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS codex_oauth_profiles (
          profile_id TEXT PRIMARY KEY,
          tool_family TEXT NOT NULL,
          auth_json TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_profiles_app_mode ON profiles(app, mode);
        CREATE INDEX IF NOT EXISTS idx_profile_order_app_mode ON profile_order(app, mode, sort_order);
        CREATE INDEX IF NOT EXISTS idx_activity_created ON activity_events(created_at);
        CREATE INDEX IF NOT EXISTS idx_gateway_timestamp ON gateway_request_logs(timestamp);
        ",
    )
    .map_err(|err| err.to_string())?;
    ensure_profiles_icon_column(conn)?;
    ensure_profiles_remark_column(conn)?;
    ensure_profiles_sort_order_column(conn)?;
    ensure_gateway_request_privacy_columns(conn)?;
    save_meta(conn, "schema_version", &SCHEMA_VERSION.to_string())?;
    Ok(())
}

fn ensure_profiles_icon_column(conn: &Connection) -> Result<(), String> {
    if table_has_column(conn, "profiles", "icon")? {
        return Ok(());
    }
    conn.execute("ALTER TABLE profiles ADD COLUMN icon TEXT", [])
        .map_err(|err| err.to_string())?;
    Ok(())
}

fn ensure_profiles_remark_column(conn: &Connection) -> Result<(), String> {
    if table_has_column(conn, "profiles", "remark")? {
        return Ok(());
    }
    conn.execute("ALTER TABLE profiles ADD COLUMN remark TEXT", [])
        .map_err(|err| err.to_string())?;
    Ok(())
}

fn ensure_profiles_sort_order_column(conn: &Connection) -> Result<(), String> {
    if table_has_column(conn, "profiles", "sort_order")? {
        return Ok(());
    }
    conn.execute(
        "ALTER TABLE profiles ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0",
        [],
    )
    .map_err(|err| err.to_string())?;
    initialize_profile_sort_order(conn)
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, String> {
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|err| err.to_string())?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|err| err.to_string())?;
    for item in columns {
        if item.map_err(|err| err.to_string())? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn ensure_gateway_request_privacy_columns(conn: &Connection) -> Result<(), String> {
    let columns = [
        (
            "privacy_filter_mode",
            "ALTER TABLE gateway_request_logs ADD COLUMN privacy_filter_mode TEXT NOT NULL DEFAULT 'off'",
        ),
        (
            "privacy_filter_hit_count",
            "ALTER TABLE gateway_request_logs ADD COLUMN privacy_filter_hit_count INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "privacy_filter_action",
            "ALTER TABLE gateway_request_logs ADD COLUMN privacy_filter_action TEXT NOT NULL DEFAULT 'none'",
        ),
    ];

    for (column, statement) in columns {
        if !table_has_column(conn, "gateway_request_logs", column)? {
            conn.execute(statement, []).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn initialize_profile_sort_order(conn: &Connection) -> Result<(), String> {
    let mut statement = conn
        .prepare("SELECT id FROM profiles ORDER BY app ASC, mode ASC, name ASC, rowid ASC")
        .map_err(|err| err.to_string())?;
    let ids = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|err| err.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;

    for (index, profile_id) in ids.iter().enumerate() {
        conn.execute(
            "UPDATE profiles SET sort_order = ?1 WHERE id = ?2",
            params![index as i64, profile_id],
        )
        .map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn setting_string(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .map_err(|err| err.to_string())
}

fn setting_bool(conn: &Connection, key: &str) -> Result<Option<bool>, String> {
    Ok(setting_string(conn, key)?.map(|value| value == "true" || value == "1"))
}

fn load_or_initialize_language_with_conn<F>(
    conn: &Connection,
    language_set_by_user: bool,
    detect_locale: F,
) -> Result<String, String>
where
    F: FnOnce() -> Option<String>,
{
    let detected_locale = detect_locale();
    let detected_language = app_language_from_system_locale(detected_locale.as_deref());
    if let Some(language) = setting_string(conn, "ui.language")? {
        if !language_set_by_user && language == "en-US" && detected_language != "en-US" {
            save_setting(conn, "ui.language", detected_language)?;
            return Ok(detected_language.to_string());
        }
        return Ok(language);
    }
    save_setting(conn, "ui.language", detected_language)?;
    Ok(detected_language.to_string())
}

fn app_language_from_system_locale(locale: Option<&str>) -> &'static str {
    let Some(locale) = locale else {
        return "en-US";
    };
    let normalized = locale.trim().replace('_', "-").to_lowercase();
    if normalized.is_empty() {
        return "en-US";
    }
    if normalized.starts_with("zh-hant")
        || normalized.starts_with("zh-tw")
        || normalized.starts_with("zh-hk")
        || normalized.starts_with("zh-mo")
    {
        return "zh-TW";
    }
    if normalized.starts_with("zh") {
        return "zh-CN";
    }
    if normalized.starts_with("en") {
        return "en-US";
    }
    "en-US"
}

#[cfg(windows)]
fn detect_system_locale_name() -> Option<String> {
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;

    let mut buffer = [0u16; 85];
    let len = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
    if len <= 1 {
        return None;
    }
    let usable_len = usize::try_from(len - 1).ok()?;
    Some(String::from_utf16_lossy(&buffer[..usable_len]))
}

#[cfg(target_os = "macos")]
fn detect_system_locale_name() -> Option<String> {
    detect_macos_preferred_locale_name().or_else(detect_unix_locale_name)
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn detect_system_locale_name() -> Option<String> {
    detect_unix_locale_name()
}

#[cfg(target_os = "macos")]
fn detect_macos_preferred_locale_name() -> Option<String> {
    let output = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_macos_apple_languages(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(target_os = "macos")]
fn parse_macos_apple_languages(value: &str) -> Option<String> {
    value.lines().find_map(|line| {
        let candidate = line
            .trim()
            .trim_start_matches('"')
            .trim_end_matches(',')
            .trim_end_matches('"')
            .trim();
        (!candidate.is_empty()
            && candidate != "("
            && candidate != ")"
            && !candidate.starts_with('{'))
        .then(|| candidate.to_string())
    })
}

#[cfg(not(windows))]
fn detect_unix_locale_name() -> Option<String> {
    ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|key| {
            let value = std::env::var(key).ok()?;
            let locale = value.split([':', '.']).next()?.trim();
            (!locale.is_empty()).then(|| locale.to_string())
        })
}

fn save_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn save_meta(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn load_active_profiles_with_conn(conn: &Connection) -> Result<ActiveProfilesByMode, String> {
    let mut statement = conn
        .prepare("SELECT mode, app, profile_id FROM active_profiles")
        .map_err(|err| err.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|err| err.to_string())?;
    let mut active = ActiveProfilesByMode::default();
    for row in rows {
        let (mode, app, profile_id) = row.map_err(|err| err.to_string())?;
        match mode.as_str() {
            "config" => {
                active.config.insert(app, profile_id);
            }
            "gateway" => {
                active.gateway.insert(app, profile_id);
            }
            _ => {}
        }
    }
    Ok(active)
}

fn replace_active_profiles_with_conn(
    conn: &Connection,
    active: &ActiveProfilesByMode,
) -> Result<(), String> {
    conn.execute("DELETE FROM active_profiles", [])
        .map_err(|err| err.to_string())?;
    for (app, profile_id) in &active.config {
        conn.execute(
            "INSERT OR REPLACE INTO active_profiles (mode, app, profile_id) VALUES ('config', ?1, ?2)",
            params![app, profile_id],
        )
        .map_err(|err| err.to_string())?;
    }
    for (app, profile_id) in &active.gateway {
        conn.execute(
            "INSERT OR REPLACE INTO active_profiles (mode, app, profile_id) VALUES ('gateway', ?1, ?2)",
            params![app, profile_id],
        )
        .map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn save_backup_file_with_conn(
    conn: &Connection,
    backup_id: &str,
    target_path: &str,
    content: Option<&[u8]>,
) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO backup_files (backup_id, target_path, content)
         VALUES (?1, ?2, ?3)",
        params![backup_id, target_path, content],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn load_backup_file_with_conn(
    conn: &Connection,
    backup_id: &str,
    target_path: &str,
) -> Result<Option<Vec<u8>>, String> {
    conn.query_row(
        "SELECT content FROM backup_files WHERE backup_id = ?1 AND target_path = ?2",
        params![backup_id, target_path],
        |row| row.get::<_, Option<Vec<u8>>>(0),
    )
    .optional()
    .map(|value| value.flatten())
    .map_err(|err| err.to_string())
}

fn save_codex_oauth_profile_with_conn(
    conn: &Connection,
    profile_id: &str,
    auth_json: &str,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO codex_oauth_profiles (
            profile_id, tool_family, auth_json, created_at, updated_at
         ) VALUES (?1, 'codex', ?2, ?3, ?3)
         ON CONFLICT(profile_id) DO UPDATE SET
            tool_family='codex',
            auth_json=excluded.auth_json,
            updated_at=excluded.updated_at",
        params![profile_id, auth_json, now],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn load_codex_oauth_profile_with_conn(
    conn: &Connection,
    profile_id: &str,
) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT auth_json FROM codex_oauth_profiles WHERE profile_id = ?1 AND tool_family = 'codex'",
        params![profile_id],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|err| err.to_string())
}

fn copy_codex_oauth_profile_with_conn(
    conn: &Connection,
    source_profile_id: &str,
    target_profile_id: &str,
) -> Result<bool, String> {
    let Some(auth_json) = load_codex_oauth_profile_with_conn(conn, source_profile_id)? else {
        return Ok(false);
    };
    save_codex_oauth_profile_with_conn(conn, target_profile_id, &auth_json)?;
    Ok(true)
}

fn delete_codex_oauth_profile_with_conn(conn: &Connection, profile_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM codex_oauth_profiles WHERE profile_id = ?1 AND tool_family = 'codex'",
        params![profile_id],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn bool_value(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn storage_bool(value: &str) -> bool {
    value == "true" || value == "1"
}

fn usage_template_to_storage(value: &UsageScriptTemplateType) -> &'static str {
    match value {
        UsageScriptTemplateType::Custom => "custom",
        UsageScriptTemplateType::General => "general",
        UsageScriptTemplateType::NewApi => "newapi",
        UsageScriptTemplateType::TokenPlan => "token_plan",
        UsageScriptTemplateType::Balance => "balance",
    }
}

fn usage_template_from_storage(value: &str) -> UsageScriptTemplateType {
    match value {
        "custom" => UsageScriptTemplateType::Custom,
        "newapi" => UsageScriptTemplateType::NewApi,
        "token_plan" => UsageScriptTemplateType::TokenPlan,
        "balance" => UsageScriptTemplateType::Balance,
        _ => UsageScriptTemplateType::General,
    }
}

fn mode_to_storage(mode: &ProviderApplyMode) -> &'static str {
    match mode {
        ProviderApplyMode::Config => "config",
        ProviderApplyMode::Gateway => "gateway",
    }
}

fn mode_from_storage(value: &str) -> ProviderApplyMode {
    match value {
        "config" => ProviderApplyMode::Config,
        _ => ProviderApplyMode::Gateway,
    }
}

fn severity_to_storage(value: &Severity) -> &'static str {
    match value {
        Severity::Ok => "ok",
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Error => "error",
    }
}

fn severity_from_storage(value: &str) -> Severity {
    match value {
        "ok" => Severity::Ok,
        "warning" => Severity::Warning,
        "error" => Severity::Error,
        _ => Severity::Info,
    }
}

fn privacy_filter_mode_to_storage(value: &PrivacyFilterMode) -> &'static str {
    match value {
        PrivacyFilterMode::Off => "off",
        PrivacyFilterMode::Detect => "detect",
        PrivacyFilterMode::Redact => "redact",
        PrivacyFilterMode::Block => "block",
    }
}

fn privacy_filter_mode_from_storage(value: &str) -> PrivacyFilterMode {
    match value {
        "detect" => PrivacyFilterMode::Detect,
        "redact" => PrivacyFilterMode::Redact,
        "block" => PrivacyFilterMode::Block,
        _ => PrivacyFilterMode::Off,
    }
}

fn privacy_filter_action_to_storage(value: &PrivacyFilterAction) -> &'static str {
    match value {
        PrivacyFilterAction::None => "none",
        PrivacyFilterAction::Detected => "detected",
        PrivacyFilterAction::Redacted => "redacted",
        PrivacyFilterAction::Blocked => "blocked",
    }
}

fn privacy_filter_action_from_storage(value: &str) -> PrivacyFilterAction {
    match value {
        "detected" => PrivacyFilterAction::Detected,
        "redacted" => PrivacyFilterAction::Redacted,
        "blocked" => PrivacyFilterAction::Blocked,
        _ => PrivacyFilterAction::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_locale_names_map_to_supported_app_languages() {
        assert_eq!(app_language_from_system_locale(Some("zh-CN")), "zh-CN");
        assert_eq!(app_language_from_system_locale(Some("zh-Hans-SG")), "zh-CN");
        assert_eq!(app_language_from_system_locale(Some("zh-TW")), "zh-TW");
        assert_eq!(app_language_from_system_locale(Some("zh-Hant-HK")), "zh-TW");
        assert_eq!(app_language_from_system_locale(Some("en-GB")), "en-US");
        assert_eq!(app_language_from_system_locale(Some("fr-FR")), "en-US");
        assert_eq!(app_language_from_system_locale(None), "en-US");
    }

    #[test]
    fn missing_language_is_initialized_once_and_persisted() {
        let conn = Connection::open_in_memory().expect("in-memory database should open");
        initialize_schema(&conn).expect("schema should initialize");

        let initialized =
            load_or_initialize_language_with_conn(&conn, false, || Some("zh-Hant-HK".to_string()))
                .expect("language should initialize");
        assert_eq!(initialized, "zh-TW");
        assert_eq!(
            setting_string(&conn, "ui.language")
                .expect("language setting should load")
                .as_deref(),
            Some("zh-TW")
        );

        let loaded =
            load_or_initialize_language_with_conn(&conn, false, || Some("zh-CN".to_string()))
                .expect("existing language should load");
        assert_eq!(loaded, "zh-TW");
    }

    #[test]
    fn default_english_language_is_corrected_to_detected_chinese() {
        let conn = Connection::open_in_memory().expect("in-memory database should open");
        initialize_schema(&conn).expect("schema should initialize");
        save_setting(&conn, "ui.language", "en-US").expect("language should save");

        let loaded =
            load_or_initialize_language_with_conn(&conn, false, || Some("zh-Hans-CN".to_string()))
                .expect("language should load");

        assert_eq!(loaded, "zh-CN");
        assert_eq!(
            setting_string(&conn, "ui.language")
                .expect("language setting should load")
                .as_deref(),
            Some("zh-CN")
        );
    }

    #[test]
    fn user_selected_english_language_is_preserved() {
        let conn = Connection::open_in_memory().expect("in-memory database should open");
        initialize_schema(&conn).expect("schema should initialize");
        save_setting(&conn, "ui.language", "en-US").expect("language should save");

        let loaded =
            load_or_initialize_language_with_conn(&conn, true, || Some("zh-Hans-CN".to_string()))
                .expect("language should load");

        assert_eq!(loaded, "en-US");
        assert_eq!(
            setting_string(&conn, "ui.language")
                .expect("language setting should load")
                .as_deref(),
            Some("en-US")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_apple_languages_output_reads_first_locale() {
        let output = r#"(
    "zh-Hans-CN",
    "en-CN"
)"#;

        assert_eq!(
            parse_macos_apple_languages(output).as_deref(),
            Some("zh-Hans-CN")
        );
    }

    #[test]
    fn gateway_request_logs_roundtrip_privacy_filter_metadata_only() {
        let conn = Connection::open_in_memory().expect("in-memory database should open");
        initialize_schema(&conn).expect("schema should initialize");
        let entry = GatewayRequestLogEntry {
            id: "request-1".to_string(),
            timestamp: "2026-06-18T12:00:00Z".to_string(),
            client: "Codex".to_string(),
            method: "POST".to_string(),
            path: "/v1/responses".to_string(),
            provider: Some("test".to_string()),
            model: Some("gpt".to_string()),
            status: 200,
            latency_ms: 42,
            error_summary: None,
            privacy_filter_mode: PrivacyFilterMode::Redact,
            privacy_filter_hit_count: 3,
            privacy_filter_action: PrivacyFilterAction::Redacted,
        };

        conn.execute(
            "INSERT INTO gateway_request_logs (
                id, timestamp, client, method, path, provider, model, status, latency_ms, error_summary,
                privacy_filter_mode, privacy_filter_hit_count, privacy_filter_action
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                entry.id,
                entry.timestamp,
                entry.client,
                entry.method,
                entry.path,
                entry.provider,
                entry.model,
                i64::from(entry.status),
                entry.latency_ms.to_string(),
                entry.error_summary,
                privacy_filter_mode_to_storage(&entry.privacy_filter_mode),
                entry.privacy_filter_hit_count as i64,
                privacy_filter_action_to_storage(&entry.privacy_filter_action),
            ],
        )
        .expect("request log should insert");

        let loaded = {
            let mut statement = conn
                .prepare(
                    "SELECT id, timestamp, client, method, path, provider, model, status,
                            latency_ms, error_summary, privacy_filter_mode,
                            privacy_filter_hit_count, privacy_filter_action
                     FROM gateway_request_logs
                     LIMIT 1",
                )
                .expect("request log should query");
            statement
                .query_row([], |row| {
                    let latency: String = row.get(8)?;
                    Ok(GatewayRequestLogEntry {
                        id: row.get(0)?,
                        timestamp: row.get(1)?,
                        client: row.get(2)?,
                        method: row.get(3)?,
                        path: row.get(4)?,
                        provider: row.get(5)?,
                        model: row.get(6)?,
                        status: row.get::<_, i64>(7)? as u16,
                        latency_ms: latency.parse::<u128>().unwrap_or(0),
                        error_summary: row.get(9)?,
                        privacy_filter_mode: privacy_filter_mode_from_storage(
                            row.get::<_, String>(10)?.as_str(),
                        ),
                        privacy_filter_hit_count: row.get::<_, i64>(11)?.max(0) as usize,
                        privacy_filter_action: privacy_filter_action_from_storage(
                            row.get::<_, String>(12)?.as_str(),
                        ),
                    })
                })
                .expect("request log should load")
        };

        assert_eq!(loaded.privacy_filter_mode, PrivacyFilterMode::Redact);
        assert_eq!(loaded.privacy_filter_hit_count, 3);
        assert_eq!(loaded.privacy_filter_action, PrivacyFilterAction::Redacted);
        let serialized = serde_json::to_string(&loaded).expect("entry should serialize");
        assert!(!serialized.contains("alice@example.com"));
        assert!(!serialized.contains("sk-test"));
    }

    #[test]
    fn codex_oauth_profiles_store_copy_and_delete_auth_json_in_sql() {
        let conn = Connection::open_in_memory().expect("in-memory database should open");
        initialize_schema(&conn).expect("schema should initialize");
        let auth_json = r#"{"tokens":{"access_token":"access","refresh_token":"refresh"}}"#;

        save_codex_oauth_profile_with_conn(&conn, "codex-oauth-a", auth_json)
            .expect("oauth profile should save");
        assert_eq!(
            load_codex_oauth_profile_with_conn(&conn, "codex-oauth-a")
                .expect("oauth profile should load")
                .as_deref(),
            Some(auth_json)
        );

        assert!(
            copy_codex_oauth_profile_with_conn(&conn, "codex-oauth-a", "codex-oauth-b")
                .expect("oauth profile should copy")
        );
        assert_eq!(
            load_codex_oauth_profile_with_conn(&conn, "codex-oauth-b")
                .expect("copied oauth profile should load")
                .as_deref(),
            Some(auth_json)
        );

        delete_codex_oauth_profile_with_conn(&conn, "codex-oauth-a")
            .expect("oauth profile should delete");
        assert!(load_codex_oauth_profile_with_conn(&conn, "codex-oauth-a")
            .expect("deleted oauth profile lookup should succeed")
            .is_none());
        assert_eq!(
            load_codex_oauth_profile_with_conn(&conn, "codex-oauth-b")
                .expect("copied oauth profile should remain")
                .as_deref(),
            Some(auth_json)
        );
    }

    #[test]
    fn backup_files_store_target_bytes_in_sql() {
        let conn = Connection::open_in_memory().expect("in-memory database should open");
        initialize_schema(&conn).expect("schema should initialize");
        let target_path = "C:/Users/example/.codex/config.toml";
        let content = b"model = \"gpt-5.5\"";

        save_backup_file_with_conn(&conn, "backup-a", target_path, Some(content))
            .expect("backup file should save");

        assert_eq!(
            load_backup_file_with_conn(&conn, "backup-a", target_path)
                .expect("backup file should load")
                .as_deref(),
            Some(content.as_slice())
        );

        save_backup_file_with_conn(&conn, "backup-a", "C:/Users/example/missing.toml", None)
            .expect("missing backup file entry should save");
        assert!(
            load_backup_file_with_conn(&conn, "backup-a", "C:/Users/example/missing.toml")
                .expect("missing backup file entry should load")
                .is_none()
        );
    }

    #[test]
    fn profile_order_updates_only_the_requested_app_and_mode() {
        let conn = Connection::open_in_memory().expect("in-memory database should open");
        initialize_schema(&conn).expect("schema should initialize");
        seed_profile_row(&conn, "codex-a", "Alpha", "codex", "config", 0);
        seed_profile_row(&conn, "codex-b", "Beta", "codex", "config", 1);
        seed_profile_row(&conn, "codex-gateway", "Gateway", "codex", "gateway", 2);
        seed_profile_row(&conn, "claude-a", "Claude", "claude", "config", 3);

        reorder_profiles_with_conn(
            &conn,
            "codex",
            &ProviderApplyMode::Config,
            &["codex-b".to_string(), "codex-a".to_string()],
        )
        .expect("profile order should save");

        assert_eq!(
            ordered_profile_ids_for_test(&conn, "codex", "config"),
            vec!["codex-b", "codex-a"]
        );
        assert_eq!(
            ordered_profile_ids_for_test(&conn, "codex", "gateway"),
            vec!["codex-gateway"]
        );
        assert_eq!(
            ordered_profile_ids_for_test(&conn, "claude", "config"),
            vec!["claude-a"]
        );
    }

    #[test]
    fn profile_order_can_include_builtin_profile_ids() {
        let conn = Connection::open_in_memory().expect("in-memory database should open");
        initialize_schema(&conn).expect("schema should initialize");
        seed_profile_row(&conn, "codex-a", "Alpha", "codex", "config", 0);
        seed_profile_row(&conn, "codex-b", "Beta", "codex", "config", 1);

        reorder_profiles_with_conn(
            &conn,
            "codex",
            &ProviderApplyMode::Config,
            &[
                "codex-b".to_string(),
                "builtin-official-codex".to_string(),
                "codex-a".to_string(),
            ],
        )
        .expect("profile order should accept builtin official profiles");

        assert_eq!(
            ordered_profile_ids_for_test(&conn, "codex", "config"),
            vec!["codex-b", "builtin-official-codex", "codex-a"]
        );
    }

    #[test]
    fn profiles_table_has_icon_column() {
        let conn = Connection::open_in_memory().expect("in-memory database should open");
        initialize_schema(&conn).expect("schema should initialize");

        assert!(table_has_column(&conn, "profiles", "icon")
            .expect("profile icon column should be inspectable"));
    }

    #[test]
    fn profiles_table_has_remark_column() {
        let conn = Connection::open_in_memory().expect("in-memory database should open");
        initialize_schema(&conn).expect("schema should initialize");

        assert!(table_has_column(&conn, "profiles", "remark")
            .expect("profile remark column should be inspectable"));
    }

    #[test]
    fn profiles_roundtrip_optional_remark() {
        let conn = Connection::open_in_memory().expect("in-memory database should open");
        initialize_schema(&conn).expect("schema should initialize");
        let profile = ProfileDraft {
            id: "profile-with-remark".to_string(),
            name: "Remarked".to_string(),
            icon: Some("R".to_string()),
            remark: Some("Work account".to_string()),
            app: "codex".to_string(),
            is_builtin: false,
            mode: ProviderApplyMode::Config,
            provider: "openai".to_string(),
            protocol: "openai-chat-completions".to_string(),
            model: String::new(),
            base_url: "https://example.test/v1".to_string(),
            auth_ref: None,
            created_at: None,
            updated_at: None,
            last_test_status: Some("pending".to_string()),
            usage_enabled: false,
            sort_order: 0,
        };

        conn.execute(
            "INSERT INTO profiles (
                id, name, icon, remark, app, mode, provider, protocol, model, base_url, auth_ref,
                created_at, updated_at, last_test_status, sort_order
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                profile.id,
                profile.name,
                profile.icon,
                profile.remark,
                profile.app,
                mode_to_storage(&profile.mode),
                profile.provider,
                profile.protocol,
                profile.model,
                profile.base_url,
                profile.auth_ref,
                profile.created_at,
                profile.updated_at,
                profile.last_test_status,
                profile.sort_order,
            ],
        )
        .expect("profile row should insert");

        let mut statement = conn
            .prepare("SELECT remark FROM profiles WHERE id = 'profile-with-remark'")
            .expect("profile should query");
        let remark = statement
            .query_row([], |row| row.get::<_, Option<String>>(0))
            .expect("remark should load");
        assert_eq!(remark.as_deref(), Some("Work account"));
    }

    fn seed_profile_row(
        conn: &Connection,
        id: &str,
        name: &str,
        app: &str,
        mode: &str,
        sort_order: i64,
    ) {
        conn.execute(
            "INSERT INTO profiles (
                id, name, app, mode, provider, protocol, model, base_url, auth_ref,
                created_at, updated_at, last_test_status, sort_order
             ) VALUES (?1, ?2, ?3, ?4, 'openai', 'openai-chat-completions', '', 'https://example.test/v1', NULL,
                NULL, NULL, 'pending', ?5)",
            params![id, name, app, mode, sort_order],
        )
        .expect("profile row should insert");
    }

    fn ordered_profile_ids_for_test(conn: &Connection, app: &str, mode: &str) -> Vec<String> {
        let mode = match mode {
            "config" => ProviderApplyMode::Config,
            "gateway" => ProviderApplyMode::Gateway,
            value => panic!("unknown profile mode '{value}'"),
        };
        let stored =
            load_profile_order_with_conn(conn, app, &mode).expect("profile order should load");
        if !stored.is_empty() {
            return stored;
        }

        let mut statement = conn
            .prepare(
                "SELECT id FROM profiles
                 WHERE app = ?1 AND mode = ?2
                 ORDER BY sort_order ASC, name ASC, id ASC",
            )
            .expect("profile fallback query should prepare");
        statement
            .query_map(params![app, mode_to_storage(&mode)], |row| {
                row.get::<_, String>(0)
            })
            .expect("profile fallback query should run")
            .map(|row| row.expect("profile id should read"))
            .collect()
    }
}
