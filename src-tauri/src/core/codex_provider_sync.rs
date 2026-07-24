use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
#[cfg(test)]
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_PROVIDER: &str = "openai";
const SESSION_DIRS: [&str; 2] = ["sessions", "archived_sessions"];

#[derive(Debug, Default)]
struct RolloutRewrite {
    next_text: String,
    changed: bool,
    thread_id: Option<String>,
    cwd: Option<String>,
    has_user_event: bool,
    original_providers: HashSet<String>,
}

#[derive(Debug, Clone)]
struct PendingRewrite {
    path: PathBuf,
    original_text: String,
    next_text: String,
    original_mtime: Option<SystemTime>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSyncStatus {
    Disabled,
    Skipped,
    #[default]
    Synced,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSyncReport {
    pub status: ProviderSyncStatus,
    pub message: String,
    pub target_provider: String,
    pub changed_session_files: usize,
    pub skipped_locked_rollout_files: Vec<PathBuf>,
    pub sqlite_rows_updated: usize,
    pub sqlite_provider_rows_updated: usize,
    pub sqlite_user_event_rows_updated: usize,
    pub sqlite_cwd_rows_updated: usize,
    pub updated_workspace_roots: usize,
    pub encrypted_content_warning: Option<String>,
    pub backup_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Default)]
struct SqliteUpdateCounts {
    provider: usize,
    user_event: usize,
    cwd: usize,
}

impl SqliteUpdateCounts {
    fn total(self) -> usize {
        self.provider + self.user_event + self.cwd
    }

    fn add(&mut self, other: Self) {
        self.provider += other.provider;
        self.user_event += other.user_event;
        self.cwd += other.cwd;
    }
}

pub fn run_default_provider_sync() -> ProviderSyncReport {
    run_provider_sync_with_target(None, None)
}

fn run_provider_sync_with_target(
    codex_home: Option<&Path>,
    explicit_target_provider: Option<&str>,
) -> ProviderSyncReport {
    let home = match codex_home {
        Some(path) => path.to_path_buf(),
        None => match codex_home_dir() {
            Ok(path) => path,
            Err(error) => return skipped_report(DEFAULT_PROVIDER, error),
        },
    };
    if !home.exists() {
        return skipped_report(
            DEFAULT_PROVIDER,
            format!("Codex home does not exist: {}", home.display()),
        );
    }

    let target_provider =
        match resolve_target_provider(&home.join("config.toml"), explicit_target_provider) {
            Ok(provider) => provider,
            Err(error) => return skipped_report(DEFAULT_PROVIDER, error),
        };
    let lock_dir = home.join("tmp").join("provider-sync.lock");
    if let Err(error) = acquire_lock(&lock_dir) {
        return skipped_report(
            &target_provider,
            format!("Provider sync lock is unavailable: {error}"),
        );
    }
    let result = run_provider_sync_locked(&home, &target_provider);
    let _ = release_lock(&lock_dir);
    result.unwrap_or_else(|error| skipped_report(&target_provider, error))
}

fn skipped_report(target_provider: &str, message: impl Into<String>) -> ProviderSyncReport {
    ProviderSyncReport {
        status: ProviderSyncStatus::Skipped,
        message: message.into(),
        target_provider: target_provider.to_string(),
        ..ProviderSyncReport::default()
    }
}

fn run_provider_sync_locked(
    home: &Path,
    target_provider: &str,
) -> Result<ProviderSyncReport, String> {
    let mut rewrites = Vec::new();
    let mut user_event_thread_ids = HashSet::new();
    let mut cwd_by_thread_id = HashMap::new();
    let mut skipped_locked_rollout_files = Vec::new();
    let mut encrypted_content_by_provider = HashMap::<String, usize>::new();
    let projectless_thread_ids =
        load_projectless_thread_ids(&home.join(".codex-global-state.json"))?;

    for path in rollout_files(home)? {
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if is_locked_io_error(&error) => {
                skipped_locked_rollout_files.push(path);
                continue;
            }
            Err(error) => {
                return Err(format!(
                    "Failed to read Codex session file {}: {error}",
                    path.display()
                ))
            }
        };
        let rewrite = rewrite_rollout_session_meta(&text, target_provider)?;
        if text.contains("encrypted_content") {
            for provider in &rewrite.original_providers {
                *encrypted_content_by_provider
                    .entry(provider.clone())
                    .or_default() += 1;
            }
        }
        if let Some(thread_id) = rewrite.thread_id.as_ref() {
            if rewrite.has_user_event {
                user_event_thread_ids.insert(thread_id.clone());
            }
            if !projectless_thread_ids.contains(thread_id) {
                if let Some(cwd) = rewrite.cwd.as_ref() {
                    cwd_by_thread_id.insert(thread_id.clone(), cwd.clone());
                }
            }
        }
        if rewrite.changed {
            let original_mtime = fs::metadata(&path).and_then(|meta| meta.modified()).ok();
            rewrites.push(PendingRewrite {
                path,
                original_text: text,
                next_text: rewrite.next_text,
                original_mtime,
            });
        }
    }

