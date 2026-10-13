//! File › Print (#641): the print settings and a preview of the pages.
//!
//! Page range and parity shape the PDF. Native printer settings and copy collation go to
//! the shared spooler; browser printing and saved print PDFs retain copies in the PDF.

use std::sync::Arc;

use egui::{Rect, Sense, Stroke, TextureHandle, Ui, pos2, vec2};
use serde_json::{Value, json};
use wordcraft_layout::DocLayout;

use crate::theme::{Tokens, medium, regular};
use crate::{WordApp, icons};

/// The Print page's settings. Not saved with the document or the UI state.
#[derive(Clone, Debug, PartialEq)]
pub struct PrintSettings {
    /// `all`, `current`, `selection` or `custom`.
    pub range: String,
    /// The custom range text, like `1-3, 5`.
    pub pages: String,
    /// `all`, `odd` or `even`.
    pub parity: String,
    pub copies: u32,
    pub collate: bool,
    /// The page shown in the preview (0-based).
    pub preview: usize,
}

impl Default for PrintSettings {
    fn default() -> Self {
        PrintSettings { range: "all".into(), pages: String::new(), parity: "all".into(), copies: 1, collate: true, preview: 0 }
    }
}

impl PrintSettings {
    /// The params of `ui.print` and `file.printPdf` for these settings.
    pub fn params(&self) -> Value {
        let mut v = json!({"range": self.range, "parity": self.parity, "copies": self.copies, "collate": self.collate});
        if self.range == "custom" {
            v["pages"] = json!(self.pages);
        }
        v
    }
}

const RANGES: [(&str, &str); 4] = [("all", "All pages"), ("current", "Current page"), ("selection", "Selected pages"), ("custom", "Custom range")];
const PARITIES: [(&str, &str); 3] = [("all", "All pages in the range"), ("odd", "Odd pages only"), ("even", "Even pages only")];

/// A choice list over `(id, label)` pairs; true when the choice changed.
fn choice(ui: &mut Ui, salt: &str, current: &mut String, items: &[(&str, &str)]) -> bool {
    let shown = items.iter().find(|(id, _)| *id == current.as_str()).map_or("", |(_, l)| *l);
    let mut changed = false;
    egui::ComboBox::from_id_salt(salt).width(260.0).selected_text(tl!(shown)).show_ui(ui, |ui| {
        for (id, label) in items {
            if ui.selectable_label(current == id, tl!(label)).clicked() && current != id {
                *current = (*id).to_string();
                changed = true;
            }
        }
    });
    changed
}

/// The output layout, kept while the document is unchanged (laying out every frame would be slow).
fn layout_of(app: &WordApp, ctx: &egui::Context) -> Arc<DocLayout> {
    let id = egui::Id::new("print_page_layout");
    let rev = app.session.rev();
    if let Some((r, l)) = ctx.data(|d| d.get_temp::<(u64, Arc<DocLayout>)>(id))
        && r == rev
    {
        return l;
    }
    let l = app.session.export_layout();
    ctx.data_mut(|d| d.insert_temp(id, (rev, l.clone())));
    l
}

/// The texture of one page, re-rendered only when the document, page or size changes.
fn page_texture(app: &WordApp, ctx: &egui::Context, lay: &DocLayout, page: usize, width: f32) -> Option<TextureHandle> {
    let id = egui::Id::new("print_page_texture");
    let key = (app.session.rev(), page, width as u32, (ctx.pixels_per_point() * 100.0) as u32);
    if let Some((k, t)) = ctx.data(|d| d.get_temp::<((u64, usize, u32, u32), TextureHandle)>(id))
        && k == key
    {
        return Some(t);
    }
    let pg = lay.pages.get(page)?;
    let scale = (width / pg.w.max(1.0) * ctx.pixels_per_point()).clamp(0.05, 4.0);
    let img = wordcraft_render::render_page(&app.session.doc, pg, scale, &Default::default());
    let color = egui::ColorImage::from_rgba_unmultiplied([img.width as usize, img.height as usize], &img.to_straight());
    let tex = ctx.load_texture("print_page_preview", color, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, (key, tex.clone())));
    Some(tex)
}

