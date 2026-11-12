package dev.bubble.shell

object BubbleNative {
    init {
        System.loadLibrary("bubble_ffi")
    }

    external fun create(): Int
    external fun destroy()
    external fun load(manifest: String, entry: ByteArray): Int
    external fun frame(width: Float, height: Float): ByteArray
    external fun pointer(action: Int, id: Int, x: Float, y: Float, timeMs: Double, pointerKind: Int, buttons: Int): Int
    external fun tick(timeMs: Double): Int
    external fun scroll(x: Float, y: Float, dx: Float, dy: Float)
    external fun edit(action: Int, text: ByteArray, a: Int, b: Int): Int
    external fun fieldAt(x: Float, y: Float): Int
    external fun focusAt(x: Float, y: Float): Int
    external fun fieldFlags(handle: Int): Int
    external fun focused(): Int
    external fun focusedText(): String
    external fun selection(): IntArray
    external fun scrollXOf(handle: Int): Float
    external fun scrollYOf(handle: Int): Float
    external fun setVisible(visible: Int)
    external fun registerFont(family: String, data: ByteArray)
    external fun nodeText(handle: Int): String
    external fun nodeBox(handle: Int): FloatArray
    external fun postEval(source: String)
    external fun faults(): Int
    external fun generation(): Int
    external fun fontEpoch(): Int
}
