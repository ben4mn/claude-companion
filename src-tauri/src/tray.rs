//! Tray icon management — pure decision helpers.
//!
//! The actual `TrayIconBuilder` call and menu-event wiring live in lib.rs
//! (they need a live `AppHandle`). This module just answers two questions
//! the rest of the app keeps asking:
//!
//!   1. Has the user toggled tray visibility? → emit a Show/Hide action.
//!   2. Is this the user's first time turning the tray off? → show the
//!      first-disable warning so they know their hotkey escape hatch.
//!
//! Keeping these as pure functions means we can unit-test the transition
//! logic without spinning up a Tauri runtime.

use crate::settings::TraySettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    /// No-op: old and new visibility agree.
    None,
    /// The tray icon should appear in the menu bar.
    Show,
    /// The tray icon should be hidden from the menu bar.
    Hide,
}

pub fn compute_tray_action(old_visible: bool, new_visible: bool) -> TrayAction {
    match (old_visible, new_visible) {
        (false, true) => TrayAction::Show,
        (true, false) => TrayAction::Hide,
        _ => TrayAction::None,
    }
}

/// The warning is shown exactly once in a user's lifetime: the first time they
/// flip the tray icon from visible to hidden, before we've acknowledged that
/// they've seen the dialog. Subsequent toggles are silent.
pub fn should_show_first_disable_warning(old: &TraySettings, new: &TraySettings) -> bool {
    old.visible && !new.visible && !new.first_disable_warning_shown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_action_when_visibility_unchanged() {
        assert_eq!(compute_tray_action(true, true), TrayAction::None);
        assert_eq!(compute_tray_action(false, false), TrayAction::None);
    }

    #[test]
    fn hides_when_visibility_flips_to_false() {
        assert_eq!(compute_tray_action(true, false), TrayAction::Hide);
    }

    #[test]
    fn shows_when_visibility_flips_to_true() {
        assert_eq!(compute_tray_action(false, true), TrayAction::Show);
    }

    #[test]
    fn warning_fires_on_first_disable() {
        let old = TraySettings { visible: true, first_disable_warning_shown: false };
        let new = TraySettings { visible: false, first_disable_warning_shown: false };
        assert!(should_show_first_disable_warning(&old, &new));
    }

    #[test]
    fn warning_suppressed_after_acknowledgement() {
        // Once the UI has set first_disable_warning_shown=true on behalf of
        // the user, we never surface the dialog again.
        let old = TraySettings { visible: true, first_disable_warning_shown: true };
        let new = TraySettings { visible: false, first_disable_warning_shown: true };
        assert!(!should_show_first_disable_warning(&old, &new));
    }

    #[test]
    fn warning_does_not_fire_when_enabling_tray() {
        let old = TraySettings { visible: false, first_disable_warning_shown: false };
        let new = TraySettings { visible: true, first_disable_warning_shown: false };
        assert!(!should_show_first_disable_warning(&old, &new));
    }

    #[test]
    fn warning_does_not_fire_on_unrelated_setting_change() {
        let old = TraySettings { visible: true, first_disable_warning_shown: false };
        let new = TraySettings { visible: true, first_disable_warning_shown: false };
        assert!(!should_show_first_disable_warning(&old, &new));
    }
}
