//! Pure occlusion detection for Claude-only mode.
//!
//! When Pane lives inside Claude's window, any other app's window that's
//! z-above Claude and intersects Pane's bbox should hide him — otherwise he
//! appears to float on top of whatever the user Cmd-Tabbed into, which is
//! exactly the thing the "Claude-only" mode promises not to do.
//!
//! The live code (lib.rs) reads the CGWindow list in z-order and feeds us
//! each non-Claude, non-Pane window rect above Claude. This module decides
//! *what to do* with that list — pure intersection math, trivially
//! unit-testable.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    /// Do two axis-aligned rects overlap? Touching edges (right == other.x)
    /// do NOT count as overlap; a window exactly butting up against Pane
    /// is not occluding him.
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }
}

/// True when *any* rect in `occluders_above` overlaps `pane` — that's the
/// signal to hide Pane until the occluder moves. Callers pass only rects
/// that are (a) z-above Claude in the window list and (b) not Pane itself.
pub fn is_occluded(pane: Rect, occluders_above: &[Rect]) -> bool {
    occluders_above.iter().any(|r| pane.intersects(r))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_intersect_when_overlapping() {
        let a = Rect::new(0.0, 0.0, 100.0, 100.0);
        let b = Rect::new(50.0, 50.0, 100.0, 100.0);
        assert!(a.intersects(&b));
        assert!(b.intersects(&a));
    }

    #[test]
    fn rect_no_intersect_when_disjoint() {
        let a = Rect::new(0.0, 0.0, 100.0, 100.0);
        let b = Rect::new(200.0, 200.0, 100.0, 100.0);
        assert!(!a.intersects(&b));
    }

    #[test]
    fn rect_edge_touching_is_not_intersection() {
        // Windows that share an edge but don't overlap interior pixels
        // shouldn't hide Pane — that's a tiled, non-overlapping layout.
        let a = Rect::new(0.0, 0.0, 100.0, 100.0);
        let b = Rect::new(100.0, 0.0, 100.0, 100.0);
        assert!(!a.intersects(&b));
    }

    #[test]
    fn is_occluded_false_with_no_windows_above() {
        let pane = Rect::new(500.0, 400.0, 68.0, 72.0);
        assert!(!is_occluded(pane, &[]));
    }

    #[test]
    fn is_occluded_false_when_windows_above_dont_overlap() {
        let pane = Rect::new(500.0, 400.0, 68.0, 72.0);
        let other = Rect::new(0.0, 0.0, 200.0, 200.0);
        assert!(!is_occluded(pane, &[other]));
    }

    #[test]
    fn is_occluded_true_when_any_window_covers_pane() {
        let pane = Rect::new(500.0, 400.0, 68.0, 72.0);
        // First rect is disjoint, second covers pane partially.
        let rects = [
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Rect::new(480.0, 380.0, 200.0, 200.0),
        ];
        assert!(is_occluded(pane, &rects));
    }

    #[test]
    fn is_occluded_true_when_window_fully_contains_pane() {
        let pane = Rect::new(500.0, 400.0, 68.0, 72.0);
        let covering = Rect::new(0.0, 0.0, 2000.0, 1200.0);
        assert!(is_occluded(pane, &[covering]));
    }

    #[test]
    fn is_occluded_pane_partially_covered_is_occluded() {
        // Even partial cover = hide. Anything less is flicker-prone.
        let pane = Rect::new(500.0, 400.0, 68.0, 72.0);
        let partial = Rect::new(520.0, 420.0, 100.0, 100.0);
        assert!(is_occluded(pane, &[partial]));
    }
}
