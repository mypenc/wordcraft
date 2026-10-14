//! Keyboard: typing, editing keys and command shortcuts (the user's custom keys from Customize
//! Keyboard first, then the registry's `shortcut` fields).

use egui::{Key, Modifiers};
use serde_json::{Value, json};

use crate::WordApp;

/// The registry's name for a key.
pub fn key_name(k: Key) -> Option<&'static str> {
    Some(match k {
        Key::ArrowLeft => "Left",
        Key::ArrowRight => "Right",
        Key::ArrowUp => "Up",
        Key::ArrowDown => "Down",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "PageUp",
        Key::PageDown => "PageDown",
        Key::Enter => "Enter",
        Key::Tab => "Tab",
        Key::Backspace => "Backspace",
        Key::Delete => "Delete",
        Key::Escape => "Escape",
        Key::Space => "Space",
        Key::Insert => "Insert",
        Key::Equals | Key::Plus => "=",
        Key::Minus => "-",
        Key::Period => ".",
        Key::Comma => ",",
        Key::OpenBracket => "[",
        Key::CloseBracket => "]",
        Key::Num0 => "0",
        Key::Num1 => "1",
        Key::Num2 => "2",
        Key::Num3 => "3",
        Key::Num4 => "4",
        Key::Num5 => "5",
        Key::Num6 => "6",
        Key::Num7 => "7",
        Key::Num8 => "8",
        Key::Num9 => "9",
        Key::F1 => "F1",
        Key::F2 => "F2",
        Key::F3 => "F3",
        Key::F4 => "F4",
        Key::F5 => "F5",
        Key::F6 => "F6",
        Key::F7 => "F7",
        Key::F8 => "F8",
        Key::F9 => "F9",
        Key::F10 => "F10",
        Key::F11 => "F11",
        Key::F12 => "F12",
        Key::A => "A",
        Key::B => "B",
        Key::C => "C",
        Key::D => "D",
        Key::E => "E",
        Key::F => "F",
        Key::G => "G",
        Key::H => "H",
        Key::I => "I",
        Key::J => "J",
        Key::K => "K",
        Key::L => "L",
        Key::M => "M",
        Key::N => "N",
        Key::O => "O",
        Key::P => "P",
        Key::Q => "Q",
        Key::R => "R",
        Key::S => "S",
        Key::T => "T",
        Key::U => "U",
        Key::V => "V",
        Key::W => "W",
        Key::X => "X",
        Key::Y => "Y",
        Key::Z => "Z",
        _ => return None,
    })
}

/// A key with its modifiers as the registry writes shortcuts (`Mod+Shift+K`).
pub fn combo(m: Modifiers, key: &str, with_shift: bool) -> String {
    let mut s = String::new();
    if m.command {
        s.push_str("Mod+");
    }
    if m.ctrl && !m.command {
        s.push_str("Ctrl+");
    }
    if m.mac_cmd && m.ctrl {
        s.push_str("Ctrl+");
    }
    if m.alt {
        s.push_str("Alt+");
    }
    if m.shift && with_shift {
        s.push_str("Shift+");
    }
    s.push_str(key);
    s
}

/// Shortcuts for interface commands (`ui.*`), which aren't in the engine's keymap: Alt+Q moves
/// to "Tell me" command search, as in Word for Windows (#819). Not on macOS, where Option+Q types
/// a character.
fn ui_shortcut(key: Key, m: Modifiers) -> Option<(&'static str, Value)> {
    let alt_only = m.alt && !m.command && !m.ctrl && !m.mac_cmd && !m.shift;
    match key {
        Key::Q if alt_only && !cfg!(target_os = "macos") => Some(("ui.dialog", json!({"name": "commands"}))),
        _ => None,
    }
}

