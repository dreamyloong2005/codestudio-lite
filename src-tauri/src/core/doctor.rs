use crate::core::app_paths::{app_paths, display_path};
use crate::core::detector;
use crate::core::types::{DoctorCheck, DoctorReport, Severity};
use chrono::Utc;

pub fn run_doctor() -> Result<DoctorReport, String> {
    let snapshot = detector::detect_environment()?;
    let paths = app_paths().map_err(|err| err.to_string())?;
    let mut checks = Vec::new();

    checks.push(DoctorCheck {
        id: "config-dir".to_string(),
        group: "Config Files".to_string(),
        label: "CodeStudio Lite directory".to_string(),
        status: path_status(paths.config_dir.exists()),
        detail: display_path(&paths.config_dir),
    });
    checks.push(DoctorCheck {
        id: "state-database".to_string(),
        group: "Config Files".to_string(),
        label: "State database".to_string(),
        status: path_status(paths.database_file.exists()),
        detail: display_path(&paths.database_file),
    });
    checks.push(DoctorCheck {
        id: "downloads-dir".to_string(),
        group: "Config Files".to_string(),
        label: "Downloads directory".to_string(),
        status: path_status(paths.downloads_dir.exists()),
        detail: display_path(&paths.downloads_dir),
    });
    checks.push(DoctorCheck {
        id: "secret-store".to_string(),
        group: "Security".to_string(),
        label: "System keychain".to_string(),
        status: Severity::Info,
        detail: "Keychain wiring is planned after profile creation flow.".to_string(),
    });

    for tool in snapshot.system.iter() {
        checks.push(DoctorCheck {
            id: format!("system-{}", tool.id),
            group: "System".to_string(),
            label: tool.name.clone(),
            status: if tool.version.is_some() {
                Severity::Ok
            } else {
                Severity::Warning
            },
            detail: tool
                .version
                .clone()
                .or_else(|| tool.details.clone())
                .unwrap_or_else(|| "Missing".to_string()),
        });
    }

    for tool in snapshot.tools.iter() {
        checks.push(DoctorCheck {
            id: format!("tool-{}", tool.id),
            group: "AI Coding Tools".to_string(),
            label: tool.name.clone(),
            status: if tool.version.is_some() {
                Severity::Ok
            } else {
                Severity::Warning
            },
            detail: tool
                .version
                .clone()
                .or_else(|| tool.details.clone())
                .unwrap_or_else(|| "Missing".to_string()),
        });
    }

    Ok(DoctorReport {
        generated_at: Utc::now().to_rfc3339(),
        checks,
        problems: snapshot.problems,
    })
}

fn path_status(exists: bool) -> Severity {
    if exists {
        Severity::Ok
    } else {
        Severity::Error
    }
}
