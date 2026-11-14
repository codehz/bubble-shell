package dev.bubble.shell

import android.app.Activity
import android.os.Bundle

class MainActivity : Activity() {
    private lateinit var shell: BubbleView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        shell = BubbleView(this)
        val font = assets.open("HostFont.ttf").use { it.readBytes() }
        val devUrl = intent?.getStringExtra("bubble_dev")
        if (devUrl != null) {
            shell.open(filesDir, font, null, null, devUrl)
        } else {
            val requested = intent?.getStringExtra("bubble_fixture") ?: "counter"
            val fixture = if (requested.matches(Regex("[a-z0-9-]+"))) requested else "counter"
            val manifest = assets.open("fixtures/$fixture.json").bufferedReader().readText()
            val entry = assets.open("fixtures/$fixture.js").readBytes()
            shell.open(filesDir, font, manifest, entry, null)
        }
        setContentView(shell)
    }

    override fun onDestroy() {
        if (::shell.isInitialized) shell.destroyEngine()
        super.onDestroy()
    }
}
