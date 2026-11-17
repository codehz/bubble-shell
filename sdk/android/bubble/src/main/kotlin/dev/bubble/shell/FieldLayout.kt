package dev.bubble.shell

import android.graphics.Paint
import android.text.Layout
import android.text.StaticLayout
import android.text.TextPaint
import android.graphics.Typeface
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.LinkedHashMap
import kotlin.math.max
import kotlin.math.min

/** The StaticLayout BubbleView draws is the only geometry source for carets. */
internal object FieldLayout {
    private const val MAGIC = 0x314C5342
    private val editCache = TextLayoutCache()

    @JvmStatic
    var hostFamily: Typeface = Typeface.DEFAULT
        set(value) {
            if (field === value) return
            field = value
            editCache.clear()
        }

    fun clearEditCache() {
        editCache.clear()
    }

    fun cachedSnapshot(
        text: String,
        width: Float,
        fontSize: Float,
        lineHeight: Float,
        density: Float,
        weight: Int,
        align: Int,
        revision: Long,
        family: Typeface = hostFamily,
    ): ByteArray {
        return editCache.snapshot(
            text,
            width,
            fontSize,
            lineHeight,
            density,
            weight,
            align,
            revision,
            family,
        )
    }

    fun build(
        text: String,
        paint: TextPaint,
        width: Float,
        lineHeight: Float,
        font: Float,
        align: Int,
    ): StaticLayout {
        val spacing = if (font > 0f && lineHeight > 0f) (lineHeight / font).coerceAtLeast(0.1f) else 1f
        val alignment = when (align) {
            1 -> Layout.Alignment.ALIGN_CENTER
            2 -> Layout.Alignment.ALIGN_OPPOSITE
            else -> Layout.Alignment.ALIGN_NORMAL
        }
        return StaticLayout.Builder
            .obtain(text, 0, text.length, paint, width.toInt().coerceAtLeast(1))
            .setIncludePad(false)
            .setLineSpacing(0f, spacing)
            .setAlignment(alignment)
            .build()
    }

    fun snapshot(
        text: String,
        layout: StaticLayout,
        width: Float,
        fontSize: Float,
        lineHeight: Float,
        density: Float,
        weight: Int,
        align: Int,
        revision: Long,
    ): ByteArray {
        val units = text.length
        val caretBytes = (units + 1) * 12
        val runs = runs(text, layout)
        val buffer = ByteBuffer.allocate(40 + caretBytes + runs.size * 28).order(ByteOrder.LITTLE_ENDIAN)
        buffer.putInt(MAGIC)
        buffer.putInt(units)
        buffer.putInt(runs.size)
        buffer.putFloat(width)
        buffer.putFloat(fontSize)
        buffer.putFloat(lineHeight)
        buffer.putFloat(density)
        buffer.putShort(weight.toShort())
        buffer.put(align.toByte())
        buffer.put(0)
        buffer.putLong(revision)
        for (i in 0..units) {
            if (layout.lineCount == 0) {
                buffer.putFloat(0f)
                buffer.putFloat(0f)
                buffer.putFloat(lineHeight)
                continue
            }
            val line = layout.getLineForOffset(i).coerceIn(0, layout.lineCount - 1)
            buffer.putFloat(layout.getPrimaryHorizontal(i))
            buffer.putFloat(layout.getLineTop(line).toFloat())
            buffer.putFloat((layout.getLineBottom(line) - layout.getLineTop(line)).toFloat())
        }
        for (run in runs) {
            buffer.putInt(run.start)
            buffer.putInt(run.end)
            buffer.putFloat(run.x)
            buffer.putFloat(run.y)
            buffer.putFloat(run.w)
            buffer.putFloat(run.h)
            buffer.put(run.level.toByte())
            buffer.put(0)
            buffer.put(0)
            buffer.put(0)
        }
        return buffer.array()
    }

    private class Run(
        val start: Int,
        val end: Int,
        val x: Float,
        val y: Float,
        val w: Float,
        val h: Float,
        val level: Int,
    )

    private fun runs(text: String, layout: StaticLayout): List<Run> {
        if (layout.lineCount == 0 || text.isEmpty()) return emptyList()
        val out = ArrayList<Run>()
        for (line in 0 until layout.lineCount) {
            val start = layout.getLineStart(line)
            var end = layout.getLineEnd(line)
            while (end > start && (text[end - 1] == '\n' || text[end - 1] == '\r')) end -= 1
            val top = layout.getLineTop(line).toFloat()
            val height = (layout.getLineBottom(line) - layout.getLineTop(line)).toFloat()
            var i = start
            while (i < end) {
                val x0 = layout.getPrimaryHorizontal(i)
                var j = i
                var rtl: Boolean? = null
                var edge = x0
                while (j < end) {
                    val step = if (Character.isHighSurrogate(text[j]) && j + 1 < text.length) 2 else 1
                    val next = min(j + step, end)
                    if (next == j) break
                    val x1 = layout.getPrimaryHorizontal(next)
                    val stepRtl = x1 < edge - 0.01f
                    if (rtl == null) rtl = stepRtl else if (stepRtl != rtl) break
                    edge = x1
                    j = next
                }
                if (j == i) break
                out.add(Run(i, j, min(x0, edge), top, max(x0, edge) - min(x0, edge), height, if (rtl == true) 1 else 0))
                i = j
            }
        }
        return out
    }
}

