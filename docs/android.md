# WordCraft on Android

A native Android app: the same engine and egui interface as the desktop build, compiled to a
`cdylib` and hosted by a `GameActivity` (no web view). Built by `.github/workflows/android.yml`
(signed arm64 APK as a workflow artifact; tag builds attach it to the release) or locally with
`android/scripts/build.sh` (Rust 1.95, `cargo-ndk`, NDK r26+, JDK 17, Gradle).

| Path | What |
|---|---|
| `apps/wordcraft-android/` | `android_main`, file/clipboard/haptic hooks, JNI bridge. Empty on non-Android targets. |
| `crates/ui-egui/src/touch/` | Touch layer (`gesture.rs` is pure and unit-tested). Inert unless a touch arrives or the target is Android. |
| `android/` | Gradle project: `MainActivity`, system pickers, export mirroring, on-device debug log. |
| `android/patches/…android-input.patch` | winit 0.30.13's Android backend drops mouse, modifiers and soft-keyboard text; this delivers them (applied to Android builds only). |

**Input.** Tap = click; hold ~0.45 s = right click (haptic); drag = drag; two fingers = scroll;
pinch = zoom; two-finger tap = undo, three = redo. The bar at the bottom has sticky
Shift/Ctrl/Alt/Space (double-tap to lock), Undo/Redo/Del/Esc/Enter and the soft keyboard; **Help ▸
Touch Bar** hides it. A mouse (hover, buttons, wheel) and a hardware keyboard keep working.

**Files.** Open copies the document into the app folder; Save As / Export asks for the
destination first, the app writes to `…/Android/data/ai.storyteller.wordcraft/files/exports/`, and
`Exports.kt` mirrors every later save to the chosen file.

**Clipboard.** Copy/Cut are mirrored to the system clipboard; the ribbon's Paste reads it.
Ctrl+V on a hardware keyboard pastes egui's in-app clipboard (egui-winit has no system clipboard
on Android), so text copied in another app is pasted with the ribbon button.

**Not on Android:** window geometry, the control/MCP server, native print, drag and drop.

Status: written against the pinned crate sources and the EffectCraft port's fixes; the gesture
tests run on the host, the Android build itself is run by CI. Expect a bring-up pass for what only
a device shows (soft-keyboard composition, ribbon sizes under touch metrics).