/// Find and run the command bound to this key. Returns true if handled.
fn dispatch(app: &mut WordApp, key: Key, m: Modifiers) -> bool {
    if let Some((id, params)) = ui_shortcut(key, m) {
        let _ = app.run(id, params);
        return true;
    }
    let Some(name) = key_name(key) else { return false };
    let reg = app.session.registry.clone();
    if let Some(spec) = app.session.keymap.resolve(&reg, &combo(m, name, true)) {
        let _ = app.run(spec.id, json!({}));
        return true;
    }
    // Shift extends caret movement; Shift+Backspace still erases (#707).
    if m.shift
        && let Some(spec) = app.session.keymap.resolve(&reg, &combo(m, name, false))
        && let Some(params) = shift_variant_params(spec.id)
    {
        let _ = app.run(spec.id, params);
        return true;
    }
    false
}

/// How a command bound to a plain key runs when Shift is also held and nothing is bound to the
/// shifted combo: caret moves extend the selection, Backspace erases as usual (#707).
fn shift_variant_params(id: &str) -> Option<Value> {
    if id.starts_with("caret.") {
        Some(json!({"extend": true}))
    } else if id == "text.backspace" {
        Some(json!({}))
    } else {
        None
    }
}

/// Shift+F10 opens the context menu, as the Menu key does (#821). (The Menu key itself doesn't
/// reach the app: the windowing layer drops it.)
fn is_context_menu_key(key: Key, m: Modifiers) -> bool {
    key == Key::F10 && m.shift && !m.command && !m.alt && !m.ctrl
}

/// F1–F12: shortcuts on these work without the canvas focused.
fn is_function_key(key: Key) -> bool {
    matches!(key, Key::F1 | Key::F2 | Key::F3 | Key::F4 | Key::F5 | Key::F6 | Key::F7 | Key::F8 | Key::F9 | Key::F10 | Key::F11 | Key::F12)
}

/// Keys while editing inside an equation. Returns true when handled.
fn equation_key(app: &mut WordApp, key: Key, m: Modifiers) -> bool {
    if m.command || m.alt || (m.ctrl && !cfg!(target_os = "macos")) {
        return false;
    }
    let (id, params) = match key {
        Key::ArrowLeft => ("equation.move", json!({"dir": "left"})),
        Key::ArrowRight => ("equation.move", json!({"dir": "right"})),
        Key::ArrowUp => ("equation.move", json!({"dir": "up"})),
        Key::ArrowDown => ("equation.move", json!({"dir": "down"})),
        Key::Home => ("equation.move", json!({"dir": "home"})),
        Key::End => ("equation.move", json!({"dir": "end"})),
        Key::Tab => ("equation.move", json!({"dir": if m.shift { "prev" } else { "next" }})),
        Key::Backspace => ("equation.backspace", json!({})),
        Key::Delete => ("equation.delete", json!({})),
        Key::Enter => ("equation.enter", json!({})),
        Key::Escape => ("equation.exit", json!({})),
        _ => return false,
    };
    let _ = app.run(id, params);
    true
}

/// Whether releasing `key` with `modifiers` is the plain Paste shortcut (Mod+V or the Paste
/// key) — not Mod+Shift+V (Paste Formatting) or Mod+Alt+V (Paste Special) — so a clipboard
/// picture may be pasted on it (#45).
fn is_paste_release(key: Key, modifiers: egui::Modifiers) -> bool {
    (modifiers.command || key == Key::Paste) && !modifiers.alt && !modifiers.shift
}

/// Whether releasing `key` with `modifiers` is Mod+Alt+V (Paste Special).
fn is_paste_special_release(key: Key, modifiers: egui::Modifiers) -> bool {
    key == Key::V && modifiers.command && modifiers.alt && !modifiers.shift
}

