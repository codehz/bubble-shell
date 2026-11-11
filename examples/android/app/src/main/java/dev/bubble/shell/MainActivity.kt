package dev.bubble.shell

import android.app.Activity
import android.graphics.Typeface
import android.os.Bundle
import android.util.Log
import android.view.Choreographer
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

class MainActivity : Activity() {
    private lateinit var shell: BubbleView
    private var frames = 0
    private var frameMs = 0.0
    private var last = System.nanoTime()
    private var phase = 0
    private var hangFrames = 0
    private val started = System.nanoTime()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        check(BubbleNative.create() == 0)
        assets.open("HostFont.ttf").use { input ->
            val bytes = input.readBytes()
            BubbleNative.registerFont("HostFont", bytes)
            val file = File(cacheDir, "HostFont.ttf")
            file.writeBytes(bytes)
            shellTypeface = Typeface.createFromFile(file)
        }
        val manifest = assets.open("manifest.json").bufferedReader().readText()
        val entry = assets.open("main.js").readBytes()
        check(BubbleNative.load(manifest, entry) == 0)
        shell = BubbleView(this)
        shell.setHostTypeface(shellTypeface ?: Typeface.DEFAULT)
        setContentView(shell)
        Choreographer.getInstance().postFrameCallback(frame)
    }

    private val frame = object : Choreographer.FrameCallback {
        override fun doFrame(frameTimeNanos: Long) {
            if (isDestroyed) return
            val now = System.nanoTime()
            val dt = (now - last) / 1_000_000.0
            last = now
            shell.refresh()
            frames += 1
            if (frames > 5) frameMs += dt
            when (phase) {
                0 -> if (frames == 20) {
                    val box = BubbleNative.nodeBox(4)
                    BubbleNative.pointerDown(box[0] + box[2] / 2f, box[1] + box[3] / 2f)
                    phase = 1
                }
                1 -> if (BubbleNative.nodeText(3) == "Count: 1") {
                    BubbleNative.postEval("while (true) {}")
                    hangFrames = 0
                    phase = 2
                }
                2 -> {
                    hangFrames += 1
                    if (BubbleNative.faults() > 0 && BubbleNative.nodeText(3) == "Count: 0") {
                        report()
                        phase = 3
                    }
                }
            }
            if (phase != 3 && now - started > 4_000_000_000L) {
                report()
                phase = 3
            }
            if (phase != 3) Choreographer.getInstance().postFrameCallback(this)
        }
    }

    private fun report() {
        val samples = (frames - 5).coerceAtLeast(1)
        val average = frameMs / samples
        val text = BubbleNative.nodeText(3)
        val checks = JSONArray()
        fun add(name: String, ok: Boolean, detail: String) {
            checks.put(JSONObject().put("name", name).put("ok", ok).put("detail", detail))
        }
        add("remount text", text == "Count: 0", text)
        add("faults", BubbleNative.faults() > 0, BubbleNative.faults().toString())
        add("frames during hang", hangFrames >= 10, hangFrames.toString())
        add("frame budget", average < 20.0, "%.2f".format(average))
        add("font epoch", BubbleNative.fontEpoch() > 0, BubbleNative.fontEpoch().toString())
        val ok = (0 until checks.length()).all { checks.getJSONObject(it).getBoolean("ok") }
        val json = JSONObject().put("suite", "m1.android").put("ok", ok).put("checks", checks).toString()
        File(filesDir, "m1-result.json").writeText(json)
        Log.i("BUBBLE_EVAL", json)
    }

    override fun onResume() {
        super.onResume()
        BubbleNative.setVisible(1)
    }

    override fun onPause() {
        BubbleNative.setVisible(0)
        super.onPause()
    }

    override fun onDestroy() {
        BubbleNative.destroy()
        super.onDestroy()
    }

    companion object {
        var shellTypeface: Typeface? = null
    }
}
