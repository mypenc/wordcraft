//! Touch input for phones and tablets (Android), layered over the mouse-and-keyboard UI.
//!
//! The panels are written for a mouse and keyboard. Rather than touching 46k lines of widgets,
//! this layer sits in [`WordApp::raw_input_hook`](crate::WordApp) and *translates*
//! fingers into the events those widgets already understand ([`gesture`] has the table):
//!
//! * a long press becomes a right click, so every context menu works;
//! * a tap or drag becomes a left click or drag (the press is deferred until it is known not
//!   to be a long press);
//! * two fingers scroll, one finger scrolls on the canvas background, pinch zooms (egui's own multi-touch, which this layer leaves alone),
//!   a quick two-finger tap undoes and a three-finger tap redoes;
//! * an on-screen bar holds the keys a finger can't: Shift, Ctrl, Alt and Space (sticky, and
//!   double-tap to lock), plus Delete, Esc, Enter, Undo, Redo and the soft keyboard.
//!
//! A real mouse or keyboard is never touched: only events that arrive in a frame with
//! `Event::Touch`, or while a finger is down, are rewritten. With the patched winit in
//! `android/patches/` a mouse arrives as ordinary pointer events and passes straight through.

pub mod gesture;

use std::collections::VecDeque;

use egui::{Event, Key, Modifiers, MouseWheelUnit, PointerButton, Pos2, TouchPhase, Vec2};
use gesture::{Btn, Out, Phase, Pt, Recognizer};

/// Something the app must do on the layer's behalf (it owns the session).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Undo,
    Redo,
}

/// One-shot or locked modifier / Space held by the on-screen bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Sticky {
    on: bool,
    locked: bool,
}

#[derive(Default)]
pub struct Touch {
    /// Touch layer active. On by default on Android; also switched on by the first touch seen
    /// on a desktop touchscreen.
    pub enabled: bool,
    /// Show the on-screen modifier bar.
    pub bar_visible: bool,
    /// The user asked to toggle the soft keyboard.
    pub keyboard_request: bool,
    rec: Recognizer,
    queue: VecDeque<Event>,
    shift: Sticky,
    ctrl: Sticky,
    alt: Sticky,
    space: Sticky,
    actions: Vec<Action>,
    /// A short vibration (the host sets it: the long press that opened a context menu).
    pub haptic: Option<Box<dyn Fn()>>,
    /// The theme was (re)installed: apply the touch metrics again.
    pub style_dirty: bool,
    collapsed: bool,
}

