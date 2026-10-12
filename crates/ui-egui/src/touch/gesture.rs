//! Touch gesture recognizer: fingers in, mouse-style actions out.
//!
//! Pure `std`, no egui types, so it is unit-tested on every host (`cargo test -p
//! effectcraft-ui-egui touch::gesture`). The egui adapter lives in `touch/mod.rs`.
//!
//! The desktop UI is written for a mouse: right-click menus, Alt/Shift-click, hover, wheel and
//! middle-drag pan. A finger has none of those, so this recognizer maps:
//!
//! | Finger gesture                         | Mouse equivalent                      |
//! |----------------------------------------|---------------------------------------|
//! | tap                                    | left click                            |
//! | press, move past the slop, drag        | left press + drag                     |
//! | press and hold still (long press)      | right click (context menu) + haptic   |
//! | two fingers moving together            | wheel / trackpad scroll (pan)         |
//! | pinch                                  | left to egui's own multi-touch zoom   |
//! | quick two-finger tap                   | undo                                  |
//! | quick three-finger tap                 | redo                                  |
//!
//! The primary press is *deferred* until the finger either lifts (a tap) or moves past the
//! slop (a drag). That is what lets a hold become a right click without a stray left click
//! firing first. Hover is never emitted for fingers.

/// A point in egui points.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

impl Pt {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    fn dist(self, o: Pt) -> f32 {
        ((self.x - o.x).powi(2) + (self.y - o.y).powi(2)).sqrt()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Btn {
    Primary,
    Secondary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Down,
    Move,
    Up,
    Cancel,
}

/// What the host should do. The adapter turns these into egui events, one pointer event per
/// frame so egui sees press → release as separate frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Out {
    Move(Pt),
    Press(Btn, Pt),
    Release(Btn, Pt),
    /// The pointer left (a finger lifted): clears hover state.
    Gone,
    /// Two-finger pan: scroll `delta` points at `at`.
    Scroll {
        at: Pt,
        delta: Pt,
    },
    /// Vibrate briefly (long press fired).
    Haptic,
    Undo,
    Redo,
}

#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Movement (points) before a press becomes a drag. Larger than a mouse: fingers wobble.
    pub slop: f32,
    /// Seconds a still finger must stay down to count as a long press.
    pub long_press: f64,
    /// Longest two/three-finger tap, seconds.
    pub multi_tap: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self { slop: 10.0, long_press: 0.45, multi_tap: 0.30 }
    }
}

#[derive(Clone, Copy, Debug)]
struct Finger {
    id: u64,
    pos: Pt,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Idle,
    /// One finger down, primary press not sent yet.
    Pending {
        origin: Pt,
        t0: f64,
    },
    /// Primary pressed and dragging.
    Dragging,
    /// The long press fired; swallow the rest of this touch.
    Spent,
    /// Two or more fingers.
    Multi {
        t0: f64,
        max: usize,
        travel: f32,
        last: Pt,
    },
}

#[derive(Debug)]
pub struct Recognizer {
    pub cfg: Config,
    fingers: Vec<Finger>,
    mode: Mode,
}

impl Default for Recognizer {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

impl Recognizer {
    pub fn new(cfg: Config) -> Self {
        Self { cfg, fingers: Vec::new(), mode: Mode::Idle }
    }

    /// True while any finger is down: the adapter drops egui-winit's own touch→mouse events.
    pub fn active(&self) -> bool {
        !self.fingers.is_empty()
    }

    /// When `tick` next has something to do (to schedule a repaint), if ever.
    pub fn deadline(&self) -> Option<f64> {
        match self.mode {
            Mode::Pending { t0, .. } => Some(t0 + self.cfg.long_press),
            _ => None,
        }
    }

    fn centroid(&self) -> Pt {
        let n = self.fingers.len().max(1) as f32;
        let (sx, sy) = self.fingers.iter().fold((0.0, 0.0), |a, f| (a.0 + f.pos.x, a.1 + f.pos.y));
        Pt::new(sx / n, sy / n)
    }

