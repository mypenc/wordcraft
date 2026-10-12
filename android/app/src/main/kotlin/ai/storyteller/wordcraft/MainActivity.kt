package ai.storyteller.wordcraft

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import android.util.Log
import android.view.HapticFeedbackConstants
import android.view.View
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import com.google.androidgamesdk.GameActivity
import java.io.File
import java.util.concurrent.Executors

/**
 * Hosts the Rust app. Rust calls the `@Suppress("unused")` methods below over JNI (see
 * `apps/wordcraft-android/src/bridge.rs`); results go back through [NativeBridge].
 *
 * Android gives apps `content://` URIs, but the engine reads and writes plain paths. So:
 *  - **Open / pictures / fonts**: the picked document is copied into `filesDir/imports/…` and its
 *    path is handed to Rust;
 *  - **Save / Export**: the user first names the destination here; Rust then writes to
 *    `getExternalFilesDir()/exports/<name>` and [Exports] mirrors every change to that document.
 */
class MainActivity : GameActivity() {
    private val io = Executors.newSingleThreadExecutor()
    private lateinit var exports: Exports

    private class Request(val id: Long, val arg: String)

    // One system dialog at a time; the rest wait their turn.
    private val queue = ArrayDeque<Pair<Boolean, Request>>() // (isSave, request)
    private var active: Pair<Boolean, Request>? = null

    private val openDocument = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        finish(uri) { u, req -> copyIn(u, req.id) }
    }
    private val createDocument = registerForActivityResult(ActivityResultContracts.CreateDocument("*/*")) { uri ->
        finish(uri) { u, req -> stage(u, req.id) }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        // Android 15 forces edge-to-edge, so the status/navigation bars would cover the top of the
        // app (the ribbon). Nothing on the Rust side reads the insets, so pad the content view by
        // them: the app then draws only inside the safe area.
        WindowCompat.setDecorFitsSystemWindows(window, false)
        super.onCreate(savedInstanceState)
        val content = findViewById<View>(android.R.id.content)
        ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
            WindowInsetsCompat.CONSUMED
        }
        ViewCompat.requestApplyInsets(content)
        exports = Exports(applicationContext)
        DebugLog.install(this)
        handleViewIntent(intent)
    }

    override fun onDestroy() {
        if (isFinishing) DebugLog.markClean(this)
        super.onDestroy()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleViewIntent(intent)
    }

    // ---- called from Rust --------------------------------------------------------------

    /** File ▸ Open and the picture / font / list pickers. [purpose]: `document`, `picture`, … */
    @Suppress("unused")
    fun openDialog(id: Long, purpose: String) = runOnUiThread { enqueue(false, Request(id, purpose)) }

    /** Save As / Export: [name] is the suggested file name. */
    @Suppress("unused")
    fun saveDialog(id: Long, name: String) = runOnUiThread { enqueue(true, Request(id, name)) }

    @Suppress("unused")
    fun showLog() = runOnUiThread { DebugLog.show(this) }

    @Suppress("unused")
    fun haptic() = runOnUiThread { window.decorView.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS) }

    /** The system clipboard's text, for the ribbon's Paste (Ctrl+V uses egui's in-app clipboard). */
    @Suppress("unused")
    fun clipboardText(): String? {
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        return cm.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(this)?.toString()
    }

    /** Copy / Cut: put the text where other apps can paste it. */
    @Suppress("unused")
    fun setClipboardText(text: String) = runOnUiThread {
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        cm.setPrimaryClip(ClipData.newPlainText("WordCraft", text))
    }

    // ---- dialogs -----------------------------------------------------------------------

    private fun enqueue(isSave: Boolean, request: Request) {
        queue.addLast(isSave to request)
        next()
    }

    private fun next() {
        if (active != null) return
        val item = queue.removeFirstOrNull() ?: return
        active = item
        val (isSave, request) = item
        if (isSave) {
            createDocument.launch(request.arg)
        } else {
            openDocument.launch(
                when (request.arg) {
                    "picture" -> arrayOf("image/*")
                    else -> arrayOf("*/*")
                }
            )
        }
    }

    private fun finish(uri: Uri?, work: (Uri, Request) -> Unit) {
        val request = active?.second
        active = null
        if (request == null) return
        if (uri == null) NativeBridge.onDialogAnswer(request.id, null) else work(uri, request)
        next()
    }

    /** Open: copy the document into the app's folder (off the UI thread) and answer with its path. */
    private fun copyIn(uri: Uri, id: Long) = io.execute {
        val dir = File(filesDir, "imports/${System.currentTimeMillis()}").apply { mkdirs() }
        val path = try {
            val target = File(dir, displayName(uri))
            contentResolver.openInputStream(uri)?.use { input -> target.outputStream().use { input.copyTo(it, 1 shl 20) } }
                ?: error("cannot open $uri")
            target.absolutePath
        } catch (t: Throwable) {
            Log.w("WordCraft", "could not read $uri", t)
            null
        }
        NativeBridge.onDialogAnswer(id, path)
    }

    /**
     * Save: the file the user named already exists (empty) at [uri]. Rust writes to a staging file
     * named like it (the extension picks the format) and [Exports] mirrors it to [uri] after every
     * save while the app runs.
     */
    private fun stage(uri: Uri, id: Long) = io.execute {
        val dir = File(getExternalFilesDir(null) ?: filesDir, "exports").apply { mkdirs() }
        val staged = File(dir, displayName(uri))
        runCatching { staged.delete() } // never mirror a stale file before the app writes
        exports.watch(staged, uri, true)
        NativeBridge.onDialogAnswer(id, staged.absolutePath)
    }

    private fun handleViewIntent(intent: Intent?) {
        val uri = intent?.takeIf { it.action == Intent.ACTION_VIEW }?.data ?: return
        io.execute {
            val dir = File(filesDir, "imports/${System.currentTimeMillis()}").apply { mkdirs() }
            try {
                val target = File(dir, displayName(uri))
                contentResolver.openInputStream(uri)?.use { input -> target.outputStream().use { input.copyTo(it, 1 shl 20) } }
                    ?: return@execute
                NativeBridge.onOpenIntent(target.absolutePath)
            } catch (t: Throwable) {
                Log.w("WordCraft", "could not open $uri", t)
            }
        }
    }

    private fun displayName(uri: Uri): String {
        if (uri.scheme == "content") {
            contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
                if (c.moveToFirst()) c.getString(0)?.let { return it.replace('/', '_') }
            }
        }
        return uri.lastPathSegment?.substringAfterLast('/') ?: "document"
    }
}
