use chrono::Utc;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
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
}

#[derive(Debug, Clone)]
struct PendingRewrite {
    path: PathBuf,
    original_text: String,
    next_text: String,
    original_mtime: Option<SystemTime>,
}

#[derive(Debug, Default)]
pub struct ProviderSyncReport {
    pub target_provider: String,
    pub changed_session_files: usize,
    pub sqlite_rows_updated: usize,
}

pub fn run_default_provider_sync() -> Result<ProviderSyncReport, String> {
    let home = codex_home_dir()?;
    if !home.exists() {
        return Ok(ProviderSyncReport {
            target_provider: DEFAULT_PROVIDER.to_string(),
            ..ProviderSyncReport::default()
        });
    }

    let target_provider = read_current_provider(&home.join("config.toml"));
    let lock_dir = home.join("tmp").join("provider-sync.lock");
    acquire_lock(&lock_dir)?;
    let result = run_provider_sync_locked(&home, &target_provider);
    let _ = release_lock(&lock_dir);
    result
}

fn run_provider_sync_locked(
    home: &Path,
    target_provider: &str,
) -> Result<ProviderSyncReport, String> {
    let mut rewrites = Vec::new();
    let mut user_event_thread_ids = HashSet::new();
    let mut cwd_by_thread_id = HashMap::new();

    for path in rollout_files(home)? {
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if is_locked_io_error(&error) => continue,
            Err(error) => {
                return Err(format!(
                    "Failed to read Codex session file {}: {error}",
                    path.display()
                ))
            }
        };
        let rewrite = rewrite_rollout_session_meta(&text, target_provider)?;
        if let Some(thread_id) = rewrite.thread_id.as_ref() {
            if rewrite.has_user_event {
                user_event_thread_ids.insert(thread_id.clone());
            }
            if let Some(cwd) = rewrite.cwd.as_ref() {
                cwd_by_thread_id.insert(thread_id.clone(), cwd.clone());
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
    if rewrites.is_empty() && sqlite_rows == 0 {
        return Ok(ProviderSyncReport {
            target_provider: target_provider.to_string(),
            changed_session_files: 0,
            sqlite_rows_updated: 0,
        });
    }

    create_backup(home, target_provider, &rewrites)?;
    let mut applied = Vec::new();
    for rewrite in &rewrites {
        match fs::write(&rewrite.path, &rewrite.next_text) {
            Ok(()) => {
                restore_file_mtime(&rewrite.path, rewrite.original_mtime);
                applied.push(rewrite.clone());
            }
            Err(error) if is_locked_io_error(&error) => {}
            Err(error) => {
                restore_rewrites(&applied);
                return Err(format!(
                    "Failed to update Codex session file {}: {error}",
                    rewrite.path.display()
                ));
            }
        }
    }

    let sqlite_rows_updated = match apply_sqlite_updates(
        home,
        target_provider,
        &user_event_thread_ids,
        &cwd_by_thread_id,
    ) {
        Ok(rows) => rows,
        Err(error) => {
            restore_rewrites(&applied);
            return Err(error);
        }
    };

    Ok(ProviderSyncReport {
        target_provider: target_provider.to_string(),
        changed_session_files: applied.len(),
        sqlite_rows_updated,
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

fn root_toml_string_value(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let stripped = line.trim();
        if stripped.starts_with('[') {
            break;
        }
        let raw = stripped
            .strip_prefix(key)?
            .trim_start()
            .strip_prefix('=')?
            .trim_start();
        return parse_toml_string(raw);
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
    if stripped.starts_with(r"\\?\") {
        return Some(stripped[4..].replace('\\', "/"));
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
) -> Result<(), String> {
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
            let _ = fs::copy(&source, dir.join(name));
        }
    }
    let manifest = rewrites
        .iter()
        .map(|rewrite| json!({"path": rewrite.path.to_string_lossy()}))
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
            "changedSessionFiles": rewrites.len(),
            "managedBy": "CodeStudio Lite provider sync"
        }))
        .map_err(|err| err.to_string())?,
    )
    .map_err(|err| format!("Failed to write provider sync backup metadata: {err}"))?;
    prune_backups(&root);
    Ok(())
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
        .filter(|path| has_session_table(path))
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

fn has_session_table(path: &Path) -> bool {
    let Ok(db) = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return false;
    };
    db.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'threads' LIMIT 1",
        [],
        |_| Ok(()),
    )
    .is_ok()
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
) -> Result<usize, String> {
    let mut total = 0;
    for path in codex_session_db_paths(home) {
        total += count_sqlite_updates_for_path(
            &path,
            target_provider,
            user_event_thread_ids,
            cwd_by_thread_id,
        )?;
    }
    Ok(total)
}