    let sqlite_rows = count_sqlite_updates(
        home,
        target_provider,
        &user_event_thread_ids,
        &cwd_by_thread_id,
    )?;
    let workspace_updates = count_global_state_updates(&home.join(".codex-global-state.json"))?;
    let encrypted_content_warning =
        encrypted_content_warning(&encrypted_content_by_provider, target_provider);
    if rewrites.is_empty() && sqlite_rows.total() == 0 && workspace_updates == 0 {
        return Ok(ProviderSyncReport {
            status: ProviderSyncStatus::Synced,
            message: "History provider data is already synchronized.".to_string(),
            target_provider: target_provider.to_string(),
            skipped_locked_rollout_files,
            encrypted_content_warning,
            ..ProviderSyncReport::default()
        });
    }

    let backup_dir = create_backup(home, target_provider, &rewrites)?;
    let mut applied = Vec::new();
    for rewrite in &rewrites {
        match fs::write(&rewrite.path, &rewrite.next_text) {
            Ok(()) => {
                restore_file_mtime(&rewrite.path, rewrite.original_mtime);
                applied.push(rewrite.clone());
            }
            Err(error) if is_locked_io_error(&error) => {
                skipped_locked_rollout_files.push(rewrite.path.clone());
            }
            Err(error) => {
                restore_rewrites(&applied);
                return Err(format!(
                    "Failed to update Codex session file {}: {error}",
                    rewrite.path.display()
                ));
            }
        }
    }

    let sqlite_updates = match apply_sqlite_updates(
        home,
        target_provider,
        &user_event_thread_ids,
        &cwd_by_thread_id,
    ) {
        Ok(rows) => rows,
        Err(error) => {
            restore_rewrites(&applied);
            let _ = restore_database_backup(home, &backup_dir);
            return Err(error);
        }
    };

    let updated_workspace_roots =
        match apply_global_state_update(&home.join(".codex-global-state.json")) {
            Ok(count) => count,
            Err(error) => {
                restore_rewrites(&applied);
                let _ = restore_database_backup(home, &backup_dir);
                let _ = restore_global_state_backup(home, &backup_dir);
                return Err(error);
            }
        };
    prune_backups(&home.join("backups_state").join("provider-sync"));

    Ok(ProviderSyncReport {
        status: ProviderSyncStatus::Synced,
        message: "History provider synchronization completed.".to_string(),
        target_provider: target_provider.to_string(),
        changed_session_files: applied.len(),
        skipped_locked_rollout_files,
        sqlite_rows_updated: sqlite_updates.total(),
        sqlite_provider_rows_updated: sqlite_updates.provider,
        sqlite_user_event_rows_updated: sqlite_updates.user_event,
        sqlite_cwd_rows_updated: sqlite_updates.cwd,
        updated_workspace_roots,
        encrypted_content_warning,
        backup_dir: Some(backup_dir),
    })
}

fn codex_home_dir() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|home| home.join(".codex"))
        .ok_or_else(|| "Could not locate the user home directory.".to_string())
}

fn read_current_provider(path: &Path) -> String {
    let Ok(text) = fs::read_to_string(path) else {
        return DEFAULT_PROVIDER.to_string();
    };
    let provider = root_toml_string_value(&text, "model_provider").unwrap_or_default();
    if provider.trim().is_empty() {
        DEFAULT_PROVIDER.to_string()
    } else {
        provider
    }
}

fn resolve_target_provider(path: &Path, explicit: Option<&str>) -> Result<String, String> {
    let Some(value) = explicit.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(read_current_provider(path));
    };
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        Ok(value.to_string())
    } else {
        Err(format!("Invalid history provider target: {value:?}"))
    }
}

fn root_toml_string_value(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let stripped = line.trim();
        if stripped.is_empty() || stripped.starts_with('#') {
            continue;
        }
        if stripped.starts_with('[') {
            break;
        }
        let Some(remainder) = stripped.strip_prefix(key) else {
            continue;
        };
        let Some(raw) = remainder.trim_start().strip_prefix('=') else {
            continue;
        };
        return parse_toml_string(raw.trim_start());
    }
    None
}

fn parse_toml_string(raw: &str) -> Option<String> {
    let quote = raw.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let mut value = String::new();
    let mut escaping = false;
    for ch in raw[quote.len_utf8()..].chars() {
        if quote == '"' && escaping {
            value.push(ch);
            escaping = false;
        } else if quote == '"' && ch == '\\' {
            escaping = true;
        } else if ch == quote {
            return Some(value);
        } else {
            value.push(ch);
        }
    }
    None
}

