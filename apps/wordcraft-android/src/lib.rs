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
//!   click, two fingers scroll, pinch zooms…) plus an on-screen Shift/Ctrl/Alt/Space bar (Help ▸
//!   Touch Bar hides it); a mouse and keyboard reach egui through the patched winit
//!   (`android/patches/`), whose soft-keyboard text events also feed the editor.
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

use android_activity::AndroidApp;
use android_activity::input::Axis;
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
            let h = host.clone();
            word.touch.haptic = Some(Box::new(move || bridge::haptic(&h)));
            Ok(Box::new(AndroidHost { app: word, android: host, last_copied: String::new() }))
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
}

impl eframe::App for AndroidHost {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.app.touch.process(ctx, raw);
        self.app.raw_input_hook(raw);
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for path in bridge::take_opened() {
            if let Err(e) = self.app.run("file.open", serde_json::json!({"path": path})) {
                log::warn!("{path}: {e}");
            }
        }
        self.app.logic(ctx);
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
        // The on-screen bar's keyboard button. (Focusing the page raises the keyboard by itself:
        // the editor publishes an IME rect, egui-winit calls `set_ime_allowed`, and winit
        // forwards that to `show_soft_input`.)
        if std::mem::take(&mut self.app.touch.keyboard_request) {
            self.android.show_soft_input(false);
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
        save_prefs(&self.app);
    }
}
