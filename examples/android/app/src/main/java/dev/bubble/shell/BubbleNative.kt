package dev.bubble.shell

object BubbleNative {
    init {
        System.loadLibrary("bubble_ffi")
    }

    external fun create(): Int
    external fun destroy()
    external fun load(manifest: String, entry: ByteArray): Int
    external fun frame(width: Float, height: Float): ByteArray
    external fun pointerDown(x: Float, y: Float)
    external fun scroll(x: Float, y: Float, dy: Float)
    external fun setVisible(visible: Int)
    external fun registerFont(family: String, data: ByteArray)
    external fun fontEpoch(): Int
}