/// Events for the focused canvas: text, editing keys, clipboard, IME. While keytips are showing
/// every key belongs to them (letters pick badges, Escape cancels), so nothing here runs.
pub fn canvas_events(app: &mut WordApp, ctx: &egui::Context) {
    if app.ui.keytips != crate::keytips::Phase::Off {
        return;
    }
    let events = ctx.input(|i| i.events.clone());
    if app.session.view.read_mode {
        // Read Mode is for navigation and selection, not typing into the document. The ribbon's
        // Edit Document button or Escape returns to Print Layout before editing.
        for e in &events {
            match e {
                egui::Event::Key { key: Key::Escape, pressed: true, .. } => {
                    let _ = app.run("view.printLayout", json!({}));
                    break;
                }
                egui::Event::Key { key: Key::PageDown | Key::ArrowRight, pressed: true, modifiers, .. } if modifiers.is_none() => {
                    crate::canvas::turn_read_page(app, 1.0)
                }
                egui::Event::Key { key: Key::PageUp | Key::ArrowLeft, pressed: true, modifiers, .. } if modifiers.is_none() => {
                    crate::canvas::turn_read_page(app, -1.0)
                }
                egui::Event::Copy => {
                    let selected = app.session.selected_text();
                    if !selected.is_empty() {
                        ctx.copy_text(selected);
                    }
                }
                _ => {}
            }
        }
        return;
    }
    for e in events {
        // An Alt shortcut (Alt+Shift+D, Alt+F7…) may also send its key's text; it isn't typed.
        match &e {
            egui::Event::Key { pressed: true, .. } => app.canvas.swallow_text = false,
            egui::Event::Text(_) if std::mem::take(&mut app.canvas.swallow_text) => continue,
            _ => {}
        }
        if let egui::Event::Paste(t) = &e {
            app.canvas.pasted = true;
            // Copied picture files arrive as their paths: insert the pictures (#45).
            if app.session.math.is_none() && !ctx.input(|i| i.modifiers.alt) && crate::paste_picture::paste_named_files(app, t) {
                continue;
            }
        }
        // Desktop: Mod+V with no text on the clipboard sends no paste event, only the key's
        // release; paste the clipboard's picture then (#45). A paste event before the release
        // means it was text. (The web reads pasted pictures itself.)
        if let egui::Event::Key { key: key @ (Key::V | Key::Paste), pressed: false, modifiers, .. } = &e {
            let paste_keys = is_paste_release(*key, *modifiers);
            let pasted = std::mem::take(&mut app.canvas.pasted);
            if !pasted && paste_keys && app.session.math.is_none() && app.services.clipboard_picture.is_some() {
                let _ = app.run("edit.paste", json!({}));
            }
            // Mod+Alt+V with no text on the clipboard: Paste Special still opens, listing what
            // is there (a picture) or saying what it holds (#687, #833).
            if !pasted && is_paste_special_release(*key, *modifiers) && app.session.math.is_none() && app.services.clipboard_picture.is_some() {
                let _ = app.run("edit.pasteSpecial", json!({}));
            }
            continue;
        }
        // Editing an equation: text and editing keys go into it.
        if app.session.math.is_some() {
            match &e {
                egui::Event::Text(t) => {
                    let m = ctx.input(|i| i.modifiers);
                    if !(m.command || t.is_empty() || (m.ctrl && !cfg!(target_os = "macos"))) && t.chars().all(|c| !c.is_control()) {
                        let _ = app.run("equation.type", json!({"text": t}));
                    }
                    continue;
                }
                egui::Event::Ime(egui::ImeEvent::Commit(text)) => {
                    app.canvas.ime_preedit.clear();
                    if !text.is_empty() {
                        let _ = app.run("equation.type", json!({"text": text}));
                    }
                    continue;
                }
                egui::Event::Paste(t) => {
                    let _ = app.run("equation.type", json!({"text": t}));
                    continue;
                }
                egui::Event::Key { key, pressed: true, modifiers, .. } if equation_key(app, *key, *modifiers) => continue,
                _ => {}
            }
        }
        match e {
            egui::Event::Text(t) => {
                let m = ctx.input(|i| i.modifiers);
                if m.command || (m.ctrl && !cfg!(target_os = "macos")) {
                    continue;
                }
                if t.chars().all(|c| !c.is_control()) && !t.is_empty() {
                    // Typing with a text box selected types into it.
                    crate::objects::enter_text_box(app);
                    let _ = app.run("text.insert", json!({"text": t}));
                }
            }
            egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) => app.canvas.ime_preedit = text,
            egui::Event::Ime(egui::ImeEvent::Commit(text)) => {
                app.canvas.ime_preedit.clear();
                if !text.is_empty() {
                    let _ = app.run("text.insert", json!({"text": text}));
                }
            }
            egui::Event::Paste(t) => {
                // Mod+Alt+V arrives as a paste too (the shell reads the clipboard): Paste Special.
                let m = ctx.input(|i| i.modifiers);
                let id = match (m.alt, m.shift) {
                    (true, true) => "edit.pasteText",
                    (true, false) => "edit.pasteSpecial",
                    _ => "edit.paste",
                };
                let _ = app.run(id, json!({"text": t}));
            }
            egui::Event::Copy | egui::Event::Cut => {
                let id = if matches!(e, egui::Event::Cut) { "edit.cut" } else { "edit.copy" };
                if let Ok(r) = app.run(id, json!({})) {
                    crate::system_clipboard::put_result(app, ctx, &r);
                }
            }
            egui::Event::Key { key, pressed: true, modifiers, .. } => {
                if key == Key::Escape && app.session.painter.is_some() {
                    app.session.painter = None;
                    continue;
                }
                // The lasso selection and Ink Replay take Delete and Escape first.
                if crate::ink::key(app, key) {
                    continue;
                }
                // Escape puts the pen or eraser down (back to selecting).
                if key == Key::Escape && app.session.view.draw.mode != wordcraft_engine::cmd::draw::DrawMode::Select {
                    app.canvas.ink = None;
                    let _ = app.run("draw.select", json!({}));
                    continue;
                }
                if key == Key::Escape && crate::table_pen::stop(app) {
                    continue;
                }
                // File › Options › Advanced: the Insert key switches overtype only when asked to.
                if key == Key::Insert && modifiers.is_none() {
                    if app.session.prefs.insert_key_overtype {
                        let _ = app.run("text.overtype", json!({}));
                    }
                    continue;
                }
                if crate::objects::key(app, key, modifiers) {
                    continue;
                }
                // Shift+F10, Windows' keyboard way to the context menu (#821).
                if is_context_menu_key(key, modifiers) {
                    app.canvas.context_menu_from_keys = true;
                    continue;
                }
                // Escape leaves a text box to just after it in the body.
                if key == Key::Escape
                    && crate::canvas::in_text_box(app)
                    && app.session.sel.is_collapsed()
                    && let wordcraft_doc::StoryRef::Part(id) = app.session.sel.focus.story
                    && let Some(pos) = app.session.doc.text_box_anchor(id)
                {
                    let _ = app.run("caret.set", json!({"pos": pos}));
                    continue;
                }
                if key == Key::Escape && matches!(app.session.sel.focus.story, wordcraft_doc::StoryRef::Part(_)) && app.session.sel.is_collapsed() {
                    let _ = app.run("insert.closeHeader", json!({}));
                    continue;
                }
                app.canvas.swallow_text = dispatch(app, key, modifiers) && modifiers.alt;
            }
            _ => {}
        }
    }
}

