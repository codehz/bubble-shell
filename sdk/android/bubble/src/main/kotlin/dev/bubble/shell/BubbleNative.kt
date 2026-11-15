package dev.bubble.shell

object BubbleNative {
    init {
        System.loadLibrary("bubble_ffi")
    }

    external fun create(): Long
    external fun destroy(handle: Long)
    external fun load(handle: Long, manifest: String, entry: ByteArray): String?
    external fun setParams(handle: Long, json: String): String?
    external fun setStorageDir(handle: Long, path: String?): String?
    external fun frame(handle: Long, width: Float, height: Float): ByteArray
    external fun pointer(handle: Long, action: Int, id: Int, x: Float, y: Float, timeMs: Double, pointerKind: Int, buttons: Int): Int
    external fun visualPointer(handle: Long, action: Int, id: Int, x: Float, y: Float, timeMs: Double, pointerKind: Int, buttons: Int): Int
    external fun tick(handle: Long, timeMs: Double): Int
    external fun scroll(handle: Long, x: Float, y: Float, dx: Float, dy: Float)
    external fun edit(handle: Long, action: Int, text: ByteArray, a: Int, b: Int): Int
    external fun editBatchBegin(handle: Long, session: Long): Int
    external fun editBatchEnd(handle: Long, session: Long): Int
    external fun editBatchAbort(handle: Long, session: Long): Int
    external fun editSnapshot(handle: Long): LongArray
    external fun editCommit(handle: Long, session: Long, text: ByteArray, newCursorPosition: Int): Int
    external fun editCompose(handle: Long, session: Long, text: ByteArray, newCursorPosition: Int): Int
    external fun editUndo(handle: Long, session: Long): Int
    external fun editRedo(handle: Long, session: Long): Int
    external fun editDeleteSurrounding(handle: Long, session: Long, before: Int, after: Int): Int
    external fun editDeleteSurroundingCodePoints(handle: Long, session: Long, before: Int, after: Int): Int
    external fun editSetComposition(handle: Long, session: Long, start: Int, end: Int): Int
    external fun editFinishComposition(handle: Long, session: Long): Int
    external fun editSelect(handle: Long, session: Long, anchor: Int, active: Int): Int
    external fun fieldAt(handle: Long, x: Float, y: Float): Int
    external fun unitAt(handle: Long, x: Float, y: Float): Int
    external fun focusAt(handle: Long, x: Float, y: Float): Int
    external fun fieldFlags(handle: Long, node: Int): Int
    external fun focused(handle: Long): Int
    external fun key(handle: Long, action: Int, timeMs: Double): Int
    external fun semantics(handle: Long): String
    external fun a11yAction(handle: Long, node: Int, action: Int, text: String): Int
    external fun editMenu(handle: Long, action: Int, clip: String): String
    external fun selectionHandles(handle: Long): FloatArray
    external fun focusedText(handle: Long): String
    external fun selection(handle: Long): IntArray
    external fun caret(handle: Long): FloatArray
    external fun scrollXOf(handle: Long, node: Int): Float
    external fun scrollYOf(handle: Long, node: Int): Float
    external fun setVisible(handle: Long, visible: Int)
    external fun registerFont(handle: Long, family: String, data: ByteArray)
    external fun nodeText(handle: Long, node: Int): String
    external fun nodeBox(handle: Long, node: Int): FloatArray
    external fun postEval(handle: Long, source: String)
    external fun faults(handle: Long): Int
    external fun generation(handle: Long): Int
    external fun fontEpoch(handle: Long): Int
    external fun pollRequest(handle: Long): String?
    external fun finishRequest(handle: Long, id: Int, ok: Int, result: String)
    external fun hasBackHandler(handle: Long): Int
    external fun send(handle: Long, name: String, payload: String): String?
}
