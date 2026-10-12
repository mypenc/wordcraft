//! Word's floating mini toolbar: the small translucent bar over a non-collapsed text selection
//! with the most-used formatting controls. It mirrors Word's trigger (selection, not caret),
//! position (above, clamped to the viewport, flipping below when tight) and dismissal rules.

use egui::{Align, CornerRadius, Layout, Order, Rect, Sense, Vec2, pos2};
use serde_json::{Value, json};

use crate::theme::Tokens;
use crate::{WordApp, widgets};

/// Vertical gap between the bar and the selection's first line.
const VGAP: f32 = 8.0;
/// Distance from the selection's left edge at which the bar is centred.
const ANCHOR_INSET: f32 = 4.0;
/// Padding inside the bar.
const PAD_X: f32 = 4.0;
const PAD_Y: f32 = 3.0;
/// Height of one row of controls (the ribbon's 22 pt buttons plus a little air), and the gap
/// between the two rows.
const ROW_H: f32 = 24.0;
const ROW_GAP: f32 = 2.0;
/// Horizontal gap between neighbouring controls.
const SPACING: f32 = 1.0;
/// The ribbon's widget metrics: a small icon button, a split button (icon part plus its 11 pt
/// arrow, two allocations), a group divider and the size box.
const BTN_W: f32 = 24.0;
const SPLIT_W: f32 = 24.0 + SPACING + 11.0;
const SEP_W: f32 = 9.0;
const COMBO_SIZE_W: f32 = 52.0;
/// Gap between the size box and the grow/shrink buttons.
const GAP: f32 = 3.0;
/// Row 2, like Word's: B I U S | highlight, font colour | the four alignments. Fourteen
/// allocations, so thirteen gaps; the two inside the split buttons are counted in `SPLIT_W`.
const ROW2_W: f32 = 8.0 * BTN_W + 2.0 * SPLIT_W + 2.0 * SEP_W + 11.0 * SPACING;
/// Row 1: font, size, grow, shrink. The font box takes what row 2 leaves, so both rows end
/// together and the box starts at the bar's left edge.
const COMBO_FONT_W: f32 = ROW2_W - (COMBO_SIZE_W + GAP + 2.0 * BTN_W + 3.0 * SPACING);

/// The bar's size: fixed, so it never jitters as the selection changes, and wide enough for every
/// control of both rows (#535: one row of a fixed width cut off everything after Align Left).
pub const BAR_W: f32 = PAD_X * 2.0 + ROW2_W;
pub const BAR_H: f32 = PAD_Y * 2.0 + ROW_H * 2.0 + ROW_GAP;

/// Where the bar sits for a selection line and viewport; `None` for degenerate inputs (NaN
/// rects must never reach egui). Pure geometry, unit tested without a window.
pub fn anchor_rect(selection_top: f32, selection_left: f32, viewport: Rect) -> Option<Rect> {
    if !selection_top.is_finite() || !selection_left.is_finite() || !viewport.is_finite() || !viewport.is_positive() {
        return None;
    }
    // Horizontal: centre on the selection's start (a few points in), clamped inside the viewport.
    let cx = (selection_left + ANCHOR_INSET).clamp(viewport.left(), viewport.right());
    let x = (cx - BAR_W / 2.0).clamp(viewport.left(), (viewport.right() - BAR_W).max(viewport.left()));
    // Vertical: above the selection, unless there is no room, then below it.
    let above = selection_top - VGAP - BAR_H;
    let y = if above >= viewport.top() { above } else { selection_top + VGAP };
    Some(Rect::from_min_size(pos2(x, y), Vec2::new(BAR_W, BAR_H)))
}

/// Where the bar and the controls in it were drawn last frame (`[bar, controls]`), kept in the
/// context's memory under this id so tests can check that no control spills out of the bar.
pub fn shown_id() -> egui::Id {
    egui::Id::new("wc_mini_toolbar").with("shown")
}

/// Whether the bar may be shown for the current app state.
pub fn visible(app: &WordApp) -> bool {
    // Reading Mode permits text selection and copying, but never formatting controls.
    if app.session.view.read_mode {
        return false;
    }
    // Never over a modal dialog or the Backstage.
    if app.dialog.is_some() || app.colors.is_some() || app.ui.backstage {
        return false;
    }
    // Never while the right-click context menu is open: it owns the pointer and would double up.
    if app.canvas.context_menu_open {
        return false;
    }
    // Never while a drag-select is in progress (Word shows it on mouse-up).
    if app.canvas.drag_selecting() {
        return false;
    }
    // Never while keytips are showing: the ribbon owns the letters, and the bar's buttons reuse
    // the ribbon's widgets, so they would record duplicate badges.
    if app.ui.keytips != crate::keytips::Phase::Off {
        return false;
    }
    // Never for a collapsed caret.
    !app.session.sel.is_collapsed()
}

