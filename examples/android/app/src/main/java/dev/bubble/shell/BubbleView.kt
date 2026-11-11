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
import java.nio.ByteBuffer
import java.nio.ByteOrder

class BubbleView(context: Context) : View(context) {
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG)
    private val textPaint = TextPaint(Paint.ANTI_ALIAS_FLAG)
    private val imagePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { isFilterBitmap = true }
    private val path = Path()
    private val images = HashMap<String, Bitmap?>()
    private var paintBytes = ByteArray(0)
    private var family: Typeface = Typeface.DEFAULT
    private var lastY = 0f
    private var tracking = false

    init {
        setBackgroundColor(Color.BLACK)
        setOnApplyWindowInsetsListener { _, insets -> onApplyWindowInsets(insets) }
    }

    fun setHostTypeface(typeface: Typeface) {
        family = typeface
    }

    fun paintSize(): Int = paintBytes.size

    fun refresh() {
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
                    fill.color = color(buffer.int)
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
                    textPaint.color = color(buffer.int)
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
                    val bitmap = images.getOrPut(src) { BitmapFactory.decodeFile(src) }
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

    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                tracking = true
                lastY = event.y
                BubbleNative.pointerDown(localX(event.x), localY(event.y))
            }
            MotionEvent.ACTION_MOVE -> if (tracking) {
                BubbleNative.scroll(localX(event.x), localY(event.y), (lastY - event.y) / resources.displayMetrics.density)
                lastY = event.y
                refresh()
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> tracking = false
        }
        return true
    }

    private fun localX(x: Float) = (x - paddingLeft) / resources.displayMetrics.density
    private fun localY(y: Float) = (y - paddingTop) / resources.displayMetrics.density

    private fun color(argb: Int): Int = argb
}