/// Whether a key runs its shortcut with the canvas unfocused (after a ribbon click, say): Mod and
/// Alt shortcuts (Alt+Q reaches Search from anywhere, as in Word, #819) and the function keys.
fn is_global_shortcut(key: Key, m: Modifiers) -> bool {
    m.command || m.alt || is_function_key(key)
}

/// Command shortcuts when the canvas isn't focused but no text field is either.
pub fn global_shortcuts(app: &mut WordApp, ctx: &egui::Context) {
    // Customize Keyboard records the keys pressed; they never run commands meanwhile.
    if app.canvas.focused || ctx.egui_wants_keyboard_input() || matches!(app.dialog, Some(crate::dialogs::Dialog::CustomizeKeyboard { .. })) {
        return;
    }
    let events = ctx.input(|i| i.events.clone());
    for e in events {
        if let egui::Event::Key { key, pressed: true, modifiers, .. } = e
            && is_global_shortcut(key, modifiers)
        {
            dispatch(app, key, modifiers);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_backspace_still_erases() {
        assert_eq!(shift_variant_params("text.backspace"), Some(json!({})));
        assert_eq!(shift_variant_params("caret.left"), Some(json!({"extend": true})));
        assert_eq!(shift_variant_params("text.delete"), None);
    }

    #[test]
    fn alt_q_opens_command_search() {
        let m = |alt, shift, command| egui::Modifiers { alt, shift, command, ..Default::default() };
        let tell_me = Some(("ui.dialog", json!({"name": "commands"})));
        if cfg!(target_os = "macos") {
            assert_eq!(ui_shortcut(Key::Q, m(true, false, false)), None, "Option+Q types a character");
        } else {
            assert_eq!(ui_shortcut(Key::Q, m(true, false, false)), tell_me);
        }
        assert_eq!(ui_shortcut(Key::Q, m(true, true, false)), None, "Alt+Shift+Q");
        assert_eq!(ui_shortcut(Key::Q, m(true, false, true)), None, "Mod+Alt+Q");
        assert_eq!(ui_shortcut(Key::Q, m(false, false, false)), None, "a plain Q");
        assert_eq!(ui_shortcut(Key::W, m(true, false, false)), None);
    }

    #[test]
    fn only_plain_paste_releases_paste_a_clipboard_picture() {
        let m = |command, shift, alt| egui::Modifiers { command, shift, alt, ..Default::default() };
        assert!(is_paste_release(Key::V, m(true, false, false)), "Mod+V");
        assert!(is_paste_release(Key::Paste, m(false, false, false)), "the Paste key");
        assert!(!is_paste_release(Key::V, m(true, true, false)), "Mod+Shift+V is Paste Formatting");
        assert!(!is_paste_release(Key::V, m(true, false, true)), "Mod+Alt+V is Paste Special");
        assert!(!is_paste_release(Key::V, m(false, false, false)), "a plain V");
        assert!(is_paste_special_release(Key::V, m(true, false, true)), "Mod+Alt+V opens Paste Special");
        assert!(!is_paste_special_release(Key::V, m(true, true, true)) && !is_paste_special_release(Key::V, m(true, false, false)));
    }

    #[test]
    fn shift_f10_is_the_context_menu_key() {
        let m = |shift, ctrl, alt| egui::Modifiers { shift, ctrl, command: ctrl, alt, ..Default::default() };
        assert!(is_context_menu_key(Key::F10, m(true, false, false)));
        assert!(!is_context_menu_key(Key::F10, m(false, false, false)), "F10 alone shows keytips");
        assert!(!is_context_menu_key(Key::F10, m(true, true, false)), "Ctrl+Shift+F10");
        assert!(!is_context_menu_key(Key::F9, m(true, false, false)), "Shift+F9 is a field key");
    }

    #[test]
    fn alt_shortcuts_work_without_the_canvas_focused() {
        let m = |command, alt, shift| egui::Modifiers { command, alt, shift, ..Default::default() };
        assert!(is_global_shortcut(Key::Q, m(false, true, false)), "Alt+Q (#819)");
        assert!(is_global_shortcut(Key::D, m(false, true, true)), "Alt+Shift+D");
        assert!(is_global_shortcut(Key::B, m(true, false, false)), "Mod+B");
        assert!(is_global_shortcut(Key::F7, m(false, false, false)), "F7");
        assert!(!is_global_shortcut(Key::Q, m(false, false, false)), "a plain letter is typing");
        assert!(!is_global_shortcut(Key::Q, m(false, false, true)), "Shift+letter is typing");
    }
}