/// Show the bar: `selection` is the screen rect of the selection's first line, `viewport` the
/// canvas area it must stay inside.
pub fn show(app: &mut WordApp, ctx: &egui::Context, selection: Rect, viewport: Rect) {
    if !visible(app) {
        return;
    }
    let Some(bar) = anchor_rect(selection.min.y, selection.min.x, viewport) else { return };
    let t = Tokens::get(ctx);
    // The bar sits above the document but below every popup, and is not movable: it is an overlay
    // pinned to the selection, so moving it would lose the association.
    // Draw the bar pinned above the selection. egui stops a click at the topmost widget, so a
    // click on the bar never reaches the document underneath.
    let _area = egui::Area::new(egui::Id::new("wc_mini_toolbar")).order(Order::Tooltip).interactable(true).fixed_pos(bar.min);
    _area.show(ctx, |ui| {
        ui.set_clip_rect(bar);
        let painter = ui.painter();
        painter.rect_filled(bar.translate(Vec2::new(0.0, 2.0)), CornerRadius::same(6), t.page_shadow);
        painter.rect_filled(bar, CornerRadius::same(6), t.menu);
        painter.rect_stroke(bar, CornerRadius::same(6), egui::Stroke::new(1.0, t.border_strong), egui::StrokeKind::Inside);
        let inner = bar.shrink2(Vec2::new(PAD_X, PAD_Y));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)));
        child.spacing_mut().item_spacing = Vec2::new(SPACING, ROW_GAP);
        content(app, &mut child, &t);
        let used = child.min_rect();
        ctx.data_mut(|d| d.insert_temp(shown_id(), [bar, used]));
    });
}

/// The controls in two rows like Word's mini toolbar, reusing the ribbon's widgets so the bar
/// matches the ribbon exactly.
fn content(app: &mut WordApp, ui: &mut egui::Ui, t: &Tokens) {
    let st: Value = app.session.run("format.state", &json!({})).unwrap_or_default();
    let flag = |k: &str| st.get(k).and_then(Value::as_bool).unwrap_or(false);
    let align = app.session.doc.para_at(&app.session.sel.focus).map(|p| app.session.doc.styles.resolve_para(&p.props).align).unwrap_or_default();
    let row = |ui: &mut egui::Ui, app: &mut WordApp, add: &dyn Fn(&mut egui::Ui, &mut WordApp)| {
        ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), ROW_H), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_height(ROW_H);
            add(ui, app);
        });
    };

    row(ui, app, &|ui, app| {
        let font = st.get("font").and_then(Value::as_str).unwrap_or("").to_string();
        let font = app.previews.font_shown(&font);
        let fams = app.previews.families();
        let prev = app.previews.font_preview_fn();
        if let Some(f) = widgets::font_combo(ui, "mini_font", COMBO_FONT_W, &font, &fams, Some(&*prev)) {
            let _ = if f == crate::widgets::ADD_FONT {
                app.run("font.add", json!({}))
            } else {
                let f = app.previews.font_stored(&f);
                app.run("format.font", json!({"name": f}))
            };
        }
        let size =
            st.get("size").and_then(Value::as_f64).map(|s| if s.fract() == 0.0 { format!("{s:.0}") } else { s.to_string() }).unwrap_or_default();
        let sizes: Vec<String> =
            wordcraft_engine::cmd::format::SIZES.iter().map(|s| if s.fract() == 0.0 { format!("{s:.0}") } else { s.to_string() }).collect();
        if let Some(v) = widgets::combo(ui, "mini_size", COMBO_SIZE_W, &size, &sizes, None)
            && let Ok(x) = v.trim().parse::<f64>()
        {
            let _ = app.run("format.size", json!({"size": x}));
        }
        ui.add_space(GAP);
        widgets::small(ui, app, "grow", None, "Increase Font Size", "format.growFont", json!({}), false);
        widgets::small(ui, app, "shrink", None, "Decrease Font Size", "format.shrinkFont", json!({}), false);
    });

    row(ui, app, &|ui, app| {
        widgets::small(ui, app, "bold", None, "Bold", "format.bold", json!({}), flag("bold"));
        widgets::small(ui, app, "italic", None, "Italic", "format.italic", json!({}), flag("italic"));
        widgets::small(ui, app, "underline", None, "Underline", "format.underline", json!({}), flag("underline"));
        widgets::small(ui, app, "strike", None, "Strikethrough", "format.strikethrough", json!({}), flag("strike"));
        sep(ui, t);

        let hl = app.canvas.last_highlight.clone();
        widgets::split(ui, app, "highlight", "Text Highlight Color", "format.highlight", json!({"color": hl}), false, None, |ui, app| {
            highlight_menu(ui, app);
        });
        let fc = app.canvas.last_font_color.clone();
        let sw = wordcraft_doc::Rgb::parse(&fc).map(crate::theme::c32);
        widgets::split(ui, app, "fontcolor", "Font Color", "format.color", json!({"color": fc}), false, sw, |ui, app| {
            font_color_menu(ui, app);
        });
        sep(ui, t);

        use wordcraft_doc::Align as A;
        widgets::small(ui, app, "alignLeft", None, "Align Left", "para.alignLeft", json!({}), align == A::Left);
        widgets::small(ui, app, "alignCenter", None, "Center", "para.alignCenter", json!({}), align == A::Center);
        widgets::small(ui, app, "alignRight", None, "Align Right", "para.alignRight", json!({}), align == A::Right);
        widgets::small(ui, app, "justify", None, "Justify", "para.justify", json!({}), align == A::Justify);
    });
}

