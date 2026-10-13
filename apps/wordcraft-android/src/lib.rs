//! WordCraft for Android.
//!
//! The same engine and egui interface as the desktop app (`apps/wordcraft`), compiled as a
//! `cdylib` that a GameActivity (see `android/`) loads. There is no web view: eframe draws with
//! wgpu (Vulkan, OpenGL ES fallback) straight into the activity's surface.
//!
//! What differs from the desktop app:
//!
//! * **Files.** Android hands apps `content://` URIs, not paths. `Services::file_dialog` goes to
//!   Kotlin's system pickers ([`bridge`]): Open copies the document into the app's folder; Save
//!   writes there and `Exports.kt` mirrors it to the file the user named.
//! * **Input.** Fingers become mouse events in `wordcraft_ui_egui::touch` (long press = right
//!   click, two fingers scroll, one finger scrolls on the canvas background, pinch zooms…) plus
//!   an on-screen Shift/Ctrl/Alt/Space bar and keyboard toggle (Help ▸ Touch Bar hides the bar);
//!   a mouse and keyboard reach egui through the patched winit (`android/patches/`), whose
//!   soft-keyboard text and cursor events also feed the editor.
//! * **Clipboard.** egui-winit has none on Android (it falls back to an in-app one), so Copy and
//!   Cut are mirrored to the system clipboard and the ribbon's Paste reads it.
//! * **No** window geometry, control server, native print, drag and drop or title bar.
//!
//! Everything here is `cfg(target_os = "android")`; on other targets the crate is empty.
#![cfg(target_os = "android")]

mod bridge;
// The desktop app's logger (standard error → logcat, plus a log file); it only needs `log`.
#[path = "../../wordcraft/src/logging.rs"]
mod logging;

use std::sync::Arc;
use std::time::{Duration, Instant};

use android_activity::AndroidApp;
use android_activity::input::{Axis, ImeOptions, InputType, TextInputAction};
use wordcraft_engine::Session;
use wordcraft_ui_egui::{Services, UiState, WordApp};

/// Entry point: android-activity calls this on its own thread, not the Java UI thread.
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    prepare_environment(&app);
    let logger = logging::install();
    logging::install_panic_hook();
    if let Some(logger) = logger {
        match log_dir() {
            Some(dir) => match logger.attach_dir(&dir) {
                Ok(path) => log::info!("WordCraft {} for Android, log file {}", env!("CARGO_PKG_VERSION"), path.display()),
                Err(e) => log::warn!("no log file: {e}"),
            },
            None => logger.no_file(),
        }
    }
    // Mouse wheel axes are off by default in GameActivity.
    app.enable_motion_axis(Axis::Hscroll);
    app.enable_motion_axis(Axis::Vscroll);
    load_user_fonts();

    let options = eframe::NativeOptions { android_app: Some(app.clone()), wgpu_options: wgpu_options(), ..Default::default() };
    let host = app.clone();
    let result = eframe::run_native(
        "WordCraft",
        options,
        Box::new(move |cc| {
            match &cc.wgpu_render_state {
                Some(rs) => log::info!("graphics: {:?}", rs.adapter.get_info()),
                None => log::info!("graphics: no wgpu device"),
            }
            bridge::set_context(cc.egui_ctx.clone());
            let mut word = WordApp::new(Session::new(wordcraft_doc::Document::new()), services(&host));
            load_prefs(&mut word);
            recover_backup(&mut word);
            // Multi-line text field: Gboard shows Enter (new line), not the ✓ Done button. No-suggestions keeps
            // Gboard from composing words, which made it re-send each word (many Backspaces) per keystroke.
            host.set_ime_editor_info(
                InputType::from_bits_retain(0x1 | 0x4000 | 0x2_0000 | 0x8_0000),
                TextInputAction::None,
                ImeOptions::from_bits_retain(0x4000_0000 | 0x1000_0000),
            );
            let h = host.clone();
            word.touch.haptic = Some(Box::new(move || bridge::haptic(&h)));
            Ok(Box::new(AndroidHost {
                app: word,
                android: host,
                last_copied: String::new(),
                fullscreen_applied: false,
                folder_poll: 0,
                last_backup: Instant::now(),
                last_prefs_check: Instant::now(),
                saved_prefs: Vec::new(),
            }))
        }),
    );
    if let Err(e) = result {
        log::error!("WordCraft could not start: {e}");
    }
}