    /// Feed one touch event (`t` in seconds).
    pub fn touch(&mut self, id: u64, phase: Phase, pos: Pt, t: f64) -> Vec<Out> {
        let mut out = Vec::new();
        match phase {
            Phase::Down => {
                self.fingers.retain(|f| f.id != id);
                self.fingers.push(Finger { id, pos });
                if self.fingers.len() == 1 {
                    self.mode = Mode::Pending { origin: pos, t0: t };
                    out.push(Out::Move(pos));
                } else {
                    // A second finger ends any one-finger gesture: pinch/pan take over.
                    if self.mode == Mode::Dragging {
                        out.push(Out::Release(Btn::Primary, pos));
                    }
                    let (t0, max, travel) = match self.mode {
                        Mode::Multi { t0, max, travel, .. } => (t0, max, travel),
                        // A first finger that was already held a while is not a quick tap.
                        Mode::Pending { t0, .. } => (t0, 0, 0.0),
                        _ => (t, 0, 0.0),
                    };
                    let max = max.max(self.fingers.len());
                    self.mode = Mode::Multi { t0, max, travel, last: self.centroid() };
                }
            }
            Phase::Move => {
                let Some(f) = self.fingers.iter_mut().find(|f| f.id == id) else { return out };
                f.pos = pos;
                match self.mode {
                    Mode::Pending { origin, .. } if pos.dist(origin) > self.cfg.slop => {
                        out.push(Out::Press(Btn::Primary, origin));
                        out.push(Out::Move(pos));
                        self.mode = Mode::Dragging;
                    }
                    Mode::Dragging => out.push(Out::Move(pos)),
                    Mode::Multi { t0, max, travel, last } => {
                        let c = self.centroid();
                        let d = Pt::new(c.x - last.x, c.y - last.y);
                        let travel = travel + (d.x * d.x + d.y * d.y).sqrt();
                        if self.fingers.len() == 2 {
                            out.push(Out::Scroll { at: c, delta: d });
                        }
                        self.mode = Mode::Multi { t0, max, travel, last: c };
                    }
                    _ => {}
                }
            }
            Phase::Up | Phase::Cancel => {
                let cancel = phase == Phase::Cancel;
                let was = self.mode;
                self.fingers.retain(|f| f.id != id);
                match was {
                    Mode::Pending { origin, .. } => {
                        if !cancel {
                            out.push(Out::Press(Btn::Primary, origin));
                            out.push(Out::Release(Btn::Primary, origin));
                        }
                        out.push(Out::Gone);
                    }
                    Mode::Dragging => {
                        out.push(Out::Release(Btn::Primary, pos));
                        out.push(Out::Gone);
                    }
                    Mode::Spent => out.push(Out::Gone),
                    Mode::Multi { t0, max, travel, .. } => {
                        if self.fingers.is_empty() {
                            let quick = t - t0 <= self.cfg.multi_tap && travel <= self.cfg.slop * 2.0;
                            if !cancel && quick {
                                match max {
                                    2 => out.push(Out::Undo),
                                    3 => out.push(Out::Redo),
                                    _ => {}
                                }
                            }
                            out.push(Out::Gone);
                        }
                    }
                    Mode::Idle => {}
                }
                if self.fingers.is_empty() {
                    self.mode = Mode::Idle;
                } else if let Mode::Multi { t0, max, travel, .. } = was {
                    self.mode = Mode::Multi { t0, max, travel, last: self.centroid() };
                } else {
                    // Stray finger left over from an odd sequence: ignore it until all lift.
                    self.mode = Mode::Spent;
                }
            }
        }
        out
    }

    /// Call every frame (and at `deadline`): fires the long press.
    pub fn tick(&mut self, t: f64) -> Vec<Out> {
        if let Mode::Pending { origin, t0 } = self.mode {
            if t - t0 >= self.cfg.long_press {
                self.mode = Mode::Spent;
                return vec![Out::Move(origin), Out::Haptic, Out::Press(Btn::Secondary, origin), Out::Release(Btn::Secondary, origin)];
            }
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Pt = Pt::new(100.0, 100.0);

    fn has(out: &[Out], o: Out) -> bool {
        out.contains(&o)
    }

    #[test]
    fn tap_is_a_left_click_with_no_hover_left_behind() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        let up = r.touch(1, Phase::Up, A, 0.1);
        assert_eq!(up, vec![Out::Press(Btn::Primary, A), Out::Release(Btn::Primary, A), Out::Gone]);
        assert!(!r.active());
    }

    #[test]
    fn nothing_is_pressed_while_a_finger_rests() {
        let mut r = Recognizer::default();
        let down = r.touch(1, Phase::Down, A, 0.0);
        assert!(!down.iter().any(|o| matches!(o, Out::Press(..))));
        assert!(r.tick(0.2).is_empty());
    }

    #[test]
    fn hold_is_a_right_click_and_never_a_left_click() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        assert_eq!(r.deadline(), Some(0.45));
        assert!(r.tick(0.44).is_empty());
        let fired = r.tick(0.46);
        assert!(has(&fired, Out::Haptic));
        assert!(has(&fired, Out::Press(Btn::Secondary, A)));
        assert!(has(&fired, Out::Release(Btn::Secondary, A)));
        // Lifting afterwards must not click.
        let up = r.touch(1, Phase::Up, A, 0.9);
        assert!(!up.iter().any(|o| matches!(o, Out::Press(..) | Out::Release(..))));
        // And it fires once only.
        assert!(r.tick(1.5).is_empty());
    }

