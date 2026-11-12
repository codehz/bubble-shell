package dev.bubble.shell

import android.content.Context
import android.os.SystemClock
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.graphics.Typeface
import android.text.Layout
import android.text.StaticLayout
import android.text.TextPaint
import android.view.MotionEvent
import android.view.View
import android.view.WindowInsets
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.CursorAnchorInfo
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.roundToInt
import android.text.InputType

class BubbleView(context: Context) : View(context) {
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG)
    private val textPaint = TextPaint(Paint.ANTI_ALIAS_FLAG)
    private val imagePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { isFilterBitmap = true }
    private val path = Path()
    private val images = HashMap<String, Bitmap?>()
    private var paintBytes = ByteArray(0)
    private var family: Typeface = Typeface.DEFAULT
    private val activePointers = LinkedHashSet<Int>()

    init {
        setBackgroundColor(Color.BLACK)
        isFocusable = true
        isFocusableInTouchMode = true
        setOnApplyWindowInsetsListener { _, insets -> onApplyWindowInsets(insets) }
    }

    fun setHostTypeface(typeface: Typeface) {
        family = typeface
    }

    fun paintSize(): Int = paintBytes.size

    fun refresh() {
        BubbleNative.tick(SystemClock.uptimeMillis().toDouble())
        val size = contentSize()
        paintBytes = BubbleNative.frame(size[0], size[1])
        invalidate()
    }

    override fun onApplyWindowInsets(insets: WindowInsets): WindowInsets {
        if (android.os.Build.VERSION.SDK_INT >= 30) {
            val bars = insets.getInsets(WindowInsets.Type.systemBars())
            setPadding(bars.left, bars.top, bars.right, bars.bottom)
        }
        return insets
    }

    private fun contentSize(): FloatArray {
        val density = resources.displayMetrics.density.coerceAtLeast(1f)
        return floatArrayOf(
            (width - paddingLeft - paddingRight) / density,
            (height - paddingTop - paddingBottom) / density,
        )
    }

    override fun onDraw(canvas: Canvas) {
        val density = resources.displayMetrics.density
        canvas.translate(paddingLeft.toFloat(), paddingTop.toFloat())
        canvas.scale(density, density)
        val buffer = ByteBuffer.wrap(paintBytes).order(ByteOrder.LITTLE_ENDIAN)
        if (buffer.remaining() < 4) return
        val count = buffer.int
        val clips = ArrayDeque<Int>()
        val opacities = ArrayDeque<Float>()
        var alpha = 1f
        repeat(count) {
            if (!buffer.hasRemaining()) return
            when (buffer.get().toInt() and 0xff) {
                1 -> {
                    val x = buffer.float
                    val y = buffer.float
                    val w = buffer.float
                    val h = buffer.float
                    val radius = buffer.float
                    fill.color = color(buffer.float, buffer.float, buffer.float, buffer.float)
                    fill.style = Paint.Style.FILL
                    path.reset()
                    path.addRoundRect(x, y, x + w, y + h, radius, radius, Path.Direction.CW)
                    canvas.drawPath(path, fill)
                }
                2 -> {
                    val x = buffer.float
                    val y = buffer.float
                    val w = buffer.float
                    val h = buffer.float
                    val font = buffer.float
                    textPaint.color = color(buffer.float, buffer.float, buffer.float, buffer.float)
                    val lineHeight = buffer.float
                    val weight = buffer.short.toInt() and 0xffff
                    val align = buffer.get().toInt() and 0xff
                    textPaint.typeface = if (weight >= 600) Typeface.create(family, Typeface.BOLD) else family
                    textPaint.textSize = font
                    val n = buffer.int
                    val raw = ByteArray(n)
                    buffer.get(raw)
                    val text = String(raw, Charsets.UTF_8)
                    if (text.isNotEmpty() && w > 0f) {
                        val spacing = if (lineHeight > 0f) (lineHeight / font).coerceAtLeast(0.1f) else 1f
                        val alignment = when (align) {
                            1 -> Layout.Alignment.ALIGN_CENTER
                            2 -> Layout.Alignment.ALIGN_OPPOSITE
                            else -> Layout.Alignment.ALIGN_NORMAL
                        }
                        val layout = StaticLayout.Builder
                            .obtain(text, 0, text.length, textPaint, w.toInt().coerceAtLeast(1))
                            .setIncludePad(false)
                            .setLineSpacing(0f, spacing)
                            .setAlignment(alignment)
                            .build()
                        canvas.save()
                        canvas.translate(x, y)
                        canvas.clipRect(0f, 0f, w, h)
                        layout.draw(canvas)
                        canvas.restore()
                    } else {
                        canvas.drawText(text, x, y + font, textPaint)
                    }
                }
                7 -> {
                    val x = buffer.float
                    val y = buffer.float
                    val w = buffer.float
                    val h = buffer.float
                    val radius = buffer.float
                    val fit = buffer.get().toInt() and 0xff
                    val n = buffer.int
                    val raw = ByteArray(n)
                    buffer.get(raw)
                    val src = String(raw, Charsets.UTF_8)
                    val bitmap = if (images.containsKey(src)) {
                        images[src]
                    } else {
                        BitmapFactory.decodeFile(src).also { images[src] = it }
                    }
                    if (bitmap != null && w > 0f && h > 0f) {
                        val scale = if (fit == 0) {
                            minOf(w / bitmap.width, h / bitmap.height)
                        } else if (fit == 1) {
                            maxOf(w / bitmap.width, h / bitmap.height)
                        } else {
                            0f
                        }
                        val dst = if (scale > 0f) {
                            val dw = bitmap.width * scale
                            val dh = bitmap.height * scale
                            RectF(x + (w - dw) / 2, y + (h - dh) / 2, x + (w + dw) / 2, y + (h + dh) / 2)
                        } else {
                            RectF(x, y, x + w, y + h)
                        }
                        canvas.save()
                        path.reset()
                        path.addRoundRect(x, y, x + w, y + h, radius, radius, Path.Direction.CW)
                        canvas.clipPath(path)
                        canvas.drawBitmap(bitmap, null, dst, imagePaint)
                        canvas.restore()
                    }
                }
                3 -> {
                    val x = buffer.float
                    val y = buffer.float
                    val w = buffer.float
                    val h = buffer.float
                    canvas.save()
                    canvas.clipRect(x, y, x + w, y + h)
                    clips.addLast(1)
                }
                4 -> if (clips.isNotEmpty()) {
                    clips.removeLast()
                    canvas.restore()
                }
                5 -> {
                    val opacity = buffer.float
                    opacities.addLast(alpha)
                    alpha = (alpha * opacity).coerceIn(0f, 1f)
                    canvas.saveLayerAlpha(null, (alpha * 255).toInt())
                }
                6 -> if (opacities.isNotEmpty()) {
                    alpha = opacities.removeLast()
                    canvas.restore()
                }
                else -> return
            }
        }
    }

    private fun sendPointer(action: Int, index: Int, event: MotionEvent) {
        val id = event.getPointerId(index)
        if (action == 2 || action == 3) activePointers.remove(id) else activePointers.add(id)
        BubbleNative.pointer(
            action,
            id,
            localX(event.getX(index)),
            localY(event.getY(index)),
            event.eventTime.toDouble(),
            0,
            if (action == 2 || action == 3) 0 else 1,
        )
        refresh()
    }

    private data class EditSnapshot(
        val session: Long,
        val revision: Long,
        val anchor: Int,
        val active: Int,
        val compositionStart: Int,
        val compositionEnd: Int,
        val composing: Boolean,
    ) {
        val start: Int get() = minOf(anchor, active)
        val end: Int get() = maxOf(anchor, active)
        val hasComposition: Boolean get() = compositionEnd >= 0
    }

    private var editorType = InputType.TYPE_CLASS_TEXT
    private var autofillHints = emptyArray<String>()

    private fun editSnapshot(): EditSnapshot? {
        val raw = BubbleNative.editSnapshot()
        if (raw.size < 7) return null
        return EditSnapshot(
            session = raw[0],
            revision = raw[1],
            anchor = raw[2].toInt(),
            active = raw[3].toInt(),
            compositionStart = raw[4].toInt(),
            compositionEnd = raw[5].toInt(),
            composing = raw[6] != 0L,
        )
    }

    private fun publishCursor() {
        val caret = BubbleNative.caret()
        if (caret.size < 4) return
        val snapshot = editSnapshot() ?: return
        val density = resources.displayMetrics.density
        val x = paddingLeft + caret[0] * density
        val top = paddingTop + caret[1] * density
        val bottom = top + caret[3] * density
        val info = CursorAnchorInfo.Builder()
            .setMatrix(android.graphics.Matrix())
            .setSelectionRange(snapshot.start, snapshot.end)
            .setInsertionMarkerLocation(x, top, bottom, bottom, CursorAnchorInfo.FLAG_HAS_VISIBLE_REGION)
            .build()
        val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
        imm.updateCursorAnchorInfo(this, info)
    }

    override fun onCheckIsTextEditor(): Boolean = true

    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection {
        outAttrs.inputType = editorType
        outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI
        setAutofillHints(*autofillHints)
        val initial = editSnapshot()
        outAttrs.initialSelStart = initial?.start ?: 0
        outAttrs.initialSelEnd = initial?.end ?: 0
        return object : BaseInputConnection(this, true) {
            private val expectedSession = initial?.session ?: 0L
            private var batchDepth = 0

            private fun current(): EditSnapshot? {
                val snapshot = editSnapshot() ?: return null
                return if (snapshot.session == expectedSession) snapshot else null
            }

            private fun bytes(text: CharSequence): ByteArray =
                text.toString().toByteArray(Charsets.UTF_8)

            private fun wrote(code: Int): Boolean {
                if (code != 0) return false
                if (batchDepth == 0) {
                    refresh()
                    publishCursor()
                }
                return true
            }

            private fun replaceRange(
                snapshot: EditSnapshot,
                composing: Boolean,
                text: CharSequence,
                newCursorPosition: Int,
            ): Boolean {
                val (start, end) = if (snapshot.hasComposition) {
                    snapshot.compositionStart to snapshot.compositionEnd
                } else {
                    snapshot.start to snapshot.end
                }
                val code = BubbleNative.editReplace(
                    snapshot.session,
                    start,
                    end,
                    bytes(text),
                    newCursorPosition,
                    if (composing) 1 else 0,
                )
                return wrote(code)
            }

            override fun commitText(text: CharSequence, newCursorPosition: Int): Boolean {
                val snapshot = current() ?: return false
                return replaceRange(snapshot, false, text, newCursorPosition)
            }

            override fun setComposingText(text: CharSequence, newCursorPosition: Int): Boolean {
                val snapshot = current() ?: return false
                return replaceRange(snapshot, true, text, newCursorPosition)
            }

            override fun finishComposingText(): Boolean {
                val snapshot = current() ?: return false
                if (!snapshot.composing) return true
                return wrote(BubbleNative.editFinishComposition(snapshot.session))
            }

            override fun setComposingRegion(start: Int, end: Int): Boolean {
                val snapshot = current() ?: return false
                return wrote(BubbleNative.editSetComposition(snapshot.session, start, end))
            }

            override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
                val snapshot = current() ?: return false
                return wrote(
                    BubbleNative.editDeleteSurrounding(
                        snapshot.session,
                        beforeLength.coerceAtLeast(0),
                        afterLength.coerceAtLeast(0),
                    ),
                )
            }

            override fun deleteSurroundingTextInCodePoints(
                beforeLength: Int,
                afterLength: Int,
            ): Boolean {
                val snapshot = current() ?: return false
                return wrote(
                    BubbleNative.editDeleteSurroundingCodePoints(
                        snapshot.session,
                        beforeLength.coerceAtLeast(0),
                        afterLength.coerceAtLeast(0),
                    ),
                )
            }

            override fun getTextBeforeCursor(n: Int, flags: Int): CharSequence {
                val snapshot = current() ?: return ""
                val value = BubbleNative.focusedText()
                val cursor = snapshot.active.coerceIn(0, value.length)
                val from = if (n < 0) 0 else (cursor - n).coerceAtLeast(0)
                return value.substring(from, cursor)
            }

            override fun getTextAfterCursor(n: Int, flags: Int): CharSequence {
                val snapshot = current() ?: return ""
                val value = BubbleNative.focusedText()
                val cursor = snapshot.active.coerceIn(0, value.length)
                val to = if (n < 0) value.length else (cursor + n).coerceAtMost(value.length)
                return value.substring(cursor, to)
            }

            override fun getSelectedText(flags: Int): CharSequence? {
                val snapshot = current() ?: return null
                val value = BubbleNative.focusedText()
                val start = snapshot.start.coerceIn(0, value.length)
                val end = snapshot.end.coerceIn(0, value.length)
                if (start == end) return null
                return value.substring(start, end)
            }

            override fun setSelection(start: Int, end: Int): Boolean {
                val snapshot = current() ?: return false
                return wrote(BubbleNative.editSelect(snapshot.session, start, end))
            }

            override fun beginBatchEdit(): Boolean {
                val snapshot = current() ?: return false
                if (BubbleNative.editBatchBegin(snapshot.session) != 0) return false
                batchDepth += 1
                return true
            }

            override fun endBatchEdit(): Boolean {
                if (batchDepth == 0) return false
                val snapshot = current()
                if (snapshot == null) {
                    BubbleNative.editBatchAbort(expectedSession)
                    batchDepth = 0
                    return false
                }
                if (BubbleNative.editBatchEnd(snapshot.session) != 0) return false
                batchDepth -= 1
                if (batchDepth == 0) {
                    refresh()
                    publishCursor()
                }
                return true
            }

            override fun closeConnection() {
                if (batchDepth > 0) {
                    BubbleNative.editBatchAbort(expectedSession)
                    batchDepth = 0
                }
                super.closeConnection()
            }

            override fun getExtractedText(
                request: android.view.inputmethod.ExtractedTextRequest?,
                flags: Int,
            ): android.view.inputmethod.ExtractedText? {
                val snapshot = current() ?: return null
                val value = BubbleNative.focusedText()
                val extracted = android.view.inputmethod.ExtractedText()
                extracted.text = value
                extracted.startOffset = 0
                extracted.partialStartOffset = -1
                extracted.partialEndOffset = -1
                extracted.selectionStart = snapshot.start
                extracted.selectionEnd = snapshot.end
                extracted.flags = if (
                    (editorType and InputType.TYPE_TEXT_FLAG_MULTI_LINE) == 0
                ) {
                    android.view.inputmethod.ExtractedText.FLAG_SINGLE_LINE
                } else {
                    0
                }
                return extracted
            }

            override fun sendKeyEvent(event: android.view.KeyEvent): Boolean {
                if (event.action != android.view.KeyEvent.ACTION_DOWN) {
                    return super.sendKeyEvent(event)
                }
                val snapshot = current() ?: return false
                return when (event.keyCode) {
                    android.view.KeyEvent.KEYCODE_DEL -> {
                        if (snapshot.hasComposition) {
                            replaceRange(snapshot, false, "", 1)
                        } else {
                            wrote(BubbleNative.editDeleteSurrounding(snapshot.session, 1, 0))
                        }
                    }
                    android.view.KeyEvent.KEYCODE_FORWARD_DEL -> {
                        if (snapshot.hasComposition) {
                            replaceRange(snapshot, false, "", 1)
                        } else {
                            wrote(BubbleNative.editDeleteSurrounding(snapshot.session, 0, 1))
                        }
                    }
                    else -> super.sendKeyEvent(event)
                }
            }

            override fun requestCursorUpdates(cursorUpdateMode: Int): Boolean {
                publishCursor()
                return true
            }
        }
    }

    fun cancelActivePointers(timeMs: Double) {
        if (activePointers.isEmpty()) return
        val ids = activePointers.toList()
        activePointers.clear()
        for (id in ids) {
            BubbleNative.pointer(3, id, 0f, 0f, timeMs, 0, 0)
        }
        refresh()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (!hasFocus) cancelActivePointers(SystemClock.uptimeMillis().toDouble())
    }

    override fun onDetachedFromWindow() {
        cancelActivePointers(SystemClock.uptimeMillis().toDouble())
        super.onDetachedFromWindow()
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> sendPointer(0, event.actionIndex, event)
            MotionEvent.ACTION_POINTER_DOWN -> sendPointer(0, event.actionIndex, event)
            MotionEvent.ACTION_MOVE -> {
                for (index in 0 until event.pointerCount) {
                    val id = event.getPointerId(index)
                    if (!activePointers.contains(id)) continue
                    BubbleNative.pointer(
                        1,
                        id,
                        localX(event.getX(index)),
                        localY(event.getY(index)),
                        event.eventTime.toDouble(),
                        0,
                        1,
                    )
                }
                refresh()
            }
            MotionEvent.ACTION_POINTER_UP -> sendPointer(2, event.actionIndex, event)
            MotionEvent.ACTION_UP -> {
                sendPointer(2, event.actionIndex, event)
                val handle = BubbleNative.focusAt(localX(event.x), localY(event.y))
                if (handle != 0) {
                    val flags = BubbleNative.fieldFlags(handle)
                    editorType = editorType(flags)
                    autofillHints = autofillHints(flags)
                    setAutofillHints(*autofillHints)
                    importantForAutofill = IMPORTANT_FOR_AUTOFILL_YES
                    requestFocus()
                    val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
                    imm.restartInput(this)
                    imm.showSoftInput(this, InputMethodManager.SHOW_IMPLICIT)
                }
            }
            MotionEvent.ACTION_CANCEL -> cancelActivePointers(event.eventTime.toDouble())
        }
        return true
    }

    private fun localX(x: Float) = (x - paddingLeft) / resources.displayMetrics.density
    private fun localY(y: Float) = (y - paddingTop) / resources.displayMetrics.density

    // Paint carries extended nonlinear sRGB; Android renders sRGB, so clamp.
    private fun color(red: Float, green: Float, blue: Float, alpha: Float): Int {
        fun channel(value: Float): Int = (value.coerceIn(0f, 1f) * 255f).roundToInt()
        return Color.argb(channel(alpha), channel(red), channel(green), channel(blue))
    }
}

private fun editorType(flags: Int): Int {
    val kind = flags shr 8 and 0xff
    var type = when (kind) {
        1 -> InputType.TYPE_CLASS_NUMBER
        2 -> InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS
        3 -> InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
        4 -> InputType.TYPE_CLASS_PHONE
        else -> InputType.TYPE_CLASS_TEXT
    }
    if (flags and 2 != 0) {
        type = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
    }
    if (flags and 1 != 0) {
        type = type or InputType.TYPE_TEXT_FLAG_MULTI_LINE
    }
    return type
}

private fun autofillHints(flags: Int): Array<String> {
    return when (flags shr 16 and 0xff) {
        1 -> arrayOf(View.AUTOFILL_HINT_USERNAME)
        2 -> arrayOf(View.AUTOFILL_HINT_PASSWORD)
        3 -> arrayOf("newPassword")
        4 -> arrayOf(View.AUTOFILL_HINT_EMAIL_ADDRESS)
        5 -> arrayOf(View.AUTOFILL_HINT_PHONE)
        6 -> arrayOf("smsOTPCode")
        else -> emptyArray()
    }
}