fn acquire_lock(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path.parent().unwrap_or_else(|| Path::new(".")))
        .map_err(|err| format!("Failed to create provider sync lock parent: {err}"))?;
    fs::create_dir(path).map_err(|err| {
        format!(
            "Codex provider sync is already running or locked at {}: {err}",
            path.display()
        )
    })?;
    fs::write(
        path.join("owner.json"),
        json!({"pid": std::process::id(), "startedAt": now_secs()}).to_string(),
    )
    .map_err(|err| format!("Failed to write provider sync lock owner: {err}"))
}

fn release_lock(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_dir_all(path)
            .map_err(|err| format!("Failed to release provider sync lock: {err}"))?;
    }
    Ok(())
}

fn rollout_files(home: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for dirname in SESSION_DIRS {
        let root = home.join(dirname);
        if root.exists() {
            collect_rollout_files(&root, &mut files)?;
        }
    }
    files.sort();
    Ok(files)
}

fn collect_rollout_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(root).map_err(|err| {
        format!(
            "Failed to read Codex session directory {}: {err}",
            root.display()
        )
    })? {
        let path = entry
            .map_err(|err| format!("Failed to read Codex session entry: {err}"))?
            .path();
        if path.is_dir() {
            collect_rollout_files(&path, files)?;
        } else if path
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn rewrite_rollout_session_meta(
    text: &str,
    target_provider: &str,
) -> Result<RolloutRewrite, String> {
    let mut rewrite = RolloutRewrite {
        has_user_event: text.contains("\"user_message\"") || text.contains("\"user_input\""),
        ..RolloutRewrite::default()
    };
    for segment in text.split_inclusive('\n') {
        let (line, ending) = split_line_ending(segment);
        let mut next_line = line.to_string();
        if !line.trim().is_empty() {
            if let Ok(mut record) = serde_json::from_str::<Value>(line) {
                if record.get("type").and_then(Value::as_str) == Some("session_meta") {
                    let Some(payload) = record.get_mut("payload").and_then(Value::as_object_mut)
                    else {
                        rewrite.next_text.push_str(&next_line);
                        rewrite.next_text.push_str(ending);
                        continue;
                    };
                    if rewrite.thread_id.is_none() {
                        rewrite.thread_id = payload
                            .get("id")
                            .and_then(Value::as_str)
                            .map(ToString::to_string);
                    }
                    if rewrite.cwd.is_none() {
                        rewrite.cwd = payload
                            .get("cwd")
                            .and_then(Value::as_str)
                            .and_then(to_desktop_workspace_path);
                    }
                    if let Some(provider) = payload
                        .get("model_provider")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|provider| !provider.is_empty())
                    {
                        rewrite.original_providers.insert(provider.to_string());
                    }
                    if payload.get("model_provider").and_then(Value::as_str)
                        != Some(target_provider)
                    {
                        payload.insert("model_provider".to_string(), json!(target_provider));
                        next_line = serde_json::to_string(&record).map_err(|err| {
                            format!("Failed to encode Codex session metadata: {err}")
                        })?;
                        rewrite.changed = true;
                    }
                }
            }
        }
        rewrite.next_text.push_str(&next_line);
        rewrite.next_text.push_str(ending);
    }
    Ok(rewrite)
}

fn split_line_ending(segment: &str) -> (&str, &str) {
    if let Some(line) = segment.strip_suffix("\r\n") {
        (line, "\r\n")
    } else if let Some(line) = segment.strip_suffix('\n') {
        (line, "\n")
    } else {
        (segment, "")
    }
}

