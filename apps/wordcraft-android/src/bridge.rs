//! JNI bridge to the Kotlin half (`android/app/src/main/kotlin/…`).
//!
//! Rust → Kotlin: methods on `MainActivity` (`openDialog`, `saveDialog`, `showLog`, `haptic`,
//! clipboard). Kotlin → Rust: the `Java_ai_storyteller_wordcraft_NativeBridge_*` functions below.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock, PoisonError};

use android_activity::AndroidApp;
use jni::objects::{JClass, JObject, JString, JValue};
use jni::sys::jlong;
use jni::{JNIEnv, JavaVM};
use wordcraft_ui_egui::{FileDialogAnswer, FileDialogRequest};

static CTX: OnceLock<egui::Context> = OnceLock::new();
static NEXT_ID: AtomicI64 = AtomicI64::new(1);
/// Dialogs waiting for Kotlin's answer.
static PENDING: Mutex<Option<HashMap<i64, Sender<FileDialogAnswer>>>> = Mutex::new(None);
/// Documents opened from other apps ("Open with"), for the UI thread to open.
static OPENED: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn set_context(ctx: egui::Context) {
    let _ = CTX.set(ctx);
}

fn wake() {
    if let Some(ctx) = CTX.get() {
        ctx.request_repaint();
    }
}

/// Documents other apps asked WordCraft to open since the last call.
pub fn take_opened() -> Vec<String> {
    std::mem::take(&mut *OPENED.lock().unwrap_or_else(PoisonError::into_inner))
}

/// Run `f` on a thread attached to the JVM with the activity as `this`.
fn with_activity<R>(app: &AndroidApp, f: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<R>) -> Option<R> {
    // SAFETY: android-activity guarantees both pointers stay valid for the life of the app.
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.ok()?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let mut env = vm.attach_current_thread().ok()?;
    match f(&mut env, &activity) {
        Ok(v) => Some(v),
        Err(e) => {
            log::warn!("JNI call failed: {e}");
            // A pending Java exception would abort the next call.
            let _ = env.exception_clear();
            None
        }
    }
}

/// `Services::file_dialog`: ask Kotlin to show the system picker; the answer arrives later
/// through `onDialogAnswer` (a path in the app's folder, or `Ok(None)` when cancelled).
pub fn file_dialog(app: &AndroidApp, request: FileDialogRequest) -> Receiver<FileDialogAnswer> {
    let (tx, rx) = channel();
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    PENDING.lock().unwrap_or_else(PoisonError::into_inner).get_or_insert_with(HashMap::new).insert(id, tx.clone());
    let started = with_activity(app, |env, activity| {
        let (method, arg) = match &request {
            FileDialogRequest::Open { purpose } => ("openDialog", purpose.as_str()),
            FileDialogRequest::Save { name } => ("saveDialog", name.as_str()),
        };
        let arg: JString = env.new_string(arg)?;
        env.call_method(activity, method, "(JLjava/lang/String;)V", &[JValue::Long(id), JValue::Object(&arg)]).map(|_| ())
    });
    if started.is_none() {
        PENDING.lock().unwrap_or_else(PoisonError::into_inner).get_or_insert_with(HashMap::new).remove(&id);
        // An `Err` makes the app show its built-in file browser instead.
        let _ = tx.send(Err("the system file picker could not be started".into()));
    }
    rx
}

/// Help ▸ Show Debug Log: a dialog with this app's log (copy / share).
#[allow(dead_code)]
pub fn show_log(app: &AndroidApp) {
    with_activity(app, |env, activity| env.call_method(activity, "showLog", "()V", &[]).map(|_| ()));
}

/// Help ▸ Full Screen: hide the status and navigation bars and draw into the notch area.
pub fn set_fullscreen(app: &AndroidApp, on: bool) {
    with_activity(app, |env, activity| env.call_method(activity, "setFullscreen", "(Z)V", &[JValue::Bool(u8::from(on))]).map(|_| ()));
}

