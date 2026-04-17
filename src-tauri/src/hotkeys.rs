//! Global hotkey management.
//!
//! The runtime binding lives elsewhere (talks to `tauri-plugin-global-shortcut`).
//! This module is the *pure* decision surface: given the current and next
//! `HotkeySettings`, tell us which accelerators need to be unregistered and
//! which need registering. That diff is what the app has to apply whenever the
//! user edits a hotkey, and it's trivial to unit-test without a live plugin.

use crate::settings::HotkeySettings;

/// One of the logical hotkey actions the app exposes. Strings match what the
/// UI stores and what settings.rs persists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    ShowHide,
    OpenSettings,
    Quit,
}

impl HotkeyAction {
    pub fn id(&self) -> &'static str {
        match self {
            HotkeyAction::ShowHide => "show_hide",
            HotkeyAction::OpenSettings => "open_settings",
            HotkeyAction::Quit => "quit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HotkeyDiff {
    /// (action id, accelerator string) pairs that need to be registered fresh.
    pub to_register: Vec<(String, String)>,
    /// Accelerator strings that were in `old` but either changed or vanished
    /// and must be unregistered before a fresh bind.
    pub to_unregister: Vec<String>,
}

/// Given the previous and next hotkey settings, compute the minimal set of
/// plugin operations needed to converge the system's registrations.
///
/// Unchanged bindings are left alone — don't re-register in a loop, the plugin
/// errors on duplicate registrations.
pub fn diff_hotkeys(old: &HotkeySettings, new: &HotkeySettings) -> HotkeyDiff {
    let mut diff = HotkeyDiff::default();

    let pairs = [
        (HotkeyAction::ShowHide, &old.show_hide, &new.show_hide),
        (HotkeyAction::OpenSettings, &old.open_settings, &new.open_settings),
        (HotkeyAction::Quit, &old.quit, &new.quit),
    ];

    for (action, old_acc, new_acc) in pairs {
        if old_acc == new_acc { continue; }
        if !old_acc.is_empty() {
            diff.to_unregister.push(old_acc.clone());
        }
        if !new_acc.is_empty() {
            diff.to_register.push((action.id().into(), new_acc.clone()));
        }
    }

    diff
}

/// Initial registrations on app boot — equivalent to `diff_hotkeys(default, current)`
/// except it never emits unregister entries (there's nothing to unregister yet).
pub fn initial_registrations(current: &HotkeySettings) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let pairs = [
        (HotkeyAction::ShowHide, &current.show_hide),
        (HotkeyAction::OpenSettings, &current.open_settings),
        (HotkeyAction::Quit, &current.quit),
    ];
    for (action, acc) in pairs {
        if !acc.is_empty() {
            out.push((action.id().into(), acc.clone()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(show_hide: &str, open_settings: &str, quit: &str) -> HotkeySettings {
        HotkeySettings {
            show_hide: show_hide.into(),
            open_settings: open_settings.into(),
            quit: quit.into(),
        }
    }

    #[test]
    fn diff_identical_settings_is_empty() {
        let a = HotkeySettings::default();
        let b = HotkeySettings::default();
        let d = diff_hotkeys(&a, &b);
        assert!(d.to_register.is_empty());
        assert!(d.to_unregister.is_empty());
    }

    #[test]
    fn diff_detects_single_rebind() {
        let a = HotkeySettings::default();
        let mut b = a.clone();
        b.show_hide = "Cmd+Shift+C".into();
        let d = diff_hotkeys(&a, &b);
        assert_eq!(d.to_unregister, vec!["Cmd+Shift+P"]);
        assert_eq!(d.to_register, vec![("show_hide".to_string(), "Cmd+Shift+C".to_string())]);
    }

    #[test]
    fn diff_rebinds_all_three() {
        let a = settings("A", "B", "C");
        let b = settings("X", "Y", "Z");
        let d = diff_hotkeys(&a, &b);
        assert_eq!(d.to_unregister.len(), 3);
        assert_eq!(d.to_register.len(), 3);
    }

    #[test]
    fn diff_handles_unset_bindings_gracefully() {
        // An empty accelerator means the user cleared the binding. Unregister
        // the old one but don't try to register empty as a new binding.
        let a = settings("Cmd+Shift+P", "Cmd+Shift+,", "Cmd+Shift+Q");
        let b = settings("", "Cmd+Shift+,", "Cmd+Shift+Q");
        let d = diff_hotkeys(&a, &b);
        assert_eq!(d.to_unregister, vec!["Cmd+Shift+P"]);
        assert_eq!(d.to_register, Vec::<(String, String)>::new());
    }

    #[test]
    fn initial_registrations_includes_every_nonempty_binding() {
        let s = HotkeySettings::default();
        let regs = initial_registrations(&s);
        assert_eq!(regs.len(), 3);
        let actions: Vec<&str> = regs.iter().map(|(a, _)| a.as_str()).collect();
        assert!(actions.contains(&"show_hide"));
        assert!(actions.contains(&"open_settings"));
        assert!(actions.contains(&"quit"));
    }

    #[test]
    fn initial_registrations_skips_empty_bindings() {
        let s = settings("Cmd+Shift+P", "", "");
        let regs = initial_registrations(&s);
        assert_eq!(regs.len(), 1);
        assert_eq!(regs[0].0, "show_hide");
    }

    #[test]
    fn action_id_matches_settings_field_names() {
        // The UI persists these exact strings. Changing them is a migration.
        assert_eq!(HotkeyAction::ShowHide.id(), "show_hide");
        assert_eq!(HotkeyAction::OpenSettings.id(), "open_settings");
        assert_eq!(HotkeyAction::Quit.id(), "quit");
    }
}