/// A hairline divider between the bar's groups.
fn sep(ui: &mut egui::Ui, t: &Tokens) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(SEP_W, 20.0), Sense::hover());
    let p = ui.painter();
    p.vline(r.center().x, r.y_range().shrink(4.0), egui::Stroke::new(1.0, t.border));
}

/// Highlight colour grid, matching the ribbon's.
fn highlight_menu(ui: &mut egui::Ui, app: &mut WordApp) {
    egui::Grid::new("mini_hl").spacing(Vec2::new(3.0, 3.0)).show(ui, |ui| {
        for (i, h) in wordcraft_doc::props::Highlight::ALL.iter().enumerate().skip(1) {
            let Some(c) = h.rgb() else { continue };
            let (r, resp) = ui.allocate_exact_size(Vec2::new(18.0, 18.0), Sense::click());
            ui.painter().rect_filled(r, 2.0, egui::Color32::from_rgb(c.0, c.1, c.2));
            if resp.on_hover_text(h.name()).clicked() {
                app.canvas.last_highlight = h.ooxml().to_string();
                let _ = app.run("format.highlight", json!({"color": h.ooxml()}));
                ui.close();
            }
            if i % 5 == 0 {
                ui.end_row();
            }
        }
    });
    if ui.selectable_label(false, "No Color").clicked() {
        let _ = app.run("format.highlight", json!({"color": "none"}));
        ui.close();
    }
}