fn to_desktop_workspace_path(value: &str) -> Option<String> {
    let stripped = value.trim();
    if stripped.is_empty() {
        return None;
    }
    let lower = stripped.to_ascii_lowercase();
    if lower.starts_with(r"\\?\unc\") {
        return Some(format!(r"\\{}", stripped[8..].replace('/', r"\")));
    }
    if let Some(path) = stripped.strip_prefix(r"\\?\") {
        return Some(path.replace('\\', "/"));
    }
    Some(stripped.to_string())
}

fn is_locked_io_error(error: &std::io::Error) -> bool {
    matches!(error.kind(), std::io::ErrorKind::PermissionDenied)
        || matches!(error.raw_os_error(), Some(32 | 33))
}

fn create_backup(
    home: &Path,
    target_provider: &str,
    rewrites: &[PendingRewrite],
) -> Result<PathBuf, String> {
    let root = home.join("backups_state").join("provider-sync");
    fs::create_dir_all(&root)
        .map_err(|err| format!("Failed to create provider sync backup directory: {err}"))?;
    let mut dir = root.join(Utc::now().format("%Y%m%d%H%M%S").to_string());
    let mut suffix = 0;
    while dir.exists() {
        suffix += 1;
        dir = root.join(format!("{}-{suffix}", Utc::now().format("%Y%m%d%H%M%S")));
    }
    fs::create_dir_all(&dir)
        .map_err(|err| format!("Failed to create provider sync backup: {err}"))?;
    for name in [
        "config.toml",
        ".codex-global-state.json",
        ".codex-global-state.json.bak",
    ] {
        let source = home.join(name);
        if source.exists() {
            fs::copy(&source, dir.join(name))
                .map_err(|err| format!("Failed to back up {}: {err}", source.display()))?;
        }
    }
    let mut database_files = Vec::new();
    for database in codex_session_db_paths(home) {
        for source in sqlite_file_set(&database) {
            if !source.exists() {
                continue;
            }
            let relative = source.strip_prefix(home).unwrap_or(&source);
            let target = dir.join("db").join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|err| format!("Failed to create database backup directory: {err}"))?;
            }
            fs::copy(&source, &target).map_err(|err| {
                format!("Failed to back up SQLite file {}: {err}", source.display())
            })?;
            database_files.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    let manifest = rewrites
        .iter()
        .map(|rewrite| {
            let original_session_meta_lines = rewrite
                .original_text
                .lines()
                .filter(|line| {
                    serde_json::from_str::<Value>(line)
                        .ok()
                        .and_then(|value| {
                            value
                                .get("type")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        })
                        .as_deref()
                        == Some("session_meta")
                })
                .collect::<Vec<_>>();
            json!({
                "path": rewrite.path.to_string_lossy(),
                "originalSessionMetaLines": original_session_meta_lines
            })
        })
        .collect::<Vec<_>>();
    fs::write(
        dir.join("session-meta-backup.json"),
        serde_json::to_string_pretty(&manifest).map_err(|err| err.to_string())?,
    )
    .map_err(|err| format!("Failed to write provider sync backup manifest: {err}"))?;
    fs::write(
        dir.join("metadata.json"),
        serde_json::to_string_pretty(&json!({
            "version": 1,
            "namespace": "provider-sync",
            "codexHome": home.to_string_lossy(),
            "targetProvider": target_provider,
            "createdAt": Utc::now().to_rfc3339(),
            "databaseFiles": database_files,
            "changedSessionFiles": rewrites.len(),
            "managedBy": "CodeStudio Lite provider sync"
        }))
        .map_err(|err| err.to_string())?,
    )
    .map_err(|err| format!("Failed to write provider sync backup metadata: {err}"))?;
    Ok(dir)
}

fn sqlite_file_set(database: &Path) -> Vec<PathBuf> {
    let raw = database.as_os_str().to_string_lossy();
    vec![
        database.to_path_buf(),
        PathBuf::from(format!("{raw}-wal")),
        PathBuf::from(format!("{raw}-shm")),
    ]
}

fn restore_database_backup(home: &Path, backup_dir: &Path) -> Result<(), String> {
    let root = backup_dir.join("db");
    if !root.exists() {
        return Ok(());
    }
    for source in recursive_files(&root)? {
        let relative = source.strip_prefix(&root).map_err(|err| err.to_string())?;
        let target = home.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::copy(&source, &target).map_err(|err| {
            format!(
                "Failed to restore SQLite backup {}: {err}",
                source.display()
            )
        })?;
    }
    Ok(())
}

fn restore_global_state_backup(home: &Path, backup_dir: &Path) -> Result<(), String> {
    for name in [".codex-global-state.json", ".codex-global-state.json.bak"] {
        let source = backup_dir.join(name);
        if source.exists() {
            fs::copy(&source, home.join(name)).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn recursive_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|err| format!("Failed to read {}: {err}", directory.display()))?
        {
            let path = entry.map_err(|err| err.to_string())?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.is_file() {
                files.push(path);
            }
        }
    }
    Ok(files)
}

fn restore_file_mtime(path: &Path, mtime: Option<SystemTime>) {
    let Some(mtime) = mtime else { return };
    let Ok(file) = fs::File::options().write(true).open(path) else {
        return;
    };
    let times = fs::FileTimes::new().set_modified(mtime);
    let _ = file.set_times(times);
}

fn restore_rewrites(rewrites: &[PendingRewrite]) {
    for rewrite in rewrites {
        let _ = fs::write(&rewrite.path, &rewrite.original_text);
        restore_file_mtime(&rewrite.path, rewrite.original_mtime);
    }
}

fn prune_backups(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut managed = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            fs::read_to_string(path.join("metadata.json"))
                .ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                .and_then(|value| {
                    value
                        .get("managedBy")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .as_deref()
                == Some("CodeStudio Lite provider sync")
        })
        .collect::<Vec<_>>();
    managed.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
    for path in managed.into_iter().skip(5) {
        let _ = fs::remove_dir_all(path);
    }
}

fn codex_session_db_paths(home: &Path) -> Vec<PathBuf> {
    let mut paths = sqlite_dir_session_dbs(home);
    let legacy = home.join("state_5.sqlite");
    if !paths.iter().any(|path| path == &legacy) {
        paths.push(legacy);
    }
    paths
}

fn sqlite_dir_session_dbs(home: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(home.join("sqlite")) else {
        return Vec::new();
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            matches!(
                path.extension().and_then(OsStr::to_str),
                Some("db") | Some("sqlite") | Some("sqlite3")
            )
        })
        .filter(|path| has_any_table(path, &["threads", "automation_runs", "inbox_items"]))
        .collect::<Vec<_>>();
    paths.sort_by_key(|path| {
        (
            path.file_name()
                .map(|name| name != OsStr::new("codex-dev.db"))
                .unwrap_or(true),
            path.file_name().map(|name| name.to_os_string()),
        )
    });
    paths
}

fn has_any_table(path: &Path, tables: &[&str]) -> bool {
    let Ok(db) = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return false;
    };
    tables.iter().any(|table| {
        db.query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
            [table],
            |_| Ok(()),
        )
        .is_ok()
    })
}

