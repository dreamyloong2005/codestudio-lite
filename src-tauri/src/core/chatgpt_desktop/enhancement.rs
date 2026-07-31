use super::{
    codex_enhancement_settings_from, codex_patch_launch_args, select_debug_port,
    spawn_codex_enhancement_injection, ChatGptDesktopSettings, CodexEnhancementInjectionSettings,
};

const SCRIPT_TEMPLATE: &str = include_str!("codex_enhancements.js");
const SETTINGS_PLACEHOLDER: &str = "__CODESTUDIO_LITE_SETTINGS__";
const MARKETPLACES_PLACEHOLDER: &str = "__CODESTUDIO_LITE_PLUGIN_MARKETPLACES__";

pub(super) fn launch<F>(settings: &ChatGptDesktopSettings, launcher: F) -> Result<(), String>
where
    F: FnOnce(&[String]) -> Result<(), String>,
{
    EnhancementController::prepare(settings)?.launch_with(launcher, |controller| {
        spawn_codex_enhancement_injection(controller.debug_port, controller.settings);
    })
}

pub(super) fn render_script(
    settings_json: &str,
    marketplaces_json: &str,
) -> Result<String, String> {
    for placeholder in [SETTINGS_PLACEHOLDER, MARKETPLACES_PLACEHOLDER] {
        if SCRIPT_TEMPLATE.matches(placeholder).count() != 1 {
            return Err(format!(
                "Codex enhancement script must contain exactly one {placeholder} placeholder."
            ));
        }
    }
    let rendered = SCRIPT_TEMPLATE
        .replace(SETTINGS_PLACEHOLDER, settings_json)
        .replace(MARKETPLACES_PLACEHOLDER, marketplaces_json);
    if rendered.contains(SETTINGS_PLACEHOLDER) || rendered.contains(MARKETPLACES_PLACEHOLDER) {
        return Err("Codex enhancement script contains an unresolved placeholder.".to_string());
    }
    Ok(rendered)
}

struct EnhancementController {
    debug_port: u16,
    settings: CodexEnhancementInjectionSettings,
}

impl EnhancementController {
    fn prepare(settings: &ChatGptDesktopSettings) -> Result<Self, String> {
        Ok(Self {
            debug_port: select_debug_port()?,
            settings: codex_enhancement_settings_from(settings),
        })
    }

    fn launch_with<F, S>(self, launcher: F, start: S) -> Result<(), String>
    where
        F: FnOnce(&[String]) -> Result<(), String>,
        S: FnOnce(Self),
    {
        launcher(&codex_patch_launch_args(self.debug_port))?;
        if self.settings.enabled() {
            start(self);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_replaces_every_required_placeholder() {
        let script = render_script(r#"{"enabled":true}"#, "[]").unwrap();
        assert!(script.contains(r#"{"enabled":true}"#));
        assert!(!script.contains(SETTINGS_PLACEHOLDER));
        assert!(!script.contains(MARKETPLACES_PLACEHOLDER));
    }
}
