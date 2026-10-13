package ai.storyteller.wordcraft

import android.content.Context
import android.net.Uri
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.util.Log
import android.widget.Toast
import java.io.File

/**
 * Mirrors files the engine writes into the app's folder to the document the user picked.
 *
 * The engine writes with plain `std::fs` (projects, rendered movies, image sequences, exported
 * JSON), but Android only lets an app write a user-chosen location through a `content://` URI.
 * So the engine writes to a staging file and this class copies it out:
 *  - one-shot exports (renders): once, after the file has stopped changing for ~2 s;
 *  - projects (`keepSyncing`): again after every later save, while the app runs.
 *
 * Limit: "stopped changing" is a heuristic. A muxer that pauses for more than two seconds mid
 * write could be copied early; a later change is copied again only for `keepSyncing` entries.
 */
class Exports(private val context: Context) {
    private class Entry(val staged: File, val uri: Uri, val keepSyncing: Boolean, alreadySynced: Boolean) {
        var lastSize = -1L
        var lastModified = -1L
        var stablePolls = 0
        // A file just copied in from `uri` is already in step with it: only later saves are copied back.
        var copiedModified = if (alreadySynced) staged.lastModified() else -1L
        val created = System.currentTimeMillis()
    }

    private val thread = HandlerThread("wordcraft-exports").apply { start() }
    private val handler = Handler(thread.looper)
    private val main = Handler(Looper.getMainLooper())
    private val entries = mutableListOf<Entry>()
    private var polling = false

    fun watch(staged: File, uri: Uri, keepSyncing: Boolean, alreadySynced: Boolean = false) {
        handler.post {
            entries.removeAll { it.staged == staged }
            entries.add(Entry(staged, uri, keepSyncing, alreadySynced))
            if (!polling) {
                polling = true
                handler.postDelayed(::poll, POLL_MS)
            }
        }
    }

    private fun poll() {
        val done = mutableListOf<Entry>()
        for (e in entries) {
            if (!e.staged.exists()) {
                if (!e.keepSyncing && System.currentTimeMillis() - e.created > ONE_SHOT_TTL_MS) done += e
                continue
            }
            val size = e.staged.length()
            val modified = e.staged.lastModified()
            if (size == e.lastSize && modified == e.lastModified) e.stablePolls++ else e.stablePolls = 0
            e.lastSize = size
            e.lastModified = modified
            if (e.stablePolls >= STABLE_POLLS && size > 0 && modified != e.copiedModified) {
                if (copy(e)) {
                    e.copiedModified = modified
                    if (!e.keepSyncing) done += e
                }
            }
        }
        entries.removeAll(done)
        if (entries.isEmpty()) polling = false else handler.postDelayed(::poll, POLL_MS)
    }

    private fun copy(e: Entry): Boolean = try {
        context.contentResolver.openOutputStream(e.uri, "wt")?.use { out ->
            e.staged.inputStream().use { it.copyTo(out, 1 shl 20) }
        } ?: error("no output stream")
        main.post { Toast.makeText(context, "Saved ${e.staged.name}", Toast.LENGTH_SHORT).show() }
        true
    } catch (t: Throwable) {
        Log.w(TAG, "export of ${e.staged} failed", t)
        main.post { Toast.makeText(context, "Could not save ${e.staged.name}: ${t.message}", Toast.LENGTH_LONG).show() }
        false
    }

    companion object {
        private const val TAG = "WordCraft"
        private const val POLL_MS = 1000L
        private const val STABLE_POLLS = 2
        private const val ONE_SHOT_TTL_MS = 6L * 60 * 60 * 1000
    }
}