    #[test]
    fn moving_past_the_slop_starts_a_drag_from_the_original_point() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        assert!(r.touch(1, Phase::Move, Pt::new(105.0, 100.0), 0.05).is_empty(), "inside the slop");
        let out = r.touch(1, Phase::Move, Pt::new(130.0, 100.0), 0.1);
        assert_eq!(out[0], Out::Press(Btn::Primary, A), "press lands where the finger went down");
        assert_eq!(out[1], Out::Move(Pt::new(130.0, 100.0)));
        let end = r.touch(1, Phase::Up, Pt::new(150.0, 100.0), 0.3);
        assert_eq!(end[0], Out::Release(Btn::Primary, Pt::new(150.0, 100.0)));
    }

    #[test]
    fn a_drag_never_becomes_a_long_press() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        r.touch(1, Phase::Move, Pt::new(200.0, 100.0), 0.1);
        assert!(r.tick(2.0).is_empty());
        assert_eq!(r.deadline(), None);
    }

    #[test]
    fn two_finger_drag_scrolls_and_sends_no_clicks() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, Pt::new(100.0, 100.0), 0.0);
        r.touch(2, Phase::Down, Pt::new(140.0, 100.0), 0.01);
        let out = r.touch(1, Phase::Move, Pt::new(100.0, 140.0), 0.1);
        let Out::Scroll { delta, .. } = out[0] else { panic!("{out:?}") };
        assert_eq!(delta, Pt::new(0.0, 20.0), "centroid moved half of one finger's travel");
        let end = r.touch(1, Phase::Up, Pt::new(100.0, 140.0), 0.6);
        assert!(!end.iter().any(|o| matches!(o, Out::Press(..) | Out::Undo | Out::Redo)));
    }

    #[test]
    fn second_finger_releases_a_drag_in_progress() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        r.touch(1, Phase::Move, Pt::new(150.0, 100.0), 0.1);
        let out = r.touch(2, Phase::Down, Pt::new(300.0, 300.0), 0.2);
        assert!(matches!(out[0], Out::Release(Btn::Primary, _)));
    }

    #[test]
    fn two_finger_tap_is_undo_three_is_redo() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        r.touch(2, Phase::Down, Pt::new(160.0, 100.0), 0.02);
        r.touch(1, Phase::Up, A, 0.1);
        let out = r.touch(2, Phase::Up, Pt::new(160.0, 100.0), 0.12);
        assert!(has(&out, Out::Undo));

        r.touch(1, Phase::Down, A, 1.0);
        r.touch(2, Phase::Down, Pt::new(160.0, 100.0), 1.01);
        r.touch(3, Phase::Down, Pt::new(220.0, 100.0), 1.02);
        r.touch(1, Phase::Up, A, 1.1);
        r.touch(2, Phase::Up, Pt::new(160.0, 100.0), 1.11);
        let out = r.touch(3, Phase::Up, Pt::new(220.0, 100.0), 1.12);
        assert!(has(&out, Out::Redo));
    }

    #[test]
    fn a_slow_two_finger_rest_is_not_undo() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        r.touch(2, Phase::Down, Pt::new(160.0, 100.0), 0.02);
        r.touch(1, Phase::Up, A, 0.8);
        let out = r.touch(2, Phase::Up, Pt::new(160.0, 100.0), 0.82);
        assert!(!has(&out, Out::Undo));
    }

    #[test]
    fn a_long_hold_then_a_second_finger_does_not_undo_on_release() {
        // First finger rested 0.4 s, second lands and both lift fast: still a quick tap by
        // the *first* finger's start time? No: it started long ago, so it must not undo.
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        r.touch(2, Phase::Down, Pt::new(160.0, 100.0), 0.40);
        r.touch(1, Phase::Up, A, 0.42);
        let out = r.touch(2, Phase::Up, Pt::new(160.0, 100.0), 0.43);
        assert!(!has(&out, Out::Undo));
    }

    #[test]
    fn cancel_never_clicks() {
        let mut r = Recognizer::default();
        r.touch(1, Phase::Down, A, 0.0);
        let out = r.touch(1, Phase::Cancel, A, 0.05);
        assert_eq!(out, vec![Out::Gone]);
    }

    #[test]
    fn two_quick_taps_are_two_clicks_for_egui_to_pair_as_a_double_click() {
        let mut r = Recognizer::default();
        let mut clicks = 0;
        for (i, t) in [0.0, 0.2].into_iter().enumerate() {
            r.touch(i as u64, Phase::Down, A, t);
            let o = r.touch(i as u64, Phase::Up, A, t + 0.05);
            clicks += o.iter().filter(|o| matches!(o, Out::Release(Btn::Primary, _))).count();
        }
        assert_eq!(clicks, 2);
    }
}