impl Touch {
    pub fn new() -> Self {
        let android = cfg!(target_os = "android");
        Self { enabled: android, bar_visible: android, style_dirty: true, ..Default::default() }
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    fn mods(&self) -> Modifiers {
        let ctrl = self.ctrl.on;
        Modifiers { alt: self.alt.on, ctrl, shift: self.shift.on, mac_cmd: false, command: ctrl }
    }

    fn any_sticky(&self) -> bool {
        self.shift.on || self.ctrl.on || self.alt.on
    }

    /// One-shot modifiers fall off after the click they were armed for.
    fn consume_one_shots(&mut self) {
        for s in [&mut self.shift, &mut self.ctrl, &mut self.alt] {
            if !s.locked {
                s.on = false;
            }
        }
    }

    /// Run from `raw_input_hook`, before egui sees this frame's input.
    pub fn process(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput, canvas: Option<egui::Rect>, pages: &[egui::Rect]) {
        let now = raw.time.unwrap_or_else(|| ctx.input(|i| i.time));
        let mut outs = Vec::new();
        let mut touch_frame = false;
        for e in &raw.events {
            if let Event::Touch { id, phase, pos, .. } = e {
                touch_frame = true;
                if !self.enabled {
                    // A touchscreen on a desktop: switch the touch layer on.
                    self.enabled = true;
                    self.bar_visible = true;
                    self.style_dirty = true;
                }
                let phase = match phase {
                    TouchPhase::Start => Phase::Down,
                    TouchPhase::Move => Phase::Move,
                    TouchPhase::End => Phase::Up,
                    TouchPhase::Cancel => Phase::Cancel,
                };
                let scroll_on_drag = phase == Phase::Down
                    && canvas.is_some_and(|r| r.contains(*pos))
                    && !pages.iter().any(|r| r.contains(*pos));
                outs.extend(self.rec.touch_with_scroll(id.0, phase, Pt::new(pos.x, pos.y), now, scroll_on_drag));
            }
        }
        if self.enabled {
            outs.extend(self.rec.tick(now));
            // egui-winit also turns the first finger into a left-button mouse; the recognizer
            // replaces that, so drop it. Real-mouse events (no Touch in the frame) survive.
            if touch_frame || self.rec.active() {
                raw.events.retain(|e| !matches!(e, Event::PointerMoved(_) | Event::PointerButton { .. } | Event::PointerGone));
            }
        }
        for o in outs {
            self.convert(o);
        }
        if self.enabled {
            self.dispatch(raw);
            if let Some(at) = self.rec.deadline() {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64((at - now).max(0.0)));
            }
            if !self.queue.is_empty() {
                ctx.request_repaint();
            }
            // Sticky modifiers apply to everything this frame carries.
            let m = self.mods();
            if self.any_sticky() {
                for e in &mut raw.events {
                    match e {
                        Event::PointerButton { modifiers, .. } | Event::MouseWheel { modifiers, .. } | Event::Key { modifiers, .. } => {
                            *modifiers = or_mods(*modifiers, m)
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn push(&mut self, e: Event) {
        // Coalesce: a fast pan or drag must not build a backlog behind the one-per-frame rule.
        match (&e, self.queue.back_mut()) {
            (Event::PointerMoved(p), Some(Event::PointerMoved(q))) => *q = *p,
            (Event::MouseWheel { delta, .. }, Some(Event::MouseWheel { delta: d, .. })) => *d += *delta,
            _ => self.queue.push_back(e),
        }
    }

    fn convert(&mut self, o: Out) {
        let pos = |p: Pt| Pos2::new(p.x, p.y);
        let m = self.mods();
        match o {
            Out::Move(p) => self.push(Event::PointerMoved(pos(p))),
            Out::Press(b, p) => {
                let button = if b == Btn::Primary { PointerButton::Primary } else { PointerButton::Secondary };
                self.push(Event::PointerButton { pos: pos(p), button, pressed: true, modifiers: m });
            }
            Out::Release(b, p) => {
                let button = if b == Btn::Primary { PointerButton::Primary } else { PointerButton::Secondary };
                self.push(Event::PointerButton { pos: pos(p), button, pressed: false, modifiers: m });
                self.consume_one_shots();
            }
            Out::Gone => self.push(Event::PointerGone),
            Out::Scroll { at, delta } => {
                self.push(Event::PointerMoved(pos(at)));
                self.push(Event::MouseWheel {
                    unit: MouseWheelUnit::Point,
                    delta: Vec2::new(delta.x, delta.y),
                    phase: TouchPhase::Move,
                    modifiers: m,
                });
            }
            Out::Haptic => {
                if let Some(h) = &self.haptic {
                    h();
                }
            }
            Out::Undo => self.actions.push(Action::Undo),
            Out::Redo => self.actions.push(Action::Redo),
        }
    }

    /// Hand the queued events to egui: pointer movement and wheel in a burst, but a button or
    /// key event alone, so egui sees a press and its release in different frames.
    fn dispatch(&mut self, raw: &mut egui::RawInput) {
        let motion = |e: &Event| matches!(e, Event::PointerMoved(_) | Event::MouseWheel { .. });
        let Some(first) = self.queue.pop_front() else { return };
        let burst = motion(&first);
        raw.events.push(first);
        while burst && self.queue.front().is_some_and(motion) {
            if let Some(e) = self.queue.pop_front() {
                raw.events.push(e);
            }
        }
    }

    fn key(&mut self, key: Key, mods: Modifiers) {
        for pressed in [true, false] {
            self.queue.push_back(Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers: mods });
        }
    }

    /// Touch-sized controls (egui's defaults are mouse-sized). Called once after the theme is
    /// installed, and again when it is reinstalled.
    pub fn apply_style(&mut self, ctx: &egui::Context) {
        if !self.enabled || !self.style_dirty {
            return;
        }
        self.style_dirty = false;
        ctx.all_styles_mut(|s| {
            s.spacing.interact_size = egui::vec2(s.spacing.interact_size.x.max(40.0), s.spacing.interact_size.y.max(34.0));
            s.spacing.button_padding = egui::vec2(s.spacing.button_padding.x.max(10.0), s.spacing.button_padding.y.max(6.0));
            s.spacing.item_spacing = egui::vec2(s.spacing.item_spacing.x.max(8.0), s.spacing.item_spacing.y.max(6.0));
            s.spacing.icon_width = s.spacing.icon_width.max(22.0);
            s.spacing.icon_width_inner = s.spacing.icon_width_inner.max(12.0);
            s.spacing.scroll.bar_width = s.spacing.scroll.bar_width.max(12.0);
            s.interaction.interact_radius = s.interaction.interact_radius.max(10.0);
            s.interaction.resize_grab_radius_side = s.interaction.resize_grab_radius_side.max(14.0);
            s.interaction.resize_grab_radius_corner = s.interaction.resize_grab_radius_corner.max(18.0);
        });
        // Fingers wobble: a press that moves a little is still a click.
        ctx.options_mut(|o| o.input_options.max_click_dist = o.input_options.max_click_dist.max(self.rec.cfg.slop + 2.0));
    }

    /// The on-screen modifier / quick-key bar.
    pub fn show_bar(&mut self, ctx: &egui::Context) {
        if !self.enabled || !self.bar_visible {
            return;
        }
        let min = egui::vec2(46.0, 40.0);
        egui::Area::new(egui::Id::new("effectcraft_touch_bar"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -10.0))
            .movable(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let toggle = |ui: &mut egui::Ui, label: &str, s: &mut Sticky, hint: &str| -> bool {
                            let text = if s.locked { format!("[{label}]") } else { label.to_owned() };
                            let r = ui.add(egui::Button::new(text).selected(s.on).min_size(min)).on_hover_text(hint);
                            if r.double_clicked() {
                                *s = Sticky { on: true, locked: true };
                                true
                            } else if r.clicked() {
                                *s = if s.on { Sticky::default() } else { Sticky { on: true, locked: false } };
                                true
                            } else {
                                false
                            }
                        };
                        toggle(ui, "Shift", &mut self.shift, "Applies to the next tap. Double-tap to lock.");
                        toggle(ui, "Ctrl", &mut self.ctrl, "Applies to the next tap. Double-tap to lock.");
                        toggle(ui, "Alt", &mut self.alt, "Applies to the next tap. Double-tap to lock.");
                        let was = self.space.on;
                        if toggle(ui, "Space", &mut self.space, "Hold-to-pan (hand tool). Stays on until tapped again.") && self.space.on != was {
                            let on = self.space.on;
                            self.queue.push_back(Event::Key {
                                key: Key::Space,
                                physical_key: Some(Key::Space),
                                pressed: on,
                                repeat: false,
                                modifiers: Modifiers::NONE,
                            });
                        }
                        ui.separator();
                        // Enter stays even when the bar is collapsed: the soft keyboard has none.
                            if ui.add(egui::Button::new("Enter").min_size(min)).on_hover_text("Enter").clicked() {
                                self.key(Key::Enter, Modifiers::NONE);
                            }
                        if !self.collapsed {
                            if ui.add(egui::Button::new("Undo").min_size(min)).on_hover_text("Undo").clicked() {
                                self.actions.push(Action::Undo);
                            }
                            if ui.add(egui::Button::new("Redo").min_size(min)).on_hover_text("Redo").clicked() {
                                self.actions.push(Action::Redo);
                            }
                            if ui.add(egui::Button::new("Del").min_size(min)).clicked() {
                                self.key(Key::Delete, Modifiers::NONE);
                            }
                            if ui.add(egui::Button::new("Esc").min_size(min)).clicked() {
                                self.key(Key::Escape, Modifiers::NONE);
                            }
                        }
                        if ui.add(egui::Button::new(if self.collapsed { ">" } else { "<" }).min_size(min)).on_hover_text("Collapse").clicked() {
                            self.collapsed = !self.collapsed;
                        }
                    });
                });
            });
    }

