//! Dock orientation + position math for Desktop mode.
//!
//! macOS exposes Dock orientation via `defaults read com.apple.dock orientation`
//! (values: `bottom` | `left` | `right`). We parse that string and use it with
//! an `NSScreen.visibleFrame` rect to decide where Pane should stand and what
//! range he can walk within.
//!
//! The subprocess-reading and NSScreen calls live in lib.rs — this module is
//! just the decision math so tests don't need a live runtime.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockOrientation { Bottom, Left, Right }

impl DockOrientation {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "left" => DockOrientation::Left,
            "right" => DockOrientation::Right,
            _ => DockOrientation::Bottom, // default + unknown
        }
    }
}

/// A rect in screen points (top-left origin), matching what `NSScreen.frame` /
/// `visibleFrame` give us after the bottom-origin flip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaneSize { pub w: f64, pub h: f64 }

/// Where Pane should sit on first placement for Desktop mode. He walks
/// horizontally along the bottom Dock, vertically along a left/right Dock.
/// Returned `(x, y)` is the window's top-left in screen points.
pub fn initial_desktop_position(
    visible_frame: ScreenRect,
    orientation: DockOrientation,
    pane: PaneSize,
) -> (f64, f64) {
    match orientation {
        DockOrientation::Bottom => {
            // Center horizontally, sit at the bottom edge of the visible
            // frame (which accounts for the Dock when it's shown).
            let x = visible_frame.x + (visible_frame.w - pane.w) / 2.0;
            let y = visible_frame.y + visible_frame.h - pane.h;
            (x.round(), y.round())
        }
        DockOrientation::Left => {
            // Sit at the left edge, centered vertically.
            let x = visible_frame.x;
            let y = visible_frame.y + (visible_frame.h - pane.h) / 2.0;
            (x.round(), y.round())
        }
        DockOrientation::Right => {
            let x = visible_frame.x + visible_frame.w - pane.w;
            let y = visible_frame.y + (visible_frame.h - pane.h) / 2.0;
            (x.round(), y.round())
        }
    }
}

/// The range Pane can walk within given the Dock orientation. For a bottom
/// Dock this is `(min_x, max_x)` on a fixed Y; for a side Dock it's
/// `(min_y, max_y)` on a fixed X. Returned as `(axis, min, max, fixed)`
/// where axis is 'h' (horizontal) or 'v' (vertical).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WalkRange {
    pub horizontal: bool,
    pub min: f64,
    pub max: f64,
    pub fixed: f64,
}

pub fn desktop_walk_range(
    visible_frame: ScreenRect,
    orientation: DockOrientation,
    pane: PaneSize,
) -> WalkRange {
    match orientation {
        DockOrientation::Bottom => WalkRange {
            horizontal: true,
            min: visible_frame.x,
            max: visible_frame.x + visible_frame.w - pane.w,
            fixed: visible_frame.y + visible_frame.h - pane.h,
        },
        DockOrientation::Left => WalkRange {
            horizontal: false,
            min: visible_frame.y,
            max: visible_frame.y + visible_frame.h - pane.h,
            fixed: visible_frame.x,
        },
        DockOrientation::Right => WalkRange {
            horizontal: false,
            min: visible_frame.y,
            max: visible_frame.y + visible_frame.h - pane.h,
            fixed: visible_frame.x + visible_frame.w - pane.w,
        },
    }
}

