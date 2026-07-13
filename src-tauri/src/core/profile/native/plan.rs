use crate::core::app_paths::display_path;
use crate::core::types::{ProfileDraft, ProviderApplyMode};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(in crate::core::profile) struct NativeConfigWritePlan {
    pub(in crate::core::profile) path: PathBuf,
    pub(in crate::core::profile) content: String,
    pub(in crate::core::profile) kind: NativeConfigWriteKind,
    pub(in crate::core::profile) delete: bool,
}

impl NativeConfigWritePlan {
    pub(in crate::core::profile) fn write(
        path: PathBuf,
        content: String,
        kind: NativeConfigWriteKind,
    ) -> Self {
        Self {
            path,
            content,
            kind,
            delete: false,
        }
    }

    pub(in crate::core::profile) fn delete(path: PathBuf, kind: NativeConfigWriteKind) -> Self {
        Self {
            path,
            content: String::new(),
            kind,
            delete: true,
        }
    }
}

#[derive(Debug, Clone)]
pub(in crate::core::profile) struct NativeConfigLifecyclePlan {
    pub(in crate::core::profile) profile: ProfileDraft,
    pub(in crate::core::profile) mode: ProviderApplyMode,
    pub(in crate::core::profile) plan: NativeConfigWritePlan,
    pub(in crate::core::profile) verify_after_write: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::core::profile) enum NativeConfigWriteKind {
    ProfileConfig,
    CodexAuthJson,
    ClaudeVsCodePluginConfig,
    GeminiCodeAssistSettings,
    ClaudeDesktopDeploymentConfig,
    ClaudeDesktopProfileConfig,
    ClaudeDesktopMetaConfig,
    ClaudeDesktopDeveloperSettings,
}

pub(in crate::core::profile) fn apply_native_config_write_plan(
    plan: &NativeConfigWritePlan,
) -> Result<(), String> {
    if plan.delete {
        return match fs::remove_file(&plan.path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(format!(
                "Could not delete native config at {}: {err}",
                display_path(&plan.path)
            )),
        };
    }

    write_native_config(&plan.path, &plan.content)
}

pub(in crate::core::profile) fn filter_native_write_plans(
    plans: Vec<NativeConfigWritePlan>,
) -> Result<Vec<NativeConfigWritePlan>, String> {
    plans
        .into_iter()
        .filter_map(|plan| match native_write_plan_changes_file(&plan) {
            Ok(true) => Some(Ok(plan)),
            Ok(false) => None,
            Err(err) => Some(Err(err)),
        })
        .collect()
}

fn native_write_plan_changes_file(plan: &NativeConfigWritePlan) -> Result<bool, String> {
    if plan.delete {
        return Ok(plan.path.exists());
    }
    if !plan.path.exists() {
        return Ok(true);
    }
    let current = fs::read(&plan.path).map_err(|err| err.to_string())?;
    Ok(current != plan.content.as_bytes())
}

pub(crate) fn write_native_config(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    write_atomic(path, content.as_bytes())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp_path = path.with_extension("tmp");
    {
        let mut file = fs::File::create(&tmp_path).map_err(|err| err.to_string())?;
        file.write_all(bytes).map_err(|err| err.to_string())?;
        file.sync_all().map_err(|err| err.to_string())?;
    }
    fs::rename(&tmp_path, path).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "codestudio-native-plan-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn filters_unchanged_writes_and_missing_deletes() {
        let temp = test_dir("filter");
        let existing = temp.join("existing.json");
        let missing = temp.join("missing.json");
        write_native_config(&existing, "same").unwrap();

        let plans = filter_native_write_plans(vec![
            NativeConfigWritePlan::write(
                existing.clone(),
                "same".to_string(),
                NativeConfigWriteKind::ProfileConfig,
            ),
            NativeConfigWritePlan::delete(missing, NativeConfigWriteKind::ProfileConfig),
            NativeConfigWritePlan::write(
                existing,
                "changed".to_string(),
                NativeConfigWriteKind::ProfileConfig,
            ),
        ])
        .unwrap();

        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].content, "changed");
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn applies_writes_and_idempotent_deletes() {
        let temp = test_dir("apply");
        let path = temp.join("nested").join("config.json");
        let write = NativeConfigWritePlan::write(
            path.clone(),
            "content".to_string(),
            NativeConfigWriteKind::ProfileConfig,
        );
        apply_native_config_write_plan(&write).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "content");

        let delete =
            NativeConfigWritePlan::delete(path.clone(), NativeConfigWriteKind::ProfileConfig);
        apply_native_config_write_plan(&delete).unwrap();
        apply_native_config_write_plan(&delete).unwrap();
        assert!(!path.exists());
        fs::remove_dir_all(temp).unwrap();
    }
}
