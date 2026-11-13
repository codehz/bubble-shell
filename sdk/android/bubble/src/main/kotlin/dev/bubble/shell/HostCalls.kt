package dev.bubble.shell

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.widget.Toast
import org.json.JSONArray
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

internal fun drainHostCalls(context: Context) {
    while (true) {
        val raw = BubbleNative.pollRequest() ?: return
        val row = JSONArray(raw)
        val id = row.getInt(0)
        val method = row.getString(1)
        val params = if (row.isNull(2)) JSONObject() else JSONObject(row.getString(2))
        if (method == "fetch") {
            Thread {
                try {
                    val connection = URL(params.optString("url")).openConnection() as HttpURLConnection
                    val verb = params.optString("method")
                    if (verb.isNotEmpty()) connection.requestMethod = verb
                    val body = params.optString("body")
                    if (body.isNotEmpty()) {
                        connection.doOutput = true
                        connection.outputStream.use { it.write(body.toByteArray()) }
                    }
                    val text = connection.inputStream.bufferedReader().readText()
                    BubbleNative.finishRequest(
                        id,
                        1,
                        JSONObject().put("status", connection.responseCode).put("text", text).toString(),
                    )
                } catch (error: Exception) {
                    BubbleNative.finishRequest(id, 0, error.message ?: "fetch failed")
                }
            }.start()
            continue
        }
        try {
            val result = when (method) {
                "toast" -> {
                    Toast.makeText(context, params.optString("message"), Toast.LENGTH_SHORT).show()
                    "null"
                }
                "openURL" -> {
                    val intent = Intent(Intent.ACTION_VIEW, Uri.parse(params.optString("url")))
                    intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    context.startActivity(intent)
                    "null"
                }
                "clipboard.readText" -> {
                    val manager = context.getSystemService(ClipboardManager::class.java)
                    JSONObject.quote(manager?.primaryClip?.getItemAt(0)?.text?.toString() ?: "")
                }
                "clipboard.writeText" -> {
                    val manager = context.getSystemService(ClipboardManager::class.java)
                    manager?.setPrimaryClip(ClipData.newPlainText("bubble", params.optString("text")))
                    "null"
                }
                else -> {
                    BubbleNative.finishRequest(id, 0, "no host handler for $method")
                    continue
                }
            }
            BubbleNative.finishRequest(id, 1, result)
        } catch (error: Exception) {
            BubbleNative.finishRequest(id, 0, error.message ?: "host call failed")
        }
    }
}