/// Whether a documents folder is chosen and still accessible.
pub fn documents_folder_set(app: &AndroidApp) -> Option<bool> {
    with_activity(app, |env, activity| env.call_method(activity, "documentsFolderSet", "()Z", &[])?.z())
}

/// Help ▸ Documents Folder: `true` opens the folder picker, `false` forgets the folder.
pub fn set_documents_folder(app: &AndroidApp, on: bool) {
    with_activity(app, |env, activity| env.call_method(activity, "setDocumentsFolder", "(Z)V", &[JValue::Bool(u8::from(on))]).map(|_| ()));
}

/// A short vibration (the long press that opens a context menu).
pub fn haptic(app: &AndroidApp) {
    with_activity(app, |env, activity| env.call_method(activity, "haptic", "()V", &[]).map(|_| ()));
}

/// Whether Android currently reports the on-screen keyboard as visible.
pub fn soft_keyboard_visible(app: &AndroidApp) -> Option<bool> {
    with_activity(app, |env, activity| env.call_method(activity, "softKeyboardVisible", "()Z", &[])?.z())
}

/// Hide the on-screen keyboard without dropping focus from the document.
pub fn hide_soft_keyboard(app: &AndroidApp) {
    with_activity(app, |env, activity| env.call_method(activity, "hideSoftKeyboard", "()V", &[]).map(|_| ()));
}

/// Open the on-screen keyboard by force (the bar's KB button), even where the usual request is ignored.
pub fn force_show_soft_keyboard(app: &AndroidApp) {
    with_activity(app, |env, activity| env.call_method(activity, "forceShowSoftKeyboard", "()V", &[]).map(|_| ()));
}

/// Remove the startup loading bar once WordCraft's first frame is on screen.
pub fn hide_loading(app: &AndroidApp) {
    with_activity(app, |env, activity| env.call_method(activity, "hideLoading", "()V", &[]).map(|_| ()));
}

/// The system clipboard's text.
pub fn clipboard_get(app: &AndroidApp) -> Option<String> {
    with_activity(app, |env, activity| {
        let value = env.call_method(activity, "clipboardText", "()Ljava/lang/String;", &[])?.l()?;
        if value.as_raw().is_null() {
            return Ok(None);
        }
        let value = JString::from(value);
        let text = env.get_string(&value)?;
        Ok(Some(String::from(text)))
    })
    .flatten()
}

/// Put `text` on the system clipboard.
pub fn clipboard_set(app: &AndroidApp, text: &str) {
    with_activity(app, |env, activity| {
        let text: JString = env.new_string(text)?;
        env.call_method(activity, "setClipboardText", "(Ljava/lang/String;)V", &[JValue::Object(&text)]).map(|_| ())
    });
}

fn read_string(env: &mut JNIEnv, s: &JString) -> Option<String> {
    if s.as_raw().is_null() {
        return None;
    }
    env.get_string(s).ok().map(String::from)
}

/// `NativeBridge.onDialogAnswer(id: Long, path: String?)`, from a Kotlin worker or the UI thread.
#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_storyteller_wordcraft_NativeBridge_onDialogAnswer(mut env: JNIEnv, _class: JClass, id: jlong, path: JString) {
    let answer = Ok(read_string(&mut env, &path));
    let sender = PENDING.lock().unwrap_or_else(PoisonError::into_inner).as_mut().and_then(|m| m.remove(&id));
    if let Some(tx) = sender {
        let _ = tx.send(answer);
    }
    wake();
}

/// `NativeBridge.onOpenIntent(path: String)`: a document opened from another app.
#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_storyteller_wordcraft_NativeBridge_onOpenIntent(mut env: JNIEnv, _class: JClass, path: JString) {
    if let Some(p) = read_string(&mut env, &path) {
        OPENED.lock().unwrap_or_else(PoisonError::into_inner).push(p);
        wake();
    }
}
