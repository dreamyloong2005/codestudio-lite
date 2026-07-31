use super::{
    codex_enhancement_settings_from, codex_patch_launch_args, select_debug_port,
    spawn_codex_enhancement_injection, ChatGptDesktopSettings, CodexEnhancementInjectionSettings,
};

pub(super) fn launch<F>(settings: &ChatGptDesktopSettings, launcher: F) -> Result<(), String>
where
    F: FnOnce(&[String]) -> Result<(), String>,
{
    EnhancementController::prepare(settings)?.launch_with(launcher, |controller| {
        spawn_codex_enhancement_injection(controller.debug_port, controller.settings);
    })
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
