package ai.storyteller.wordcraft

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.DocumentsContract
import android.view.WindowManager
import android.webkit.MimeTypeMap
import androidx.core.graphics.Insets
import androidx.core.view.WindowInsetsControllerCompat
import android.provider.OpenableColumns
import android.util.Log
import android.view.HapticFeedbackConstants
import android.view.View
import android.view.inputmethod.InputMethodManager
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
    @Volatile private var softKeyboardVisible = false
    @Volatile private var fullscreen = false
    private val prefs by lazy { getSharedPreferences("wordcraft", MODE_PRIVATE) }

    private class Request(val id: Long, val arg: String)

    // One system dialog at a time; the rest wait their turn.
    private val queue = ArrayDeque<Pair<Boolean, Request>>() // (isSave, request)
    private var active: Pair<Boolean, Request>? = null

    /** Open with write access kept across restarts, so Save can write back to the picked file. */
    private class OpenWritable : ActivityResultContracts.OpenDocument() {
        override fun createIntent(context: Context, input: Array<String>): Intent =
            super.createIntent(context, input).addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
    }

    private val openDocument = registerForActivityResult(OpenWritable()) { uri ->
        if (uri != null) runCatching {
            contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        } // read-only providers refuse the write grant; opening still works
        finish(uri) { u, req -> copyIn(u, req) }
    }

    /** The documents folder: asked for once, kept (with write access) until the user clears it. */
    private val openTree = registerForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        if (uri != null) {
            runCatching {
                contentResolver.takePersistableUriPermission(
                    uri, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                )
                prefs.edit().putString(PREF_WORKSPACE, uri.toString()).apply()
            }.onFailure { Log.w("WordCraft", "could not keep access to $uri", it) }
        }
        val pending = pendingSave
        pendingSave = null
        if (pending != null) launchSave(pending) // the Save that asked for the folder carries on
        else { active = null; next() }
    }
    private var pendingSave: Request? = null
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
            // Fullscreen: the app draws under the (hidden) status bar and the notch.
            val bars = if (fullscreen) Insets.NONE
                else insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
            softKeyboardVisible = insets.isVisible(WindowInsetsCompat.Type.ime())
            WindowInsetsCompat.CONSUMED
        }
        ViewCompat.requestApplyInsets(content)
        exports = Exports(applicationContext)
        DebugLog.install(this)
        handleViewIntent(intent)
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus && fullscreen) applyFullscreen(true) // the system shows the bars again after a dialog
    }

    private fun applyFullscreen(on: Boolean) {
        fullscreen = on
        if (Build.VERSION.SDK_INT >= 28) {
            val lp = window.attributes
            lp.layoutInDisplayCutoutMode = if (on) WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
                else WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_DEFAULT
            window.attributes = lp
        }
        val controller = WindowCompat.getInsetsController(window, window.decorView)
        if (on) {
            controller.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            controller.hide(WindowInsetsCompat.Type.systemBars())
        } else {
            controller.show(WindowInsetsCompat.Type.systemBars())
        }
        ViewCompat.requestApplyInsets(findViewById(android.R.id.content))
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

    /** Help ▸ Full Screen: hide the status / navigation bars and draw into the notch area. */
    @Suppress("unused")
    fun setFullscreen(on: Boolean) = runOnUiThread { applyFullscreen(on) }

    /** Whether a documents folder is chosen and still accessible. */
    @Suppress("unused")
    fun documentsFolderSet(): Boolean = workspaceUri() != null

    /** Help ▸ Documents Folder: `true` asks the user to pick the folder, `false` forgets it. */
    @Suppress("unused")
    fun setDocumentsFolder(on: Boolean) = runOnUiThread {
        if (on) {
            enqueueFolder()
        } else {
            workspaceUri()?.let { runCatching { contentResolver.releasePersistableUriPermission(it, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION) } }
            prefs.edit().remove(PREF_WORKSPACE).apply()
        }
    }

    @Suppress("unused")
    fun showLog() = runOnUiThread { DebugLog.show(this) }

    @Suppress("unused")
    fun haptic() = runOnUiThread { window.decorView.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS) }

    @Suppress("unused")
    fun softKeyboardVisible(): Boolean = softKeyboardVisible

    @Suppress("unused")
    fun hideSoftKeyboard() = runOnUiThread {
        val token = window.decorView.windowToken ?: return@runOnUiThread
        val inputMethod = getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager
        if (inputMethod.hideSoftInputFromWindow(token, 0)) {
            softKeyboardVisible = false
        } else {
            Log.w("WordCraft", "Android did not accept the request to hide the soft keyboard")
        }
    }

    /** The bar's KB button: open the keyboard even where the normal request is ignored (some Xiaomi pads). */
    @Suppress("unused")
    fun forceShowSoftKeyboard() = runOnUiThread {
        // The keyboard attaches to the focused view (the game surface), not always the decor view.
        val view = currentFocus ?: window.decorView
        view.requestFocus()
        val inputMethod = getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager
        inputMethod.restartInput(view)
        @Suppress("DEPRECATION")
        var shown = inputMethod.showSoftInput(view, InputMethodManager.SHOW_FORCED)
        if (android.os.Build.VERSION.SDK_INT >= 30) {
            window.insetsController?.show(android.view.WindowInsets.Type.ime())
            shown = true
        }
        if (shown) softKeyboardVisible = true else Log.w("WordCraft", "Android did not accept the request to show the soft keyboard")
    }

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

    private fun enqueueFolder() {
        if (active != null) return // a dialog is open; ask again once it is done
        active = false to Request(-1, "folder")
        openTree.launch(null)
    }

    /** The chosen documents folder, if its access is still granted. */
    private fun workspaceUri(): Uri? {
        val uri = prefs.getString(PREF_WORKSPACE, null)?.let(Uri::parse) ?: return null
        val held = contentResolver.persistedUriPermissions.any { it.uri == uri && it.isWritePermission }
        return if (held) uri else null
    }

    /** Save: into the documents folder when there is one; the first time, ask for it. */
    private fun launchSave(request: Request) {
        val tree = workspaceUri()
        when {
            tree != null -> {
                val created = runCatching {
                    val parent = DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))
                    val ext = request.arg.substringAfterLast('.', "").lowercase()
                    val mime = MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "application/octet-stream"
                    DocumentsContract.createDocument(contentResolver, parent, mime, request.arg)
                }.getOrNull()
                if (created != null) finish(created) { u, req -> stage(u, req.id) }
                else createDocument.launch(request.arg) // the folder went away: let the user pick
            }
            !prefs.getBoolean(PREF_ASKED, false) -> {
                prefs.edit().putBoolean(PREF_ASKED, true).apply()
                pendingSave = request
                openTree.launch(null)
            }
            else -> createDocument.launch(request.arg)
        }
    }

    private fun next() {
        if (active != null) return
        val item = queue.removeFirstOrNull() ?: return
        active = item
        val (isSave, request) = item
        if (isSave) {
            launchSave(request)
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
    private fun copyIn(uri: Uri, request: Request) = io.execute {
        val id = request.id
        val dir = File(filesDir, "imports/${System.currentTimeMillis()}").apply { mkdirs() }
        val path = try {
            val target = File(dir, displayName(uri))
            contentResolver.openInputStream(uri)?.use { input -> target.outputStream().use { input.copyTo(it, 1 shl 20) } }
                ?: error("cannot open $uri")
            // Save writes the app's copy; mirror it back to the picked file when we may write there.
            if (request.arg == "document") mirrorBack(target, uri)
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

    /** Keep [uri] in step with the app's copy [file] after every save, if write access was granted. */
    private fun mirrorBack(file: File, uri: Uri) {
        val writable = checkUriPermission(uri, android.os.Process.myPid(), android.os.Process.myUid(), Intent.FLAG_GRANT_WRITE_URI_PERMISSION) ==
            PackageManager.PERMISSION_GRANTED
        if (writable) exports.watch(file, uri, true, alreadySynced = true)
    }

    private fun handleViewIntent(intent: Intent?) {
        val uri = intent?.takeIf { it.action == Intent.ACTION_VIEW }?.data ?: return
        io.execute {
            val dir = File(filesDir, "imports/${System.currentTimeMillis()}").apply { mkdirs() }
            try {
                val target = File(dir, displayName(uri))
                contentResolver.openInputStream(uri)?.use { input -> target.outputStream().use { input.copyTo(it, 1 shl 20) } }
                    ?: return@execute
                mirrorBack(target, uri)
                NativeBridge.onOpenIntent(target.absolutePath)
            } catch (t: Throwable) {
                Log.w("WordCraft", "could not open $uri", t)
            }
        }
    }

    private companion object {
        const val PREF_WORKSPACE = "documents_folder"
        const val PREF_ASKED = "asked_documents_folder"
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