fn table_columns(db: &Connection, table: &str) -> Result<HashSet<String>, String> {
    let mut statement = db
        .prepare(&format!(
            "PRAGMA table_info(\"{}\")",
            table.replace('"', "\"\"")
        ))
        .map_err(|err| format!("Failed to inspect Codex SQLite schema: {err}"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|err| format!("Failed to inspect Codex SQLite schema: {err}"))?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|err| format!("Failed to read Codex SQLite schema: {err}"))?;
    Ok(columns)
}

fn count_sqlite_updates(
    home: &Path,
    target_provider: &str,
    user_event_thread_ids: &HashSet<String>,
    cwd_by_thread_id: &HashMap<String, String>,
) -> Result<SqliteUpdateCounts, String> {
    let mut total = SqliteUpdateCounts::default();
    for path in codex_session_db_paths(home) {
        total.add(count_sqlite_updates_for_path(
            &path,
            target_provider,
            user_event_thread_ids,
            cwd_by_thread_id,
        )?);
    }
    Ok(total)
}

fn count_sqlite_updates_for_path(
    path: &Path,
    target_provider: &str,
    user_event_thread_ids: &HashSet<String>,
    cwd_by_thread_id: &HashMap<String, String>,
) -> Result<SqliteUpdateCounts, String> {
    if !path.exists() {
        return Ok(SqliteUpdateCounts::default());
    }
    let db = Connection::open(path).map_err(|err| {
        format!(
            "Failed to open Codex SQLite database {}: {err}",
            path.display()
        )
    })?;
    let columns = table_columns(&db, "threads")?;
    if !columns.contains("model_provider") {
        return Ok(SqliteUpdateCounts::default());
    }
    let provider =
        db.query_row(
            "SELECT COUNT(*) FROM threads WHERE COALESCE(model_provider, '') <> ?1",
            [target_provider],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|err| format!("Failed to count Codex SQLite updates: {err}"))? as usize;
    let mut user_event = 0;
    if columns.contains("has_user_event") {
        for thread_id in user_event_thread_ids {
            user_event += db
                .query_row(
                    "SELECT COUNT(*) FROM threads WHERE id = ?1 AND COALESCE(has_user_event, 0) <> 1",
                    [thread_id],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|err| format!("Failed to count Codex SQLite user event updates: {err}"))?
                as usize;
        }
    }
    let mut cwd_updates = 0;
    if columns.contains("cwd") {
        for (thread_id, expected_cwd) in cwd_by_thread_id {
            cwd_updates += db
                .query_row(
                    "SELECT COUNT(*) FROM threads WHERE id = ?1 AND COALESCE(cwd, '') <> ?2",
                    (thread_id, expected_cwd),
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|err| format!("Failed to count Codex SQLite cwd updates: {err}"))?
                as usize;
        }
    }
    Ok(SqliteUpdateCounts {
        provider,
        user_event,
        cwd: cwd_updates,
    })
}

fn apply_sqlite_updates(
    home: &Path,
    target_provider: &str,
    user_event_thread_ids: &HashSet<String>,
    cwd_by_thread_id: &HashMap<String, String>,
) -> Result<SqliteUpdateCounts, String> {
    let mut total = SqliteUpdateCounts::default();
    for path in codex_session_db_paths(home) {
        total.add(apply_sqlite_updates_for_path(
            &path,
            target_provider,
            user_event_thread_ids,
            cwd_by_thread_id,
        )?);
    }
    Ok(total)
}

