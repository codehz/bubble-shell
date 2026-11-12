package dev.bubble.shell

import android.content.Context
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
        BubbleNative.tick(System.nanoTime() / 1_000_000.0)
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

    private var composing = false
    private var composingText = ""
    private var editorType = InputType.TYPE_CLASS_TEXT
    private var autofillHints = emptyArray<String>()

    private fun sendEdit(action: Int, text: String, a: Int = 0, b: Int = 0) {
        BubbleNative.edit(action, text.toByteArray(Charsets.UTF_8), a, b)
        refresh()
    }

    override fun onCheckIsTextEditor(): Boolean = true

    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection {
        outAttrs.inputType = editorType
        outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI
        outAttrs.setAutofillHints(*autofillHints)
        return object : BaseInputConnection(this, true) {
            override fun commitText(text: CharSequence, newCursorPosition: Int): Boolean {
                val value = text.toString()
                if (composing) {
                    sendEdit(7, value)
                    composing = false
                    composingText = ""
                } else if (value.isNotEmpty()) {
                    sendEdit(1, value)
                }
                return true
            }

            override fun setComposingText(text: CharSequence, newCursorPosition: Int): Boolean {
                val value = text.toString()
                if (!composing) {
                    sendEdit(5, "")
                    composing = true
                }
                composingText = value
                sendEdit(6, value)
                return true
            }

            override fun finishComposingText(): Boolean {
                if (composing) {
                    sendEdit(7, composingText)
                    composing = false
                    composingText = ""
                }
                return true
            }

            override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
                repeat(beforeLength.coerceAtLeast(0)) { sendEdit(2, "") }
                repeat(afterLength.coerceAtLeast(0)) { sendEdit(3, "") }
                return true
            }
        }
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
                    requestFocus()
                    val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
                    imm.restartInput(this)
                    imm.showSoftInput(this, InputMethodManager.SHOW_IMPLICIT)
                }
            }
            MotionEvent.ACTION_CANCEL -> {
                for (id in activePointers.toList()) {
                    BubbleNative.pointer(3, id, 0f, 0f, event.eventTime.toDouble(), 0, 0)
                }
                activePointers.clear()
                refresh()
            }
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