/// Font colour: Automatic, the colour grid, Recent Colors and More Colors… (#606).
fn font_color_menu(ui: &mut egui::Ui, app: &mut WordApp) {
    egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
        if ui.selectable_label(false, "Automatic").clicked() {
            let _ = app.run("format.color", json!({"color": "auto"}));
            ui.close();
            return;
        }
        let theme = app.session.doc.settings.theme_colors.clone();
        let target = crate::colors::Target::cmd("format.color").remember(crate::colors::Remember::FontColor);
        crate::colors::menu(ui, app, &theme, target);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vp() -> Rect {
        Rect::from_min_size(pos2(100.0, 50.0), Vec2::new(1200.0, 700.0))
    }

    #[test]
    fn reading_mode_hides_formatting_even_with_an_existing_selection() {
        let mut app = WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), Default::default());
        app.run("text.insert", json!({"text": "Selectable text"})).unwrap();
        app.run("select.all", json!({})).unwrap();
        assert!(!app.session.sel.is_collapsed());
        assert!(visible(&app), "selection shows the formatting bar in editing mode");

        app.run("view.readMode", json!({})).unwrap();
        assert!(!app.session.sel.is_collapsed(), "entering Reading Mode must preserve the selection");
        assert!(!visible(&app), "formatting bar must not be accessible in Reading Mode");

        app.run("view.printLayout", json!({})).unwrap();
        assert!(visible(&app), "returning to editing restores the formatting bar");
    }

    #[test]
    fn sits_above_the_selection_when_there_is_room() {
        let left = vp().center().x;
        let r = anchor_rect(400.0, left, vp()).unwrap_or(Rect::EVERYTHING);
        assert_eq!(r.size(), Vec2::new(BAR_W, BAR_H));
        assert!((r.max.y - (400.0 - VGAP)).abs() < 1e-3, "bar bottom should clear the selection by VGAP");
        assert!(r.max.y < 400.0, "the bar must be above the selection");
        assert!((r.center().x - (left + ANCHOR_INSET)).abs() < 1e-3, "bar should be centred on the selection's left edge");
    }

    #[test]
    fn centred_unless_the_viewport_forces_a_shift() {
        let left = vp().min.x + 20.0;
        let r = anchor_rect(400.0, left, vp()).unwrap_or(Rect::EVERYTHING);
        assert!((r.min.x - vp().min.x).abs() < 1e-3, "the bar must be pushed onto the viewport's left edge");
        let right = vp().max.x - 20.0;
        let r = anchor_rect(400.0, right, vp()).unwrap_or(Rect::EVERYTHING);
        assert!((r.max.x - vp().max.x).abs() < 1e-3, "the bar must be pushed onto the viewport's right edge");
    }

    #[test]
    fn flips_below_when_there_is_no_room_above() {
        let r = anchor_rect(60.0, 300.0, vp()).unwrap_or(Rect::EVERYTHING);
        assert!((r.min.y - (60.0 + VGAP)).abs() < 1e-3, "bar top should clear the selection by VGAP below");
        assert!(r.min.y > 60.0, "the bar must be below the selection");
    }

    #[test]
    fn never_leaves_the_viewport() {
        for left in [0.0, 130.0, 400.0, 2000.0, -500.0] {
            let r = anchor_rect(400.0, left, vp()).unwrap_or(Rect::EVERYTHING);
            assert!(r.min.x >= vp().min.x - 1e-3, "left {left}: bar left {} out of viewport", r.min.x);
            assert!(r.max.x <= vp().max.x + 1e-3, "left {left}: bar right {} out of viewport", r.max.x);
        }
    }

    #[test]
    fn degenerate_inputs_are_rejected_not_crashed() {
        assert!(anchor_rect(f32::NAN, 10.0, vp()).is_none());
        assert!(anchor_rect(10.0, f32::NAN, vp()).is_none());
        assert!(anchor_rect(f32::INFINITY, 10.0, vp()).is_none());
        assert!(anchor_rect(10.0, f32::NEG_INFINITY, vp()).is_none());
        assert!(anchor_rect(10.0, 10.0, Rect::EVERYTHING).is_none());
        assert!(anchor_rect(10.0, 10.0, Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(0.0, 100.0))).is_none());
    }

    #[test]
    fn a_viewport_narrower_than_the_bar_still_yields_a_rect() {
        let r = anchor_rect(10.0, 5.0, Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(20.0, 20.0)));
        let r = r.unwrap_or(Rect::EVERYTHING);
        assert!(r.width() > 0.0 && r.height() > 0.0, "a tiny viewport must still place a positive-size bar");
        assert!(r.min.x.is_finite() && r.min.y.is_finite());
    }

    /// #535: the bar was one fixed-width row that cut off everything after Align Left. Draw it
    /// over a real selection: every control fits inside the bar, and the bar is no wider than its
    /// controls (both rows end together).
    #[test]
    fn every_control_fits_inside_the_bar() {
        use wordcraft_engine::Session;
        let mut a = WordApp::new(Session::new(wordcraft_doc::Document::new()), Default::default());
        a.run("text.insert", json!({"text": "The quick brown fox jumps over the lazy dog"})).unwrap();
        a.run("select.text", json!({"text": "brown fox"})).unwrap();
        let mut h = egui_kittest::Harness::builder().with_size(Vec2::new(1440.0, 900.0)).build_ui_state(
            |ui, app: &mut WordApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            a,
        );
        for _ in 0..6 {
            h.step();
        }
        let [bar, used] = h.ctx.data(|d| d.get_temp::<[Rect; 2]>(shown_id())).expect("the bar is shown over a selection");
        let inner = bar.shrink2(Vec2::new(PAD_X, PAD_Y));
        assert!(
            inner.expand(0.1).contains_rect(used),
            "controls {:?} spill out of the bar {:?}",
            [used.min.x, used.min.y, used.max.x, used.max.y],
            [inner.min.x, inner.min.y, inner.max.x, inner.max.y]
        );
        assert!((used.width() - inner.width()).abs() < 0.5, "the bar ({}) should be as wide as its controls ({})", inner.width(), used.width());
        assert!(used.min.x - inner.min.x < 0.5, "the font box starts at the bar's left edge");
    }
}