    /// Keep a keyboard toggle visible above the system IME, which can cover the bottom bar.
    pub fn show_keyboard_toggle(&mut self, ctx: &egui::Context) {
        if !self.enabled {
            return;
        }
        egui::Area::new(egui::Id::new("wordcraft_touch_keyboard"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 72.0))
            .show(ctx, |ui| {
                if ui.button("Keyboard").on_hover_text("Show or hide the on-screen keyboard").clicked() {
                    self.keyboard_request = true;
                }
            });
    }
}

fn or_mods(a: Modifiers, b: Modifiers) -> Modifiers {
    Modifiers {
        alt: a.alt || b.alt,
        ctrl: a.ctrl || b.ctrl,
        shift: a.shift || b.shift,
        mac_cmd: a.mac_cmd || b.mac_cmd,
        command: a.command || b.command,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch_event(id: u64, phase: TouchPhase, x: f32, y: f32) -> Event {
        Event::Touch { device_id: egui::TouchDeviceId(0), id: egui::TouchId(id), phase, pos: Pos2::new(x, y), force: None }
    }

    fn raw(time: f64, events: Vec<Event>) -> egui::RawInput {
        egui::RawInput { time: Some(time), events, ..Default::default() }
    }

    fn clicks(frames: &[egui::RawInput], button: PointerButton) -> usize {
        frames.iter().flat_map(|r| &r.events).filter(|e| matches!(e, Event::PointerButton { button: b, pressed: false, .. } if *b == button)).count()
    }

    fn run(t: &mut Touch, ctx: &egui::Context, mut frames: Vec<egui::RawInput>) -> Vec<egui::RawInput> {
        for f in &mut frames {
            t.process(ctx, f, None, &[]);
        }
        frames
    }

    /// Frames after the last touch event so queued events drain (one button event per frame).
    fn idle(from: f64, n: usize) -> Vec<egui::RawInput> {
        (0..n).map(|i| raw(from + i as f64 * 0.016, vec![])).collect()
    }

    #[test]
    fn long_press_reaches_egui_as_one_right_click_and_no_left_click() {
        let ctx = egui::Context::default();
        let mut t = Touch::new();
        t.enabled = true;
        let mut frames =
            vec![raw(0.0, vec![touch_event(1, TouchPhase::Start, 50.0, 50.0), Event::PointerMoved(Pos2::new(50.0, 50.0)), Event::PointerButton {
                pos: Pos2::new(50.0, 50.0),
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            }])];
        frames.extend((1..40).map(|i| raw(i as f64 * 0.016, vec![])));
        frames.push(raw(0.7, vec![
            touch_event(1, TouchPhase::End, 50.0, 50.0),
            Event::PointerButton { pos: Pos2::new(50.0, 50.0), button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE },
            Event::PointerGone,
        ]));
        frames.extend(idle(0.72, 8));
        let out = run(&mut t, &ctx, frames);
        assert_eq!(clicks(&out, PointerButton::Secondary), 1);
        assert_eq!(clicks(&out, PointerButton::Primary), 0, "egui-winit's emulated left button is dropped");
    }

    #[test]
    fn a_tap_is_one_left_click_and_the_raw_touch_still_reaches_egui() {
        let ctx = egui::Context::default();
        let mut t = Touch::new();
        t.enabled = true;
        let mut frames = vec![
            raw(0.0, vec![touch_event(1, TouchPhase::Start, 10.0, 10.0), Event::PointerButton {
                pos: Pos2::new(10.0, 10.0),
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            }]),
            raw(0.08, vec![touch_event(1, TouchPhase::End, 10.0, 10.0), Event::PointerButton {
                pos: Pos2::new(10.0, 10.0),
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }]),
        ];
        frames.extend(idle(0.1, 6));
        let out = run(&mut t, &ctx, frames);
        assert_eq!(clicks(&out, PointerButton::Primary), 1);
        assert!(out.iter().flat_map(|r| &r.events).any(|e| matches!(e, Event::Touch { .. })), "multi-touch needs the raw touches");
    }

    #[test]
    fn one_finger_scrolls_canvas_gutters_but_keeps_page_drags_for_selection() {
        let ctx = egui::Context::default();
        let canvas = egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 400.0));
        let page = egui::Rect::from_min_size(Pos2::new(80.0, 40.0), Vec2::new(140.0, 220.0));
        let mut touch = Touch::new();
        touch.enabled = true;

        let mut start = raw(0.0, vec![touch_event(1, TouchPhase::Start, 20.0, 100.0)]);
        touch.process(&ctx, &mut start, Some(canvas), &[page]);
        let mut move_bg = raw(0.1, vec![touch_event(1, TouchPhase::Move, 20.0, 130.0)]);
        touch.process(&ctx, &mut move_bg, Some(canvas), &[page]);
        assert!(move_bg.events.iter().any(|e| matches!(e, Event::MouseWheel { .. })), "canvas gutter drag scrolls");
        let mut end_bg = raw(0.2, vec![touch_event(1, TouchPhase::End, 20.0, 130.0)]);
        touch.process(&ctx, &mut end_bg, Some(canvas), &[page]);

        let mut page_start = raw(1.0, vec![touch_event(2, TouchPhase::Start, 100.0, 100.0)]);
        touch.process(&ctx, &mut page_start, Some(canvas), &[page]);
        let mut page_move = raw(1.1, vec![touch_event(2, TouchPhase::Move, 100.0, 130.0)]);
        touch.process(&ctx, &mut page_move, Some(canvas), &[page]);
        assert!(!page_move.events.iter().any(|e| matches!(e, Event::MouseWheel { .. })), "page drag remains a selection gesture");
    }

    #[test]
    fn a_real_mouse_is_never_rewritten() {
        let ctx = egui::Context::default();
        let mut t = Touch::new();
        t.enabled = true;
        let click =
            |pressed| Event::PointerButton { pos: Pos2::new(5.0, 5.0), button: PointerButton::Secondary, pressed, modifiers: Modifiers::NONE };
        let out = run(&mut t, &ctx, vec![raw(0.0, vec![Event::PointerMoved(Pos2::new(5.0, 5.0)), click(true)]), raw(0.1, vec![click(false)])]);
        assert_eq!(clicks(&out, PointerButton::Secondary), 1);
        assert!(matches!(out[0].events[0], Event::PointerMoved(_)));
    }

    #[test]
    fn armed_shift_applies_to_the_next_tap_only() {
        let ctx = egui::Context::default();
        let mut t = Touch::new();
        t.enabled = true;
        t.shift = Sticky { on: true, locked: false };
        let mut frames =
            vec![raw(0.0, vec![touch_event(1, TouchPhase::Start, 1.0, 1.0)]), raw(0.05, vec![touch_event(1, TouchPhase::End, 1.0, 1.0)])];
        frames.extend(idle(0.07, 6));
        let out = run(&mut t, &ctx, frames);
        let shifted = out.iter().flat_map(|r| &r.events).filter(|e| matches!(e, Event::PointerButton { modifiers, .. } if modifiers.shift)).count();
        assert_eq!(shifted, 2, "press and release carry Shift");
        assert!(!t.shift.on, "one-shot: released after the click");
    }

    #[test]
    fn locked_modifiers_survive_clicks() {
        let mut t = Touch::new();
        t.ctrl = Sticky { on: true, locked: true };
        t.consume_one_shots();
        assert!(t.ctrl.on);
    }

    #[test]
    fn a_two_finger_pan_does_not_pile_up_events() {
        let ctx = egui::Context::default();
        let mut t = Touch::new();
        t.enabled = true;
        let mut frames = vec![raw(0.0, vec![touch_event(1, TouchPhase::Start, 10.0, 10.0), touch_event(2, TouchPhase::Start, 50.0, 10.0)])];
        for i in 1..30 {
            let y = 10.0 + i as f32 * 3.0;
            frames.push(raw(i as f64 * 0.004, vec![touch_event(1, TouchPhase::Move, 10.0, y), touch_event(2, TouchPhase::Move, 50.0, y)]));
        }
        let out = run(&mut t, &ctx, frames);
        let total: f32 =
            out.iter().flat_map(|r| &r.events).filter_map(|e| if let Event::MouseWheel { delta, .. } = e { Some(delta.y) } else { None }).sum();
        assert!(t.queue.len() <= 2, "coalesced, not queued per touch sample");
        let queued: f32 = t.queue.iter().filter_map(|e| if let Event::MouseWheel { delta, .. } = e { Some(delta.y) } else { None }).sum();
        assert!((total + queued - 87.0).abs() < 0.01, "no scroll distance lost: {}", total + queued);
    }
}
