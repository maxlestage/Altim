package com.maxlestage.altim.kit

import java.io.ByteArrayOutputStream

/**
 * Incremental Server-Sent Events parser: feed it bytes as they arrive, get the complete `data:` payloads.
 * Handles chunks cut anywhere (even inside a UTF-8 character), CRLF, comments (`: ok` heartbeats) and `retry:`.
 */
class SseParser {
    private val buffer = ByteArrayOutputStream()
    private val data = mutableListOf<String>()
    var retryMs: Int? = null
        private set

    fun feed(chunk: ByteArray, length: Int = chunk.size): List<String> {
        val events = mutableListOf<String>()
        for (i in 0 until length) {
            val b = chunk[i]
            if (b != '\n'.code.toByte()) {
                buffer.write(b.toInt())
                continue
            }
            var bytes = buffer.toByteArray()
            buffer.reset()
            if (bytes.isNotEmpty() && bytes.last() == '\r'.code.toByte()) bytes = bytes.copyOf(bytes.size - 1)
            val line = bytes.toString(Charsets.UTF_8)
            when {
                line.isEmpty() -> {
                    if (data.isNotEmpty()) events += data.joinToString("\n")
                    data.clear()
                }
                line.startsWith(":") -> Unit
                line.startsWith("data:") -> data += line.removePrefix("data:").removePrefix(" ")
                line.startsWith("retry:") -> retryMs = line.removePrefix("retry:").trim().toIntOrNull()
            }
        }
        return events
    }

    /** Parses the live price ticks, ignoring any malformed event. */
    fun ticks(chunk: ByteArray, length: Int = chunk.size): List<LiveTick> =
        feed(chunk, length).mapNotNull { runCatching { AltimJson.decodeFromString(LiveTick.serializer(), it) }.getOrNull() }
}
