package dev.zimaconnect

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject

/** Bridge to the Rust node (core/zima-connect-android). All calls block: use [Api]. */
object Native {
    init {
        System.loadLibrary("zima_connect_android")
    }

    @JvmStatic external fun call(method: String, args: String): String

    @JvmStatic external fun nextEvent(timeoutMs: Long): String
}

class ApiException(message: String) : Exception(message)

object Api {
    /** Runs one call on a background thread. Returns the `ok` value (JSONObject/JSONArray/String/…/null). */
    suspend fun call(method: String, args: JSONObject = JSONObject()): Any? =
        withContext(Dispatchers.IO) {
            val r = JSONObject(Native.call(method, args.toString()))
            if (r.has("err")) throw ApiException(r.getString("err"))
            val ok = r.opt("ok")
            if (ok == JSONObject.NULL) null else ok
        }

    fun args(vararg pairs: Pair<String, Any?>): JSONObject =
        JSONObject().also { o -> pairs.forEach { (k, v) -> o.put(k, v ?: JSONObject.NULL) } }
}