/// `$HOME`, the temp folder and the settings folder must point inside the app's own storage;
/// the desktop defaults (`~/.config`, `/data/local/tmp`) don't exist or aren't writable here.
/// `wordcraft_control_key::settings_dir()` then resolves to `<files>/.config/wordcraft`.
fn prepare_environment(app: &AndroidApp) {
    let Some(data) = app.internal_data_path() else { return };
    let tmp = data.join("tmp");
    let _ = std::fs::create_dir_all(&tmp);
    // SAFETY: first thing `android_main` does, before this app spawns any thread of its own.
    unsafe {
        std::env::set_var("HOME", &data);
        std::env::set_var("TMPDIR", &tmp);
        std::env::remove_var("XDG_CONFIG_HOME");
    }
}

/// Ask the adapter for its own texture limit (up to 16384 px) so a modest phone GPU still opens
/// the app. Vulkan first, OpenGL ES as the fallback.
fn wgpu_options() -> eframe::WgpuConfiguration {
    use eframe::egui_wgpu::WgpuSetup;
    let mut config = eframe::WgpuConfiguration::default();
    if let WgpuSetup::CreateNew(setup) = &mut config.wgpu_setup {
        setup.instance_descriptor.backends = eframe::wgpu::Backends::VULKAN | eframe::wgpu::Backends::GL;
        setup.device_descriptor = Arc::new(|adapter| {
            let limits = adapter.limits();
            eframe::wgpu::DeviceDescriptor {
                label: Some("WordCraft device"),
                required_limits: eframe::wgpu::Limits { max_texture_dimension_2d: limits.max_texture_dimension_2d.min(16384), ..limits },
                ..Default::default()
            }
        });
    }
    config
}

fn services(host: &AndroidApp) -> Services {
    let h = host.clone();
    let clip = host.clone();
    Services {
        file_dialog: Some(Box::new(move |request| bridge::file_dialog(&h, request))),
        clipboard_text: Some(Box::new(move || bridge::clipboard_get(&clip))),
        ..Default::default()
    }
}

// ---- crash recovery ----------------------------------------------------------------------
// While there are unsaved changes, a copy of the document is kept in the app's own folder every
// few seconds. A normal exit deletes it; if the app crashes or is killed it is still there at the
// next start and reopens with the changes marked unsaved.

const BACKUP_EVERY: Duration = Duration::from_secs(15);
const PREFS_EVERY: Duration = Duration::from_secs(3);

fn backup_file() -> Option<std::path::PathBuf> {
    Some(prefs_path()?.parent()?.join("recovery").join("backup.docx"))
}

fn clear_backup() {
    if let Some(f) = backup_file() {
        let _ = std::fs::remove_file(f);
    }
}

fn recover_backup(app: &mut WordApp) {
    let Some(file) = backup_file().filter(|f| f.exists()) else { return };
    match app.run("file.open", serde_json::json!({"path": file.to_string_lossy()})) {
        Ok(_) => {
            // The copy is not the user's file: Save asks where it goes (the documents folder).
            app.session.path = None;
            app.session.dirty = true;
            app.status("Recovered your unsaved changes from the last session. Save to keep them.");
            log::info!("recovered {}", file.display());
        }
        Err(e) => log::warn!("could not recover {}: {e}", file.display()),
    }
}

impl AndroidHost {
    /// Settings are written soon after they change, not only on a clean exit, so a crash or a
    /// killed app keeps them (the Help toggles, interface size, theme, recent files…).
    fn save_prefs_if_changed(&mut self) {
        if self.last_prefs_check.elapsed() < PREFS_EVERY {
            return;
        }
        self.last_prefs_check = Instant::now();
        let Some(path) = prefs_path() else { return };
        let Ok(bytes) = serde_json::to_vec_pretty(&self.app.prefs()) else { return };
        if bytes == self.saved_prefs {
            return;
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Write beside the file and rename, so a crash mid-write never leaves half a settings file.
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, &bytes).is_ok() && std::fs::rename(&tmp, &path).is_ok() {
            self.saved_prefs = bytes;
        }
    }

    fn backup_if_due(&mut self) {
        if self.last_backup.elapsed() < BACKUP_EVERY {
            return;
        }
        self.last_backup = Instant::now();
        let session = &self.app.session;
        // Password-protected documents are not copied out unencrypted.
        if !session.dirty || session.password.is_some() {
            return;
        }
        let Some(file) = backup_file() else { return };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = wordcraft_engine::io::save_path_with(&file, &session.doc, None, &wordcraft_engine::io::ExportOptions::default()) {
            log::warn!("crash-recovery copy failed: {e}");
        }
    }
}