fn apply_sqlite_updates_for_path(
    path: &Path,
    target_provider: &str,
    user_event_thread_ids: &HashSet<String>,
    cwd_by_thread_id: &HashMap<String, String>,
) -> Result<SqliteUpdateCounts, String> {
    if !path.exists() {
        return Ok(SqliteUpdateCounts::default());
    }
    let mut db = Connection::open(path).map_err(|err| {
        format!(
            "Failed to open Codex SQLite database {}: {err}",
            path.display()
        )
    })?;
    let columns = table_columns(&db, "threads")?;
    if !columns.contains("model_provider") {
        return Ok(SqliteUpdateCounts::default());
    }
    let tx = db
        .transaction()
        .map_err(|err| format!("Failed to start Codex SQLite update: {err}"))?;
    let provider = tx
        .execute(
            "UPDATE threads SET model_provider = ?1 WHERE COALESCE(model_provider, '') <> ?1",
            [target_provider],
        )
        .map_err(|err| format!("Failed to update Codex SQLite provider rows: {err}"))?;
    let mut user_event = 0;
    if columns.contains("has_user_event") {
        for thread_id in user_event_thread_ids {
            user_event += tx
                .execute(
                    "UPDATE threads SET has_user_event = 1 WHERE id = ?1 AND COALESCE(has_user_event, 0) <> 1",
                    [thread_id],
                )
                .map_err(|err| format!("Failed to update Codex SQLite user event rows: {err}"))?;
        }
    }
    let mut cwd_count = 0;
    if columns.contains("cwd") {
        for (thread_id, cwd) in cwd_by_thread_id {
            cwd_count += tx
                .execute(
                    "UPDATE threads SET cwd = ?1 WHERE id = ?2 AND COALESCE(cwd, '') <> ?1",
                    (cwd, thread_id),
                )
                .map_err(|err| format!("Failed to update Codex SQLite cwd rows: {err}"))?;
        }
    }
    tx.commit()
        .map_err(|err| format!("Failed to commit Codex SQLite update: {err}"))?;
    Ok(SqliteUpdateCounts {
        provider,
        user_event,
        cwd: cwd_count,
    })
}

fn load_global_state(path: &Path) -> Result<Map<String, Value>, String> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let text = fs::read_to_string(path)
        .map_err(|err| format!("Failed to read Codex global state: {err}"))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|err| format!("Failed to parse Codex global state: {err}"))?;
    Ok(value.as_object().cloned().unwrap_or_default())
}