fn count_sqlite_updates_for_path(
    path: &Path,
    target_provider: &str,
    user_event_thread_ids: &HashSet<String>,
    cwd_by_thread_id: &HashMap<String, String>,
) -> Result<usize, String> {
    if !path.exists() {
        return Ok(0);
    }
    let db = Connection::open(path).map_err(|err| {
        format!(
            "Failed to open Codex SQLite database {}: {err}",
            path.display()
        )
    })?;
    let columns = table_columns(&db, "threads")?;
    if !columns.contains("model_provider") {
        return Ok(0);
    }
    let mut total =
        db.query_row(
            "SELECT COUNT(*) FROM threads WHERE COALESCE(model_provider, '') <> ?1",
            [target_provider],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|err| format!("Failed to count Codex SQLite updates: {err}"))? as usize;
    if columns.contains("has_user_event") {
        for thread_id in user_event_thread_ids {
            total += db
                .query_row(
                    "SELECT COUNT(*) FROM threads WHERE id = ?1 AND COALESCE(has_user_event, 0) <> 1",
                    [thread_id],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|err| format!("Failed to count Codex SQLite user event updates: {err}"))?
                as usize;
        }
    }
    if columns.contains("cwd") {
        for (thread_id, cwd) in cwd_by_thread_id {
            total += db
                .query_row(
                    "SELECT COUNT(*) FROM threads WHERE id = ?1 AND COALESCE(cwd, '') <> ?2",
                    (thread_id, cwd),
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|err| format!("Failed to count Codex SQLite cwd updates: {err}"))?
                as usize;
        }
    }
    Ok(total)
}

fn apply_sqlite_updates(
    home: &Path,
    target_provider: &str,
    user_event_thread_ids: &HashSet<String>,
    cwd_by_thread_id: &HashMap<String, String>,
) -> Result<usize, String> {
    let mut total = 0;
    for path in codex_session_db_paths(home) {
        total += apply_sqlite_updates_for_path(
            &path,
            target_provider,
            user_event_thread_ids,
            cwd_by_thread_id,
        )?;
    }
    Ok(total)
}

fn apply_sqlite_updates_for_path(
    path: &Path,
    target_provider: &str,
    user_event_thread_ids: &HashSet<String>,
    cwd_by_thread_id: &HashMap<String, String>,
) -> Result<usize, String> {
    if !path.exists() {
        return Ok(0);
    }
    let mut db = Connection::open(path).map_err(|err| {
        format!(
            "Failed to open Codex SQLite database {}: {err}",
            path.display()
        )
    })?;
    let columns = table_columns(&db, "threads")?;
    if !columns.contains("model_provider") {
        return Ok(0);
    }
    let tx = db
        .transaction()
        .map_err(|err| format!("Failed to start Codex SQLite update: {err}"))?;
    let mut total = tx
        .execute(
            "UPDATE threads SET model_provider = ?1 WHERE COALESCE(model_provider, '') <> ?1",
            [target_provider],
        )
        .map_err(|err| format!("Failed to update Codex SQLite provider rows: {err}"))?;
    if columns.contains("has_user_event") {
        for thread_id in user_event_thread_ids {
            total += tx
                .execute(
                    "UPDATE threads SET has_user_event = 1 WHERE id = ?1 AND COALESCE(has_user_event, 0) <> 1",
                    [thread_id],
                )
                .map_err(|err| format!("Failed to update Codex SQLite user event rows: {err}"))?;
        }
    }
    if columns.contains("cwd") {
        for (thread_id, cwd) in cwd_by_thread_id {
            total += tx
                .execute(
                    "UPDATE threads SET cwd = ?1 WHERE id = ?2 AND COALESCE(cwd, '') <> ?1",
                    (cwd, thread_id),
                )
                .map_err(|err| format!("Failed to update Codex SQLite cwd rows: {err}"))?;
        }
    }
    tx.commit()
        .map_err(|err| format!("Failed to commit Codex SQLite update: {err}"))?;
    Ok(total)
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
                "model_provider = \"custom\"\n[model_providers.custom]\n",
                "model_provider"
            )
            .as_deref(),
            Some("custom")
        );
    }
}