// ---- preferences (the desktop app's `main.rs` helpers; they are private there) ---------------

fn prefs_path() -> Option<std::path::PathBuf> {
    wordcraft_control_key::settings_dir().map(|b| b.join("ui.json"))
}

fn log_dir() -> Option<std::path::PathBuf> {
    Some(prefs_path()?.parent()?.join("logs"))
}

fn load_prefs(app: &mut WordApp) {
    if let Some(p) = prefs_path()
        && let Ok(bytes) = std::fs::read(&p)
        && let Some(ui) = UiState::from_json(&bytes)
    {
        app.apply_prefs(ui);
    }
}

fn save_prefs(app: &WordApp) {
    if let Some(p) = prefs_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(&app.prefs()) {
            let _ = std::fs::write(&p, bytes);
        }
    }
}

fn load_user_fonts() {
    let Some(dir) = wordcraft_control_key::settings_dir().map(|d| d.join("fonts")) else { return };
    let (loaded, skipped) = wordcraft_fonts::FontDb::global().load_user_fonts(&dir);
    if !loaded.is_empty() || !skipped.is_empty() {
        log::info!("user fonts: {} loaded, {} skipped from {}", loaded.len(), skipped.len(), dir.display());
    }
}

// ---- the eframe app --------------------------------------------------------------------------

struct AndroidHost {
    app: WordApp,
    android: AndroidApp,
    /// The last text mirrored to the system clipboard (so an unchanged copy isn't sent twice).
    last_copied: String,
    /// Whether Help ▸ Full Screen is what Android was last told.
    fullscreen_applied: bool,
    /// Frames until the documents folder state is read from Android again.
    folder_poll: u32,
    /// When the crash-recovery copy was last written.
    last_backup: Instant,
    /// When preferences were last checked for changes, and the text last written.
    last_prefs_check: Instant,
    saved_prefs: Vec<u8>,
}

impl eframe::App for AndroidHost {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.app.touch.process(ctx, raw, self.app.canvas.canvas_rect, &self.app.canvas.page_rects);
        self.app.raw_input_hook(raw);
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for path in bridge::take_opened() {
            if let Err(e) = self.app.run("file.open", serde_json::json!({"path": path})) {
                log::warn!("{path}: {e}");
            }
        }
        self.app.logic(ctx);
        self.backup_if_due();
        self.save_prefs_if_changed();
        // The window's back/close request: with unsaved changes, ask first.
        let closing = ctx.input(|i| i.viewport().close_requested());
        if closing && !self.app.close_requested() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        } else if self.app.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.ui(ui);
        let ctx = ui.ctx().clone();
        self.app.touch_frame(&ctx);
        // Help ▸ Full Screen and Help ▸ Documents Folder.
        if self.app.touch.fullscreen != self.fullscreen_applied {
            self.fullscreen_applied = self.app.touch.fullscreen;
            bridge::set_fullscreen(&self.android, self.fullscreen_applied);
        }
        if let Some(on) = self.app.touch.docs_folder_request.take() {
            bridge::set_documents_folder(&self.android, on);
            self.folder_poll = 0;
        }
        // The folder picker answers later, so ask Android now and then what the state is.
        if self.folder_poll == 0 {
            if let Some(set) = bridge::documents_folder_set(&self.android) {
                self.app.touch.docs_folder = set;
            }
            self.folder_poll = 120;
        }
        self.folder_poll -= 1;
        // Help ▸ Block Keyboard: focusing the page raises the keyboard by itself; close it again.
        if self.app.touch.block_keyboard && bridge::soft_keyboard_visible(&self.android) == Some(true) {
            bridge::hide_soft_keyboard(&self.android);
        }
        // Copy / Cut → the system clipboard.
        let copied = ctx.output(|o| {
            o.commands.iter().rev().find_map(|c| match c {
                egui::OutputCommand::CopyText(t) => Some(t.clone()),
                _ => None,
            })
        });
        if let Some(text) = copied.filter(|t| *t != self.last_copied) {
            bridge::clipboard_set(&self.android, &text);
            self.last_copied = text;
        }
    }

    fn on_exit(&mut self) {
        // A normal exit (after any "Save changes?" answer) leaves nothing to recover.
        clear_backup();
        save_prefs(&self.app);
    }
}
