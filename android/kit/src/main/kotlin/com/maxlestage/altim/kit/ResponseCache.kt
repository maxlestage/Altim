package com.maxlestage.altim.kit

import java.io.File

/**
 * Last good answers of the server, kept on the phone: when the network or the server is down, the app shows them with
 * their time instead of an error ("hors ligne : données de 10:32").
 */
interface ResponseCache {
    fun load(key: String): Pair<String, Long>?
    fun save(key: String, body: String)
}

/** One file per request in a directory (the app's private cache), at most [limit] files, the oldest dropped. */
class FileResponseCache(private val dir: File, private val limit: Int = 300) : ResponseCache {
    private var writes = 0

    init {
        dir.mkdirs()
    }

    companion object {
        /** FNV-1a 64 bits of the request (path and query): a stable file name without any personal data in it. */
        fun fileName(key: String): String {
            var h = -0x340d631b7bdddcdbL // 0xcbf29ce484222325
            for (b in key.toByteArray()) {
                h = h xor (b.toLong() and 0xff)
                h *= 0x100000001b3L
            }
            return java.lang.Long.toUnsignedString(h, 16) + ".json"
        }
    }

    override fun load(key: String): Pair<String, Long>? {
        val f = File(dir, fileName(key))
        return if (f.isFile) runCatching { f.readText() to f.lastModified() }.getOrNull() else null
    }

    @Synchronized
    override fun save(key: String, body: String) {
        val f = File(dir, fileName(key))
        val tmp = File(dir, f.name + ".tmp")
        runCatching {
            tmp.writeText(body)
            if (!tmp.renameTo(f)) {
                f.delete()
                tmp.renameTo(f)
            }
        }
        if (++writes % 25 == 0) prune()
    }

    /** Keeps the [limit] most recent files. */
    fun prune() {
        val files = dir.listFiles { f -> f.name.endsWith(".json") } ?: return
        if (files.size <= limit) return
        files.sortedByDescending { it.lastModified() }.drop(limit).forEach { it.delete() }
    }

    fun clear() {
        dir.listFiles()?.forEach { it.delete() }
    }
}