/// The Print page: settings on the left, the preview on the right.
pub fn show(app: &mut WordApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    let lay = layout_of(app, &ctx);
    let total = lay.pages.len();
    let current = app.session.current_pages(&lay);
    app.ui.print.preview = app.ui.print.preview.min(total.saturating_sub(1));

    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(380.0);
            ui.spacing_mut().item_spacing.y = 8.0;
            let native = app.services.print_native.is_some();
            let hook = native || app.services.print.is_some();
            if native {
                if let Some(discover) = &app.services.printers {
                    crate::printing::show(&mut app.printing, ui, discover, |label| crate::i18n::t(label).to_owned());
                }
                app.ui.print.copies = app.printing.settings.copies;
            } else if hook {
                ui.label(tl!("Send the document straight to the system print dialog, or save a copy in another format below."));
                ui.label(
                    egui::RichText::new(tl!("The printer, two-sided printing and pages per sheet are chosen in that dialog.")).color(t.text_dim),
                );
            } else {
                ui.label(tl!("Save a copy in another format. Printing goes through a PDF you can print from any viewer."));
            }
            ui.add_space(4.0);
            if hook
                && ui
                    .add_enabled(
                        !native || app.printing.ready(),
                        egui::Button::new(egui::RichText::new(tl!("Print")).font(medium(13.5))).min_size(vec2(320.0, 34.0)),
                    )
                    .clicked()
            {
                let params = app.ui.print.params();
                let _ = app.run("ui.print", params);
            }
            if ui
                .add(egui::Button::new(egui::RichText::new(tl!("Save these pages as PDF…")).font(medium(13.5))).min_size(vec2(320.0, 34.0)))
                .clicked()
            {
                app.print_pdf_dialog();
            }
            ui.add_space(6.0);

            if native {
                if app.ui.print.copies > 1 {
                    ui.add_enabled(!app.printing.busy(), egui::Checkbox::new(&mut app.ui.print.collate, tl!("Collate copies")));
                }
            } else {
                ui.label(egui::RichText::new(tl!("Copies")).font(medium(13.0)));
                ui.horizontal(|ui| {
                    let mut n = app.ui.print.copies;
                    ui.add(egui::DragValue::new(&mut n).range(1..=wordcraft_engine::print::MAX_COPIES).speed(0.2));
                    app.ui.print.copies = n.clamp(1, wordcraft_engine::print::MAX_COPIES);
                    if app.ui.print.copies > 1 {
                        ui.checkbox(&mut app.ui.print.collate, tl!("Collate copies"));
                    }
                });
            }

            ui.label(egui::RichText::new(tl!("Pages")).font(medium(13.0)));
            if choice(ui, "print_range", &mut app.ui.print.range, &RANGES) {
                // Show the first page that will print.
                app.ui.print.preview = if app.ui.print.range == "all" { 0 } else { current.0.min(total.saturating_sub(1)) };
            }
            if app.ui.print.range == "custom" {
                ui.add(
                    egui::TextEdit::singleline(&mut app.ui.print.pages)
                        .hint_text(tl!("Pages like 1-3, 5"))
                        .desired_width(260.0)
                        .char_limit(wordcraft_engine::print::MAX_SPEC_CHARS),
                );
            }
            choice(ui, "print_parity", &mut app.ui.print.parity, &PARITIES);

            match wordcraft_engine::print::plan(&app.ui.print.params(), total, current) {
                Ok(plan) => {
                    let msg = crate::i18n::fmt(
                        tl!("{count} of {total} pages will print"),
                        &[("count", &plan.chosen.to_string()), ("total", &total.to_string())],
                    );
                    ui.label(egui::RichText::new(msg).color(t.text_dim));
                }
                Err(e) => {
                    ui.label(egui::RichText::new(e.to_string()).color(t.red));
                }
            }
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);
            if hook {
                ui.label(tl!("Or save a copy in another format:"));
            }
            export_formats(app, ui);
        });
        ui.add_space(24.0);
        ui.vertical(|ui| preview(app, ui, &lay, total));
    });
}

/// The preview: one page with Previous/Next.
fn preview(app: &mut WordApp, ui: &mut Ui, lay: &DocLayout, total: usize) {
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    let width = 340.0;
    let page = app.ui.print.preview;
    let aspect = lay.pages.get(page).map_or(11.0 / 8.5, |p| p.h.max(1.0) / p.w.max(1.0)).clamp(0.2, 5.0);
    let (r, _) = ui.allocate_exact_size(vec2(width, width * aspect), Sense::hover());
    ui.painter().rect_filled(r.translate(vec2(2.0, 3.0)), 0.0, t.page_shadow);
    ui.painter().rect(r, 0.0, egui::Color32::WHITE, Stroke::new(1.0, t.border_strong), egui::StrokeKind::Inside);
    if let Some(tex) = page_texture(app, &ctx, lay, page, width) {
        ui.painter().image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), egui::Color32::WHITE);
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let nav = |ui: &mut Ui, icon: &str, tip: &str, enabled: bool| {
            let (r, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
            let col = if enabled { t.icon } else { t.text_disabled };
            if enabled && resp.hovered() {
                ui.painter().rect_filled(r, 3.0, t.hover);
            }
            icons::paint(ui.painter(), r.shrink(4.0), icon, col, col);
            enabled && resp.on_hover_text(tip).clicked()
        };
        if nav(ui, "chevronLeft", tl!("Previous Page"), page > 0) {
            app.ui.print.preview = page - 1;
        }
        let label = crate::i18n::fmt(tl!("Page {page} of {pages}"), &[("page", &(page + 1).to_string()), ("pages", &total.to_string())]);
        ui.label(egui::RichText::new(label).font(regular(13.0)));
        if nav(ui, "chevronRight", tl!("Next Page"), page + 1 < total) {
            app.ui.print.preview = page + 1;
        }
    });
}

/// The "save a copy" formats (File › Export, and below the settings on the Print page).
pub fn export_formats(app: &mut WordApp, ui: &mut Ui) {
    for (label, ext) in [
        ("PDF document (*.pdf)", "pdf"),
        ("Word document (*.docx)", "docx"),
        ("Word XML Document (*.xml)", "xml"),
        ("OpenDocument Text (*.odt)", "odt"),
        ("Rich Text Format (*.rtf)", "rtf"),
        ("Web page (*.html)", "html"),
        ("Markdown (*.md)", "md"),
        ("LaTeX (*.tex)", "tex"),
        ("Plain text (*.txt)", "txt"),
        ("Page image (*.png)", "png"),
    ] {
        if ui.add(egui::Button::new(egui::RichText::new(tl!(label)).font(medium(13.5))).min_size(vec2(320.0, 34.0))).clicked() {
            app.export_dialog(ext);
        }
        ui.add_space(4.0);
    }
}
