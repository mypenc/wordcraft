package ai.storyteller.wordcraft

/**
 * Kotlin → Rust. The symbols are `Java_ai_storyteller_wordcraft_NativeBridge_*` in
 * `apps/wordcraft-android/src/bridge.rs`; keep the package, object and method names in step.
 */
object NativeBridge {
    /** The answer to a file dialog Rust asked for: a path in the app's folder, or null if cancelled. */
    @JvmStatic
    external fun onDialogAnswer(id: Long, path: String?)

    /** A document opened from another app ("Open with"): a path in the app's folder. */
    @JvmStatic
    external fun onOpenIntent(path: String)
}