/**
 * Bounded layouts for one host. Each entry owns its TextPaint, so a color
 * write before draw cannot change another retained layout. Position, clip, and
 * color are not part of the key.
 */
internal class TextLayoutCache(
    private val layoutCap: Int = 256,
    private val snapshotCap: Int = 64,
) {
    private class LayoutKey(
        val text: String,
        val widthPx: Int,
        val fontSizeBits: Int,
        val lineHeightBits: Int,
        val weight: Int,
        val align: Int,
        val family: Typeface,
    ) {
        override fun equals(other: Any?): Boolean {
            if (this === other) return true
            val key = other as? LayoutKey ?: return false
            return widthPx == key.widthPx &&
                fontSizeBits == key.fontSizeBits &&
                lineHeightBits == key.lineHeightBits &&
                weight == key.weight &&
                align == key.align &&
                family === key.family &&
                text == key.text
        }

        override fun hashCode(): Int {
            var result = text.hashCode()
            result = 31 * result + widthPx
            result = 31 * result + fontSizeBits
            result = 31 * result + lineHeightBits
            result = 31 * result + weight
            result = 31 * result + align
            result = 31 * result + System.identityHashCode(family)
            return result
        }
    }

    private class SnapshotKey(
        val text: String,
        val widthBits: Int,
        val fontSizeBits: Int,
        val lineHeightBits: Int,
        val densityBits: Int,
        val weight: Int,
        val align: Int,
        val revision: Long,
        val family: Typeface,
    ) {
        override fun equals(other: Any?): Boolean {
            if (this === other) return true
            val key = other as? SnapshotKey ?: return false
            return widthBits == key.widthBits &&
                fontSizeBits == key.fontSizeBits &&
                lineHeightBits == key.lineHeightBits &&
                densityBits == key.densityBits &&
                weight == key.weight &&
                align == key.align &&
                revision == key.revision &&
                family === key.family &&
                text == key.text
        }

        override fun hashCode(): Int {
            var result = text.hashCode()
            result = 31 * result + widthBits
            result = 31 * result + fontSizeBits
            result = 31 * result + lineHeightBits
            result = 31 * result + densityBits
            result = 31 * result + weight
            result = 31 * result + align
            result = 31 * result + revision.hashCode()
            result = 31 * result + System.identityHashCode(family)
            return result
        }
    }

    private class Lru<K, V>(private val cap: Int) : LinkedHashMap<K, V>(16, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<K, V>?): Boolean = size > cap
    }

    private val layouts = Lru<LayoutKey, StaticLayout>(layoutCap)
    private val snapshots = Lru<SnapshotKey, ByteArray>(snapshotCap)
    private val lock = Any()

    fun clear() {
        synchronized(lock) {
            layouts.clear()
            snapshots.clear()
        }
    }

    fun layout(
        text: String,
        width: Float,
        fontSize: Float,
        lineHeight: Float,
        weight: Int,
        align: Int,
        family: Typeface,
    ): StaticLayout {
        val key = LayoutKey(
            text,
            width.toInt().coerceAtLeast(1),
            java.lang.Float.floatToIntBits(fontSize),
            java.lang.Float.floatToIntBits(lineHeight),
            weight,
            align,
            family,
        )
        synchronized(lock) {
            layouts[key]?.let { return it }
        }
        val paint = TextPaint(Paint.ANTI_ALIAS_FLAG)
        paint.textSize = fontSize
        paint.typeface = if (weight >= 600) Typeface.create(family, Typeface.BOLD) else family
        val built = FieldLayout.build(text, paint, width, lineHeight, fontSize, align)
        synchronized(lock) {
            layouts[key]?.let { return it }
            layouts[key] = built
            return built
        }
    }

    fun snapshot(
        text: String,
        width: Float,
        fontSize: Float,
        lineHeight: Float,
        density: Float,
        weight: Int,
        align: Int,
        revision: Long,
        family: Typeface,
    ): ByteArray {
        val key = SnapshotKey(
            text,
            java.lang.Float.floatToIntBits(width),
            java.lang.Float.floatToIntBits(fontSize),
            java.lang.Float.floatToIntBits(lineHeight),
            java.lang.Float.floatToIntBits(density),
            weight,
            align,
            revision,
            family,
        )
        synchronized(lock) {
            snapshots[key]?.let { return it }
        }
        val layout = layout(text, width, fontSize, lineHeight, weight, align, family)
        val bytes = FieldLayout.snapshot(
            text,
            layout,
            width,
            fontSize,
            lineHeight,
            density,
            weight,
            align,
            revision,
        )
        synchronized(lock) {
            snapshots[key] = bytes
        }
        return bytes
    }
}
