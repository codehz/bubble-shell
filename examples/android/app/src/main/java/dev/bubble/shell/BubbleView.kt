package dev.bubble.shell

import android.content.Context
import android.graphics.Typeface
import android.os.SystemClock
import android.view.Choreographer
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.Rect
import android.graphics.RectF
import java.io.File
import android.text.TextPaint
import android.view.MotionEvent
import android.view.View
import android.view.WindowInsets
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityNodeProvider
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.CursorAnchorInfo
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.hypot
import kotlin.math.roundToInt
import android.os.Bundle
import android.text.InputType
import org.json.JSONArray
import org.json.JSONObject

private const val A11Y_SCROLL_FORWARD = 8
private const val A11Y_SCROLL_BACKWARD = 16
private const val A11Y_SCROLL_LEFT = 32
private const val A11Y_SCROLL_RIGHT = 64
private const val A11Y_SCROLL_UP = 128
private const val A11Y_SCROLL_DOWN = 256

private val ACTION_SCROLL_LEFT_ID = AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_LEFT.id
private val ACTION_SCROLL_RIGHT_ID = AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_RIGHT.id
private val ACTION_SCROLL_UP_ID = AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_UP.id
private val ACTION_SCROLL_DOWN_ID = AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_DOWN.id

class BubbleView(context: Context) : View(context) {
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG)
    private val textPaint = TextPaint(Paint.ANTI_ALIAS_FLAG)
    private val imagePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { isFilterBitmap = true }
    private val path = Path()
    private val images = HashMap<String, Bitmap?>()
    private var paintBytes = ByteArray(0)
    private var family: Typeface = Typeface.DEFAULT
    private val activePointers = LinkedHashSet<Int>()
    private var semanticNodes: Map<Int, JSONObject> = emptyMap()
    private var semanticChildren: Map<Int, List<Int>> = emptyMap()
    private var semanticRoots: List<Int> = emptyList()
    private var accessibilityFocus = 0
    private var semanticSnapshot = ""
    private var selectionDrag: Pair<Int, Int>? = null
    private var engine = false
    private var devUrl: String? = null
    private var devStamp = ""
    private var devBusy = false
    private var lastDev = 0L
    var onFrame: (() -> Unit)? = null
    private val frameCallback: Choreographer.FrameCallback = Choreographer.FrameCallback {
        if (!isAttachedToWindow) return@FrameCallback
        pollDev()
        refresh()
        onFrame?.invoke()
        Choreographer.getInstance().postFrameCallback(frameCallback)
    }

    init {
        setBackgroundColor(Color.BLACK)
        isFocusable = true
        isFocusableInTouchMode = true
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_YES
        setOnApplyWindowInsetsListener { _, insets -> onApplyWindowInsets(insets) }
    }

    fun setHostTypeface(typeface: Typeface) {
        family = typeface
        FieldLayout.hostFamily = typeface
    }

    fun open(storageDir: File, font: ByteArray, manifest: String?, entry: ByteArray?, devUrl: String?) {
        if (!engine) {
            check(BubbleNative.create() == 0)
            engine = true
        }
        BubbleNative.setStorageDir(storageDir.absolutePath)
        BubbleNative.registerFont("HostFont", font)
        val file = File(context.cacheDir, "HostFont.ttf")
        file.writeBytes(font)
        setHostTypeface(Typeface.createFromFile(file))
        this.devUrl = devUrl
        if (devUrl == null && manifest != null && entry != null) {
            check(BubbleNative.load(manifest, entry) == 0)
        }
    }

    fun destroyEngine() {
        if (!engine) return
        Choreographer.getInstance().removeFrameCallback(frameCallback)
        BubbleNative.destroy()
        engine = false
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        if (engine) BubbleNative.setVisible(1)
        Choreographer.getInstance().postFrameCallback(frameCallback)
    }

    override fun onWindowVisibilityChanged(visibility: Int) {
        super.onWindowVisibilityChanged(visibility)
        if (!engine) return
        if (visibility == VISIBLE) {
            BubbleNative.setVisible(1)
        } else {
            cancelActivePointers(SystemClock.uptimeMillis().toDouble())
            BubbleNative.setVisible(0)
        }
    }

    private fun pollDev() {
        val url = devUrl ?: return
        val now = SystemClock.uptimeMillis()
        if (devBusy || now - lastDev < 1000) return
        lastDev = now
        devBusy = true
        Thread {
            try {
                val text = java.net.URL(url).readText()
                val payload = org.json.JSONObject(text)
                val manifest = payload.getString("manifest")
                val entry = payload.getString("entry")
                val stamp = manifest + "\n" + entry
                if (stamp == devStamp) return@Thread
                post {
                    devStamp = stamp
                    BubbleNative.load(manifest, entry.toByteArray())
                }
            } catch (_: Exception) {
            } finally {
                devBusy = false
            }
        }.start()
    }

    fun paintSize(): Int = paintBytes.size

    fun refresh() {
        BubbleNative.tick(SystemClock.uptimeMillis().toDouble())
        drainHostCalls(context)
        val size = contentSize()
        paintBytes = BubbleNative.frame(size[0], size[1])
        updateSemantics()
        updateInputSession()
        invalidate()
    }

    private fun updateSemantics() {
        val raw = BubbleNative.semantics()
        if (raw == semanticSnapshot) return
        semanticSnapshot = raw
        val previousFocus = semanticNodes.entries.firstOrNull { it.value.optBoolean("focused") }?.key
        val nodes = LinkedHashMap<Int, JSONObject>()
        val children = HashMap<Int, MutableList<Int>>()
        val roots = mutableListOf<Int>()
        try {
            val array = JSONObject(raw).optJSONArray("nodes") ?: JSONArray()
            for (index in 0 until array.length()) {
                val node = array.getJSONObject(index)
                nodes[node.getInt("handle")] = node
            }
            for ((handle, node) in nodes) {
                val parent = node.optInt("parent", 0)
                if (parent != 0 && nodes.containsKey(parent)) {
                    children.getOrPut(parent) { mutableListOf() }.add(handle)
                } else {
                    roots.add(handle)
                }
            }
        } catch (_: Exception) {
            nodes.clear()
        }
        semanticNodes = nodes
        semanticChildren = children
        semanticRoots = roots
        if (accessibilityFocus != 0 &&
            (nodes[accessibilityFocus] == null || nodes[accessibilityFocus]?.optBoolean("hidden") == true)
        ) {
            val previous = accessibilityFocus
            accessibilityFocus = 0
            sendSemanticEvent(previous, AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUS_CLEARED)
        }
        sendSemanticEvent(AccessibilityNodeProvider.HOST_VIEW_ID, AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED)
        val focused = nodes.entries.firstOrNull { it.value.optBoolean("focused") }?.key
        if (focused != null && focused != previousFocus) {
            sendSemanticEvent(focused, AccessibilityEvent.TYPE_VIEW_FOCUSED)
        }
    }

    private fun sendSemanticEvent(handle: Int, type: Int) {
        val manager = context.getSystemService(Context.ACCESSIBILITY_SERVICE) as android.view.accessibility.AccessibilityManager
        if (!manager.isEnabled) return
        val event = AccessibilityEvent.obtain(type)
        event.packageName = context.packageName
        event.className = "android.view.View"
        event.setSource(this, handle)
        if (type == AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED) {
            event.contentChangeTypes = AccessibilityEvent.CONTENT_CHANGE_TYPE_SUBTREE
        }
        parent?.requestSendAccessibilityEvent(this, event)
    }

    private fun screenRect(node: JSONObject): Rect {
        val density = resources.displayMetrics.density
        val location = IntArray(2)
        getLocationOnScreen(location)
        val bounds = node.optJSONArray("bounds") ?: JSONArray()
        val x = bounds.optDouble(0, 0.0).toFloat()
        val y = bounds.optDouble(1, 0.0).toFloat()
        val w = bounds.optDouble(2, 0.0).toFloat()
        val h = bounds.optDouble(3, 0.0).toFloat()
        return Rect(
            location[0] + paddingLeft + (x * density).roundToInt(),
            location[1] + paddingTop + (y * density).roundToInt(),
            location[0] + paddingLeft + ((x + w) * density).roundToInt(),
            location[1] + paddingTop + ((y + h) * density).roundToInt(),
        )
    }

    override fun getAccessibilityNodeProvider(): AccessibilityNodeProvider = semanticProvider

    private val semanticProvider = object : AccessibilityNodeProvider() {
        override fun findFocus(focus: Int): AccessibilityNodeInfo? {
            val handle = if (focus == AccessibilityNodeInfo.FOCUS_ACCESSIBILITY) {
                accessibilityFocus.takeIf { it != 0 }
            } else if (focus == AccessibilityNodeInfo.FOCUS_INPUT) {
                semanticNodes.entries.firstOrNull { it.value.optBoolean("focused") }?.key
            } else {
                null
            }
            return handle?.let { createAccessibilityNodeInfo(it) }
        }

        override fun createAccessibilityNodeInfo(virtualViewId: Int): AccessibilityNodeInfo? {
            if (virtualViewId == HOST_VIEW_ID) {
                val info = AccessibilityNodeInfo.obtain(this@BubbleView)
                info.packageName = context.packageName
                info.className = "android.view.View"
                info.isEnabled = true
                info.isFocusable = true
                val location = IntArray(2)
                getLocationOnScreen(location)
                info.setBoundsInScreen(
                    Rect(
                        location[0] + paddingLeft,
                        location[1] + paddingTop,
                        location[0] + width - paddingRight,
                        location[1] + height - paddingBottom,
                    ),
                )
                for (handle in semanticRoots) {
                    info.addChild(this@BubbleView, handle)
                }
                return info
            }
            val node = semanticNodes[virtualViewId] ?: return null
            val editable = node.optBoolean("editable")
            val hidden = node.optBoolean("hidden")
            val actions = node.optInt("actions")
            val info = AccessibilityNodeInfo.obtain()
            info.setSource(this@BubbleView, virtualViewId)
            info.packageName = context.packageName
            info.className = if (editable) "android.widget.EditText" else "android.view.View"
            info.setParent(this@BubbleView, node.optInt("parent", 0).takeIf { it != 0 } ?: HOST_VIEW_ID)
            for (child in semanticChildren[virtualViewId].orEmpty()) {
                info.addChild(this@BubbleView, child)
            }
            val label = node.optString("label")
            val value = node.optString("value")
            val role = node.optString("role")
            if (role == "text") info.text = label else if (editable) info.text = value
            info.contentDescription = if (editable) label else label.ifEmpty { value }
            info.isEnabled = !node.optBoolean("disabled")
            info.isVisibleToUser = !hidden
            info.isFocusable = node.optBoolean("focusable") || editable
            info.isFocused = node.optBoolean("focused")
            info.isAccessibilityFocused = accessibilityFocus == virtualViewId
            info.isClickable = actions and 2 != 0
            info.isEditable = editable
            info.isScrollable = node.optBoolean("scrollable")
            info.isSelected = node.optBoolean("selected")
            if (node.has("checked")) info.isChecked = node.optBoolean("checked")
            info.setBoundsInScreen(screenRect(node))
            if (!hidden && info.isEnabled) {
                info.addAction(
                    if (info.isAccessibilityFocused) AccessibilityNodeInfo.ACTION_CLEAR_ACCESSIBILITY_FOCUS
                    else AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS,
                )
            }
            if (actions and 1 != 0) info.addAction(AccessibilityNodeInfo.ACTION_FOCUS)
            if (actions and 2 != 0) info.addAction(AccessibilityNodeInfo.ACTION_CLICK)
            if (actions and 4 != 0) info.addAction(AccessibilityNodeInfo.ACTION_SET_TEXT)
            if (actions and A11Y_SCROLL_FORWARD != 0) {
                info.addAction(AccessibilityNodeInfo.ACTION_SCROLL_FORWARD)
                info.addAction(AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD)
            }
            if (actions and A11Y_SCROLL_LEFT != 0) info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_LEFT)
            if (actions and A11Y_SCROLL_RIGHT != 0) info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_RIGHT)
            if (actions and A11Y_SCROLL_UP != 0) info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_UP)
            if (actions and A11Y_SCROLL_DOWN != 0) info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_DOWN)
            return info
        }

        override fun performAction(
            virtualViewId: Int,
            action: Int,
            arguments: Bundle?,
        ): Boolean {
            if (virtualViewId == HOST_VIEW_ID) return false
            val node = semanticNodes[virtualViewId] ?: return false
            when (action) {
                AccessibilityNodeInfo.ACTION_CLICK -> {
                    if (BubbleNative.a11yAction(virtualViewId, 1, "") != 0) return false
                    afterA11yAction()
                    return true
                }
                AccessibilityNodeInfo.ACTION_FOCUS -> {
                    if (BubbleNative.a11yAction(virtualViewId, 0, "") != 0) return false
                    afterA11yAction()
                    return true
                }
                AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS -> {
                    if (node.optBoolean("hidden") || node.optBoolean("disabled")) return false
                    if (accessibilityFocus == virtualViewId) return false
                    if (accessibilityFocus != 0) {
                        sendSemanticEvent(accessibilityFocus, AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUS_CLEARED)
                    }
                    accessibilityFocus = virtualViewId
                    sendSemanticEvent(virtualViewId, AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUSED)
                    return true
                }
                AccessibilityNodeInfo.ACTION_CLEAR_ACCESSIBILITY_FOCUS -> {
                    if (accessibilityFocus == virtualViewId) {
                        accessibilityFocus = 0
                        sendSemanticEvent(virtualViewId, AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUS_CLEARED)
                    }
                    return true
                }
                AccessibilityNodeInfo.ACTION_SET_TEXT -> {
                    val text = arguments?.getCharSequence(
                        AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE,
                    )?.toString() ?: return false
                    if (BubbleNative.a11yAction(virtualViewId, 2, text) != 0) return false
                    afterA11yAction()
                    return true
                }
                AccessibilityNodeInfo.ACTION_SCROLL_FORWARD,
                AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD,
                ACTION_SCROLL_LEFT_ID,
                ACTION_SCROLL_RIGHT_ID,
                ACTION_SCROLL_UP_ID,
                ACTION_SCROLL_DOWN_ID,
                -> {
                    val scroll = when (action) {
                        AccessibilityNodeInfo.ACTION_SCROLL_FORWARD -> 3
                        AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD -> 4
                        ACTION_SCROLL_LEFT_ID -> 5
                        ACTION_SCROLL_RIGHT_ID -> 6
                        ACTION_SCROLL_UP_ID -> 7
                        else -> 8
                    }
                    if (BubbleNative.a11yAction(virtualViewId, scroll, "") != 0) return false
                    afterA11yAction()
                    return true
                }
            }
            return false
        }
    }

    private fun afterA11yAction() {
        afterKey()
    }

    override fun onKeyDown(keyCode: Int, event: android.view.KeyEvent): Boolean {
        val action = when (keyCode) {
            android.view.KeyEvent.KEYCODE_TAB -> if (event.isShiftPressed) 1 else 0
            android.view.KeyEvent.KEYCODE_ENTER,
            android.view.KeyEvent.KEYCODE_DPAD_CENTER,
            -> 2
            android.view.KeyEvent.KEYCODE_SPACE -> 3
            android.view.KeyEvent.KEYCODE_ESCAPE -> 4
            android.view.KeyEvent.KEYCODE_DPAD_LEFT -> 5
            android.view.KeyEvent.KEYCODE_DPAD_RIGHT -> 6
            android.view.KeyEvent.KEYCODE_MOVE_HOME -> 7
            android.view.KeyEvent.KEYCODE_MOVE_END -> 8
            else -> return super.onKeyDown(keyCode, event)
        }
        val handled = BubbleNative.key(action, SystemClock.uptimeMillis().toDouble())
        if (handled > 0) {
            afterKey()
            return true
        }
        if (handled < 0) return true
        return super.onKeyDown(keyCode, event)
    }

    private fun afterKey() {
        refresh()
    }

    private var inputSession = 0L

    private fun updateInputSession() {
        val session = editSnapshot()?.session ?: 0L
        if (session == inputSession) return
        inputSession = session
        val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
        if (session == 0L) {
            imm.hideSoftInputFromWindow(windowToken, 0)
        } else {
            val flags = BubbleNative.fieldFlags(BubbleNative.focused())
            editorType = editorType(flags)
            autofillHints = autofillHints(flags)
            setAutofillHints(*autofillHints)
        }
        imm.restartInput(this)
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
                        val layout = FieldLayout.build(text, textPaint, w, lineHeight, font, align)
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
        drawSelectionHandles(canvas)
    }

    private fun drawSelectionHandles(canvas: Canvas) {
        val handles = BubbleNative.selectionHandles()
        if (handles.size < 8) return
        fill.style = Paint.Style.FILL
        fill.color = Color.argb(242, 77, 163, 255)
        for (index in 0 until 2) {
            val cx = handles[index * 4] + handles[index * 4 + 2] / 2
            val cy = handles[index * 4 + 1] + handles[index * 4 + 3] / 2
            canvas.drawCircle(cx, cy, 7f, fill)
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

    /** Last anchor published to the IME; read by the editing acceptance test. */
    var lastCursorAnchorInfo: CursorAnchorInfo? = null
        private set

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
        lastCursorAnchorInfo = info
    }

    override fun onCheckIsTextEditor(): Boolean = editSnapshot() != null

    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection? {
        val initial = editSnapshot() ?: return null
        outAttrs.inputType = editorType
        outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI
        setAutofillHints(*autofillHints)
        outAttrs.initialSelStart = initial.start
        outAttrs.initialSelEnd = initial.end
        return object : BaseInputConnection(this, true) {
            private val expectedSession = initial.session
            private var batchDepth = 0
            private var closed = false

            private fun current(): EditSnapshot? {
                if (closed) return null
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

            override fun commitText(text: CharSequence, newCursorPosition: Int): Boolean {
                val snapshot = current() ?: return false
                return wrote(
                    BubbleNative.editCommit(
                        snapshot.session,
                        bytes(text),
                        newCursorPosition,
                    ),
                )
            }

            override fun setComposingText(text: CharSequence, newCursorPosition: Int): Boolean {
                val snapshot = current() ?: return false
                return wrote(
                    BubbleNative.editCompose(
                        snapshot.session,
                        bytes(text),
                        newCursorPosition,
                    ),
                )
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
                closed = true
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
                val keyAction = when (event.keyCode) {
                    android.view.KeyEvent.KEYCODE_TAB -> if (event.isShiftPressed) 1 else 0
                    android.view.KeyEvent.KEYCODE_ESCAPE -> 4
                    else -> null
                }
                if (keyAction != null) {
                    val handled = BubbleNative.key(keyAction, event.eventTime.toDouble())
                    if (handled > 0) {
                        afterKey()
                        return true
                    }
                }
                val snapshot = current() ?: return false
                return when (event.keyCode) {
                    android.view.KeyEvent.KEYCODE_DEL -> {
                        if (snapshot.hasComposition || snapshot.start != snapshot.end) {
                            wrote(BubbleNative.editCommit(snapshot.session, ByteArray(0), 1))
                        } else {
                            wrote(BubbleNative.editDeleteSurrounding(snapshot.session, 1, 0))
                        }
                    }
                    android.view.KeyEvent.KEYCODE_FORWARD_DEL -> {
                        if (snapshot.hasComposition || snapshot.start != snapshot.end) {
                            wrote(BubbleNative.editCommit(snapshot.session, ByteArray(0), 1))
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
        if (!engine || activePointers.isEmpty()) return
        val ids = activePointers.toList()
        activePointers.clear()
        for (id in ids) {
            BubbleNative.pointer(3, id, 0f, 0f, timeMs, 0, 0)
        }
        refresh()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (engine && !hasFocus) cancelActivePointers(SystemClock.uptimeMillis().toDouble())
    }

    override fun onDetachedFromWindow() {
        Choreographer.getInstance().removeFrameCallback(frameCallback)
        if (engine) cancelActivePointers(SystemClock.uptimeMillis().toDouble())
        super.onDetachedFromWindow()
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                val x = localX(event.x)
                val y = localY(event.y)
                val handles = BubbleNative.selectionHandles()
                if (handles.size >= 8) {
                    val startHit = hypot(x - (handles[0] + handles[2] / 2), y - (handles[1] + handles[3] / 2)) <= 16f
                    val endHit = hypot(x - (handles[4] + handles[6] / 2), y - (handles[5] + handles[7] / 2)) <= 16f
                    if (startHit || endHit) {
                        val selection = BubbleNative.selection()
                        if (selection.size >= 2) {
                            val anchor = if (startHit) selection[1] else selection[0]
                            selectionDrag = event.getPointerId(0) to anchor
                            return true
                        }
                    }
                }
                sendPointer(0, event.actionIndex, event)
            }
            MotionEvent.ACTION_POINTER_DOWN -> sendPointer(0, event.actionIndex, event)
            MotionEvent.ACTION_MOVE -> {
                val drag = selectionDrag
                if (drag != null && drag.first == event.getPointerId(0)) {
                    val unit = BubbleNative.unitAt(localX(event.x), localY(event.y))
                    val snapshot = editSnapshot()
                    if (unit >= 0 && snapshot != null) {
                        BubbleNative.editSelect(snapshot.session, drag.second, unit)
                        refresh()
                    }
                    return true
                }
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
            MotionEvent.ACTION_POINTER_UP -> {
                if (selectionDrag?.first == event.getPointerId(event.actionIndex)) selectionDrag = null
                sendPointer(2, event.actionIndex, event)
            }
            MotionEvent.ACTION_UP -> {
                if (selectionDrag?.first == event.getPointerId(event.actionIndex)) {
                    selectionDrag = null
                    return true
                }
                sendPointer(2, event.actionIndex, event)
                val handle = BubbleNative.focusAt(localX(event.x), localY(event.y))
                if (handle != 0) {
                    refresh()
                    importantForAutofill = IMPORTANT_FOR_AUTOFILL_YES
                    requestFocus()
                    val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
                    imm.showSoftInput(this, InputMethodManager.SHOW_IMPLICIT)
                }
            }
            MotionEvent.ACTION_CANCEL -> {
                selectionDrag = null
                cancelActivePointers(event.eventTime.toDouble())
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

    companion object {
        @JvmStatic
        fun layoutSnapshot(
            text: ByteArray,
            width: Float,
            fontSize: Float,
            lineHeight: Float,
            density: Float,
            weight: Int,
            align: Int,
            revision: Long,
        ): ByteArray {
            val value = String(text, Charsets.UTF_8)
            val paint = FieldLayout.paint(fontSize, weight)
            val layout = FieldLayout.build(value, paint, width, lineHeight, fontSize, align)
            return FieldLayout.snapshot(value, layout, width, fontSize, lineHeight, density, weight, align, revision)
        }
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
