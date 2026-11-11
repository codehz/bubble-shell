package dev.bubble.shell

import android.app.Activity
import android.graphics.Typeface
import android.os.Bundle
import android.view.Choreographer

class MainActivity : Activity() {
    private lateinit var shell: BubbleView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        check(BubbleNative.create() == 0)
        assets.open("HostFont.ttf").use { input ->
            val bytes = input.readBytes()
            BubbleNative.registerFont("HostFont", bytes)
            val file = java.io.File(cacheDir, "HostFont.ttf")
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
            if (!isDestroyed) {
                shell.refresh()
                Choreographer.getInstance().postFrameCallback(this)
            }
        }
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