/// When the user drops Pane away from the Dock edge, compute where he should
/// walk back to — the nearest point on the Dock-edge range. `None` means he's
/// already in range, no walk needed.
pub fn walk_back_target(
    current_x: f64,
    current_y: f64,
    range: &WalkRange,
) -> Option<(f64, f64)> {
    if range.horizontal {
        let in_range = current_x >= range.min && current_x <= range.max;
        let on_line = (current_y - range.fixed).abs() < 1.0;
        if in_range && on_line { return None; }
        let tx = current_x.clamp(range.min, range.max);
        Some((tx, range.fixed))
    } else {
        let in_range = current_y >= range.min && current_y <= range.max;
        let on_line = (current_x - range.fixed).abs() < 1.0;
        if in_range && on_line { return None; }
        let ty = current_y.clamp(range.min, range.max);
        Some((range.fixed, ty))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vf() -> ScreenRect { ScreenRect { x: 0.0, y: 25.0, w: 1440.0, h: 780.0 } }
    fn pane() -> PaneSize { PaneSize { w: 120.0, h: 160.0 } }

    #[test]
    fn parse_bottom_left_right_unknown_default_to_bottom() {
        assert_eq!(DockOrientation::parse("bottom"), DockOrientation::Bottom);
        assert_eq!(DockOrientation::parse("BOTTOM"), DockOrientation::Bottom);
        assert_eq!(DockOrientation::parse("left"), DockOrientation::Left);
        assert_eq!(DockOrientation::parse("right"), DockOrientation::Right);
        assert_eq!(DockOrientation::parse(""), DockOrientation::Bottom);
        assert_eq!(DockOrientation::parse("diagonal"), DockOrientation::Bottom);
        // With trailing newline, which `defaults read` emits.
        assert_eq!(DockOrientation::parse("right\n"), DockOrientation::Right);
    }

    #[test]
    fn initial_position_bottom_dock_sits_at_bottom_center() {
        let (x, y) = initial_desktop_position(vf(), DockOrientation::Bottom, pane());
        // Centered: (1440 - 120) / 2 = 660
        assert_eq!(x, 660.0);
        // Bottom: 25 + 780 - 160 = 645
        assert_eq!(y, 645.0);
    }

    #[test]
    fn initial_position_left_dock_sits_at_left_middle() {
        let (x, y) = initial_desktop_position(vf(), DockOrientation::Left, pane());
        assert_eq!(x, 0.0);
        // Middle: 25 + (780 - 160) / 2 = 25 + 310 = 335
        assert_eq!(y, 335.0);
    }

    #[test]
    fn initial_position_right_dock_sits_at_right_middle() {
        let (x, y) = initial_desktop_position(vf(), DockOrientation::Right, pane());
        // Right edge: 0 + 1440 - 120 = 1320
        assert_eq!(x, 1320.0);
        assert_eq!(y, 335.0);
    }

    #[test]
    fn walk_range_bottom_dock_is_horizontal() {
        let r = desktop_walk_range(vf(), DockOrientation::Bottom, pane());
        assert!(r.horizontal);
        assert_eq!(r.min, 0.0);
        assert_eq!(r.max, 1320.0);
        assert_eq!(r.fixed, 645.0);
    }

    #[test]
    fn walk_range_side_dock_is_vertical() {
        let r = desktop_walk_range(vf(), DockOrientation::Left, pane());
        assert!(!r.horizontal);
        assert_eq!(r.min, 25.0);
        assert_eq!(r.max, 645.0);
        assert_eq!(r.fixed, 0.0);
    }

    #[test]
    fn walk_back_none_when_already_on_bottom_range() {
        let r = desktop_walk_range(vf(), DockOrientation::Bottom, pane());
        assert_eq!(walk_back_target(500.0, 645.0, &r), None);
    }

    #[test]
    fn walk_back_clamps_to_range_bottom() {
        let r = desktop_walk_range(vf(), DockOrientation::Bottom, pane());
        // User dropped Pane above the walk line and to the right of the range.
        let t = walk_back_target(2000.0, 200.0, &r).expect("walk back needed");
        assert_eq!(t.0, 1320.0); // clamped X
        assert_eq!(t.1, 645.0);  // ground Y
    }

    #[test]
    fn walk_back_clamps_to_range_left_dock() {
        let r = desktop_walk_range(vf(), DockOrientation::Left, pane());
        // Dropped in the middle of the screen — needs to walk back to x=0,
        // y clamped to the vertical range.
        let t = walk_back_target(600.0, 400.0, &r).expect("walk back needed");
        assert_eq!(t.0, 0.0);
        assert_eq!(t.1, 400.0);
    }
}