fn load_projectless_thread_ids(path: &Path) -> Result<HashSet<String>, String> {
    let state = load_global_state(path)?;
    Ok(state
        .get("projectless-thread-ids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(ToString::to_string)
        .collect())
}

fn normalized_path_list(value: &Value) -> Vec<String> {
    let values = if let Some(items) = value.as_array() {
        items.iter().filter_map(Value::as_str).collect::<Vec<_>>()
    } else {
        value.as_str().into_iter().collect::<Vec<_>>()
    };
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for value in values {
        let Some(path) = to_desktop_workspace_path(value) else {
            continue;
        };
        let comparable = path
            .replace('/', r"\")
            .trim_end_matches('\\')
            .to_ascii_lowercase();
        if seen.insert(comparable) {
            normalized.push(path);
        }
    }
    normalized
}

fn normalized_path_object(value: &Map<String, Value>) -> Map<String, Value> {
    let mut normalized = Map::new();
    for (key, item) in value {
        let key = to_desktop_workspace_path(key).unwrap_or_else(|| key.clone());
        normalized.insert(key, item.clone());
    }
    normalized
}

fn normalized_global_state_fields(state: &Map<String, Value>) -> Map<String, Value> {
    let mut updates = Map::new();
    for key in ["electron-saved-workspace-roots", "project-order"] {
        if let Some(value) = state.get(key) {
            updates.insert(key.to_string(), json!(normalized_path_list(value)));
        }
    }
    if let Some(value) = state.get("active-workspace-roots") {
        let paths = normalized_path_list(value);
        let normalized = if value.is_array() {
            json!(paths)
        } else {
            paths
                .first()
                .map(|path| json!(path))
                .unwrap_or_else(|| value.clone())
        };
        updates.insert("active-workspace-roots".to_string(), normalized);
    }
    if let Some(labels) = state
        .get("electron-workspace-root-labels")
        .and_then(Value::as_object)
    {
        updates.insert(
            "electron-workspace-root-labels".to_string(),
            Value::Object(normalized_path_object(labels)),
        );
    }
    if let Some(targets) = state
        .get("open-in-target-preferences")
        .and_then(Value::as_object)
    {
        let mut normalized_targets = targets.clone();
        if let Some(per_path) = targets.get("perPath").and_then(Value::as_object) {
            normalized_targets.insert(
                "perPath".to_string(),
                Value::Object(normalized_path_object(per_path)),
            );
        }
        updates.insert(
            "open-in-target-preferences".to_string(),
            Value::Object(normalized_targets),
        );
    }
    updates
}

fn count_global_state_updates(path: &Path) -> Result<usize, String> {
    let state = load_global_state(path)?;
    Ok(normalized_global_state_fields(&state)
        .iter()
        .filter(|(key, value)| state.get(*key) != Some(*value))
        .count())
}

fn apply_global_state_update(path: &Path) -> Result<usize, String> {
    let mut state = load_global_state(path)?;
    let updates = normalized_global_state_fields(&state);
    let changed = updates
        .iter()
        .filter(|(key, value)| state.get(*key) != Some(*value))
        .count();
    if changed == 0 {
        return Ok(0);
    }
    for (key, value) in updates {
        state.insert(key, value);
    }
    let text = serde_json::to_string_pretty(&Value::Object(state))
        .map_err(|err| format!("Failed to encode Codex global state: {err}"))?;
    fs::write(path, &text).map_err(|err| format!("Failed to update Codex global state: {err}"))?;
    if let Some(parent) = path.parent() {
        fs::write(parent.join(".codex-global-state.json.bak"), text)
            .map_err(|err| format!("Failed to update Codex global-state backup: {err}"))?;
    }
    Ok(changed)
}

fn encrypted_content_warning(
    counts: &HashMap<String, usize>,
    target_provider: &str,
) -> Option<String> {
    let mut providers = counts
        .iter()
        .filter(|(provider, count)| provider.as_str() != target_provider && **count > 0)
        .map(|(provider, _)| provider.clone())
        .collect::<Vec<_>>();
    if providers.is_empty() {
        return None;
    }
    providers.sort();
    let total = counts.values().sum::<usize>();
    Some(format!(
        "{total} session file(s) contain encrypted content created by {}. Their visible metadata was synchronized to {target_provider}, but continuing or compacting them can fail with invalid_encrypted_content; use the original provider/account or start a new session for reliable continuation.",
        providers.join(", ")
    ))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    struct TestHome(PathBuf);

    impl TestHome {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "codestudio-provider-sync-{}-{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            fs::create_dir_all(&path).expect("create test home");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestHome {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_test_rollout(home: &Path, provider: &str, id: &str, cwd: &str) -> PathBuf {
        let path = home.join("sessions/2026/rollout-test.jsonl");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\",\"cwd\":\"{cwd}\",\"model_provider\":\"{provider}\"}}}}\n{{\"type\":\"user_message\",\"payload\":{{\"text\":\"hello\"}}}}\n"
            ),
        )
        .unwrap();
        path
    }

    fn create_test_db(path: &Path, provider: &str, cwd: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let db = Connection::open(path).unwrap();
        db.execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, model_provider TEXT, has_user_event INTEGER, cwd TEXT)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO threads VALUES ('thread-a', ?1, 0, ?2)",
            (provider, cwd),
        )
        .unwrap();
    }

    #[test]
    fn rewrite_session_meta_provider_preserves_non_meta_lines() {
        let input = concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-a\",\"cwd\":\"C:/repo\",\"model_provider\":\"old\"}}\n",
            "{\"type\":\"user_message\",\"payload\":{\"text\":\"hello\"}}\n"
        );
        let rewrite = rewrite_rollout_session_meta(input, "new-provider").expect("rewrite");

        assert!(rewrite.changed);
        assert_eq!(rewrite.thread_id.as_deref(), Some("thread-a"));
        assert!(rewrite.has_user_event);
        assert!(rewrite
            .next_text
            .contains("\"model_provider\":\"new-provider\""));
        assert!(rewrite.next_text.contains("\"type\":\"user_message\""));
    }

    #[test]
    fn root_toml_provider_defaults_to_openai_when_missing() {
        assert_eq!(parse_toml_string("\"custom\"").as_deref(), Some("custom"));
        assert_eq!(
            root_toml_string_value(
                "# managed by the user\n\nmodel_provider = \"custom\"\n[model_providers.custom]\n",
                "model_provider"
            )
            .as_deref(),
            Some("custom")
        );
    }

    #[test]
    fn live_rollout_scan_only_skips_lock_errors() {
        assert!(is_locked_io_error(&std::io::Error::from_raw_os_error(32)));
        assert!(!is_locked_io_error(&std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid UTF-8",
        )));
    }

    #[test]
    fn sync_preserves_projectless_cwd_and_normalizes_workspace_state() {
        let home = TestHome::new();
        fs::write(
            home.path().join("config.toml"),
            "model_provider = \"custom\"\n",
        )
        .unwrap();
        write_test_rollout(home.path(), "openai", "thread-a", r"\\?\C:\workspace");
        create_test_db(&home.path().join("state_5.sqlite"), "openai", "C:/original");
        fs::write(
            home.path().join(".codex-global-state.json"),
            json!({
                "projectless-thread-ids": ["thread-a"],
                "electron-saved-workspace-roots": [r"\\?\C:\workspace", "C:/workspace"],
                "project-order": [r"\\?\C:\workspace"],
                "active-workspace-roots": r"\\?\C:\workspace",
                "electron-workspace-root-labels": {r"\\?\C:\workspace": "Workspace"},
                "open-in-target-preferences": {"perPath": {r"\\?\C:\workspace": "terminal"}}
            })
            .to_string(),
        )
        .unwrap();

        let result = run_provider_sync_with_target(Some(home.path()), Some("custom"));

        assert_eq!(result.status, ProviderSyncStatus::Synced);
        assert_eq!(result.sqlite_cwd_rows_updated, 0);
        let db = Connection::open(home.path().join("state_5.sqlite")).unwrap();
        let cwd: String = db
            .query_row("SELECT cwd FROM threads WHERE id = 'thread-a'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(cwd, "C:/original");
        let state: Value = serde_json::from_str(
            &fs::read_to_string(home.path().join(".codex-global-state.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            state["electron-saved-workspace-roots"],
            json!(["C:/workspace"])
        );
        assert_eq!(state["active-workspace-roots"], json!("C:/workspace"));
        assert_eq!(
            state["open-in-target-preferences"]["perPath"],
            json!({"C:/workspace": "terminal"})
        );
        assert!(home.path().join(".codex-global-state.json.bak").exists());
    }

    #[test]
    fn sync_backup_contains_database_and_sidecars() {
        let home = TestHome::new();
        fs::write(
            home.path().join("config.toml"),
            "model_provider = \"custom\"\n",
        )
        .unwrap();
        write_test_rollout(home.path(), "openai", "thread-a", "C:/workspace");
        let db_path = home.path().join("state_5.sqlite");
        create_test_db(&db_path, "openai", "C:/old");
        fs::write(format!("{}-wal", db_path.display()), b"").unwrap();
        fs::write(format!("{}-shm", db_path.display()), b"").unwrap();

        let result = run_provider_sync_with_target(Some(home.path()), Some("custom"));
        let backup = result.backup_dir.expect("sync backup");

        assert!(backup.join("db/state_5.sqlite").exists());
        assert!(backup.join("db/state_5.sqlite-wal").exists());
        assert!(backup.join("db/state_5.sqlite-shm").exists());
        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(backup.join("session-meta-backup.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            manifest[0]["originalSessionMetaLines"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn encrypted_history_reports_cross_provider_continuation_risk() {
        let home = TestHome::new();
        fs::write(
            home.path().join("config.toml"),
            "model_provider = \"custom\"\n",
        )
        .unwrap();
        let rollout = write_test_rollout(home.path(), "openai", "thread-a", "C:/workspace");
        fs::OpenOptions::new()
            .append(true)
            .open(rollout)
            .unwrap()
            .write_all(b"{\"type\":\"response_item\",\"payload\":{\"encrypted_content\":\"ciphertext\"}}\n")
            .unwrap();
        create_test_db(&home.path().join("state_5.sqlite"), "openai", "C:/old");

        let result = run_provider_sync_with_target(Some(home.path()), Some("custom"));

        let warning = result.encrypted_content_warning.expect("risk warning");
        assert!(warning.contains("openai"));
        assert!(warning.contains("invalid_encrypted_content"));
    }

    #[test]
    fn lock_and_invalid_explicit_target_skip_without_writing() {
        let home = TestHome::new();
        fs::write(
            home.path().join("config.toml"),
            "model_provider = \"openai\"\n",
        )
        .unwrap();
        let rollout = write_test_rollout(home.path(), "openai", "thread-a", "C:/workspace");
        let original = fs::read_to_string(&rollout).unwrap();

        let invalid = run_provider_sync_with_target(Some(home.path()), Some("bad/provider"));
        assert_eq!(invalid.status, ProviderSyncStatus::Skipped);
        assert_eq!(fs::read_to_string(&rollout).unwrap(), original);

        fs::create_dir_all(home.path().join("tmp/provider-sync.lock")).unwrap();
        let locked = run_provider_sync_with_target(Some(home.path()), Some("custom"));
        assert_eq!(locked.status, ProviderSyncStatus::Skipped);
        assert!(locked.message.to_ascii_lowercase().contains("lock"));
        assert_eq!(fs::read_to_string(&rollout).unwrap(), original);
    }

    #[test]
    fn provider_sync_preserves_rollout_modified_time() {
        let home = TestHome::new();
        fs::write(
            home.path().join("config.toml"),
            "model_provider = \"custom\"\n",
        )
        .unwrap();
        let rollout = write_test_rollout(home.path(), "openai", "thread-a", "C:/workspace");
        let past = SystemTime::now() - Duration::from_secs(86_400);
        fs::File::options()
            .write(true)
            .open(&rollout)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(past))
            .unwrap();
        let before = fs::metadata(&rollout).unwrap().modified().unwrap();

        let result = run_provider_sync_with_target(Some(home.path()), Some("custom"));

        assert_eq!(result.status, ProviderSyncStatus::Synced);
        let after = fs::metadata(&rollout).unwrap().modified().unwrap();
        let drift = after
            .duration_since(before)
            .unwrap_or_else(|error| error.duration());
        assert!(drift < Duration::from_secs(2), "mtime drifted by {drift:?}");
    }
}
