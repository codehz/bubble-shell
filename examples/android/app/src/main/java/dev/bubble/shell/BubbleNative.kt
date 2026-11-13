package dev.bubble.shell

object BubbleNative {
    init {
        System.loadLibrary("bubble_ffi")
    }

    external fun create(): Int
    external fun destroy()
    external fun load(manifest: String, entry: ByteArray): Int
    external fun setStorageDir(path: String): Int
    external fun frame(width: Float, height: Float): ByteArray
    external fun pointer(action: Int, id: Int, x: Float, y: Float, timeMs: Double, pointerKind: Int, buttons: Int): Int
    external fun tick(timeMs: Double): Int
    external fun scroll(x: Float, y: Float, dx: Float, dy: Float)
    external fun edit(action: Int, text: ByteArray, a: Int, b: Int): Int
    external fun editBatchBegin(session: Long): Int
    external fun editBatchEnd(session: Long): Int
    external fun editBatchAbort(session: Long): Int
    external fun editSnapshot(): LongArray
    external fun editCommit(session: Long, text: ByteArray, newCursorPosition: Int): Int
    external fun editCompose(session: Long, text: ByteArray, newCursorPosition: Int): Int
    external fun editUndo(session: Long): Int
    external fun editRedo(session: Long): Int
    external fun editDeleteSurrounding(session: Long, before: Int, after: Int): Int
    external fun editDeleteSurroundingCodePoints(session: Long, before: Int, after: Int): Int
    external fun editSetComposition(session: Long, start: Int, end: Int): Int
    external fun editFinishComposition(session: Long): Int
    external fun editSelect(session: Long, anchor: Int, active: Int): Int
    external fun fieldAt(x: Float, y: Float): Int
    external fun unitAt(x: Float, y: Float): Int
    external fun focusAt(x: Float, y: Float): Int
    external fun fieldFlags(handle: Int): Int
    external fun focused(): Int
    external fun key(action: Int, timeMs: Double): Int
    external fun semantics(): String
    external fun a11yAction(handle: Int, action: Int, text: String): Int
    external fun editMenu(action: Int, clip: String): String
    external fun selectionHandles(): FloatArray
    external fun focusedText(): String
    external fun selection(): IntArray
    external fun caret(): FloatArray
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
    external fun pollRequest(): String?
    external fun finishRequest(id: Int, ok: Int, result: String)
}
