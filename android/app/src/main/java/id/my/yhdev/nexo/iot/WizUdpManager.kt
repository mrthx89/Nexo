package id.my.yhdev.nexo.iot

import android.content.Context
import android.net.wifi.WifiManager
import android.util.Log
import com.google.gson.Gson
import com.google.gson.reflect.TypeToken
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress

data class DiscoveredBulb(
    val ip: String,
    val mac: String,
    var name: String = "Philips WiZ",
    var state: Boolean = true,
    var dimming: Int = 100,
    var temp: Int? = null,
    var r: Int? = null,
    var g: Int? = null,
    var b: Int? = null,
    var sceneId: Int? = null,
    var lastSeenMs: Long = System.currentTimeMillis()
)

class WizUdpManager(private val context: Context) {

    companion object {
        private const val TAG = "WizUdpManager"
        const val WIZ_PORT = 38899
        private const val PREFS_NAME = "nexo_wiz_bulbs"
        private const val KEY_CACHED_BULBS = "cached_bulbs_json"
    }

    private val gson = Gson()
    private val wifiManager = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager

    fun getCachedBulbs(): List<DiscoveredBulb> {
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        val json = prefs.getString(KEY_CACHED_BULBS, null) ?: return emptyList()
        return try {
            val type = object : TypeToken<List<DiscoveredBulb>>() {}.type
            gson.fromJson(json, type) ?: emptyList()
        } catch (e: Exception) {
            emptyList()
        }
    }

    fun saveBulbs(bulbs: List<DiscoveredBulb>) {
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        val json = gson.toJson(bulbs)
        prefs.edit().putString(KEY_CACHED_BULBS, json).apply()
    }

    suspend fun scanBulbs(timeoutMs: Int = 2500): List<DiscoveredBulb> = withContext(Dispatchers.IO) {
        val discovered = mutableMapOf<String, DiscoveredBulb>()
        getCachedBulbs().forEach { discovered[it.ip] = it }

        var multicastLock: WifiManager.MulticastLock? = null
        var socket: DatagramSocket? = null

        try {
            multicastLock = wifiManager?.createMulticastLock("nexo_wiz_scan")?.apply {
                setReferenceCounted(true)
                acquire()
            }

            socket = DatagramSocket().apply {
                broadcast = true
                soTimeout = timeoutMs
            }

            // 1. Send UDP Broadcast getSystemConfig probe
            val probe = JSONObject().apply {
                put("method", "getSystemConfig")
                put("params", JSONObject())
            }.toString().toByteArray(Charsets.UTF_8)

            val broadcastAddr = InetAddress.getByName("255.255.255.255")
            val packet = DatagramPacket(probe, probe.size, broadcastAddr, WIZ_PORT)
            socket.send(packet)

            val buffer = ByteArray(2048)
            val startTime = System.currentTimeMillis()

            while (System.currentTimeMillis() - startTime < timeoutMs) {
                try {
                    val recvPacket = DatagramPacket(buffer, buffer.size)
                    socket.receive(recvPacket)
                    val ip = recvPacket.address.hostAddress ?: continue
                    val text = String(recvPacket.data, 0, recvPacket.length, Charsets.UTF_8)
                    val json = runCatching { JSONObject(text) }.getOrNull() ?: continue
                    val result = json.optJSONObject("result") ?: continue
                    val mac = result.optString("mac", "")

                    val bulb = discovered[ip] ?: DiscoveredBulb(
                        ip = ip,
                        mac = mac,
                        name = if (mac.length >= 4) "Philips WiZ (${mac.takeLast(4).uppercase()})" else "Philips WiZ ($ip)"
                    )
                    bulb.lastSeenMs = System.currentTimeMillis()
                    discovered[ip] = bulb
                } catch (e: java.net.SocketTimeoutException) {
                    break
                } catch (e: Exception) {
                    Log.w(TAG, "Error receiving WiZ probe packet: ${e.message}")
                    break
                }
            }
        } catch (e: Exception) {
            Log.e(TAG, "WiZ discovery broadcast failed", e)
        } finally {
            socket?.close()
            try {
                if (multicastLock?.isHeld == true) multicastLock.release()
            } catch (ignored: Exception) {}
        }

        // 2. Query initial pilot state for discovered bulbs
        val resultList = discovered.values.toList()
        resultList.forEach { bulb ->
            queryBulbPilot(bulb)
        }

        saveBulbs(resultList)
        resultList
    }

    suspend fun queryBulbPilot(bulb: DiscoveredBulb) = withContext(Dispatchers.IO) {
        var socket: DatagramSocket? = null
        try {
            socket = DatagramSocket().apply { soTimeout = 1000 }
            val query = JSONObject().apply {
                put("method", "getPilot")
                put("params", JSONObject())
            }.toString().toByteArray(Charsets.UTF_8)

            val targetAddr = InetAddress.getByName(bulb.ip)
            socket.send(DatagramPacket(query, query.size, targetAddr, WIZ_PORT))

            val buffer = ByteArray(1024)
            val recvPacket = DatagramPacket(buffer, buffer.size)
            socket.receive(recvPacket)
            val text = String(recvPacket.data, 0, recvPacket.length, Charsets.UTF_8)
            val json = JSONObject(text)
            val result = json.optJSONObject("result")
            if (result != null) {
                bulb.state = result.optBoolean("state", true)
                bulb.dimming = result.optInt("dimming", 100)
                if (result.has("temp")) bulb.temp = result.optInt("temp")
                if (result.has("r")) bulb.r = result.optInt("r")
                if (result.has("g")) bulb.g = result.optInt("g")
                if (result.has("b")) bulb.b = result.optInt("b")
                if (result.has("sceneId")) bulb.sceneId = result.optInt("sceneId")
            }
        } catch (ignored: Exception) {
        } finally {
            socket?.close()
        }
    }

    suspend fun sendPilot(ip: String, params: JSONObject): Boolean = withContext(Dispatchers.IO) {
        var socket: DatagramSocket? = null
        try {
            socket = DatagramSocket().apply { soTimeout = 1200 }
            val cmd = JSONObject().apply {
                put("method", "setPilot")
                put("env", "pro")
                put("params", params)
            }.toString().toByteArray(Charsets.UTF_8)

            val targetAddr = InetAddress.getByName(ip)
            socket.send(DatagramPacket(cmd, cmd.size, targetAddr, WIZ_PORT))
            true
        } catch (e: Exception) {
            Log.e(TAG, "Failed to send WiZ pilot to $ip", e)
            false
        } finally {
            socket?.close()
        }
    }

    suspend fun setPower(ip: String, state: Boolean): Boolean {
        val params = JSONObject().apply { put("state", state) }
        return sendPilot(ip, params)
    }

    suspend fun setDimming(ip: String, dimming: Int): Boolean {
        val params = JSONObject().apply {
            put("state", true)
            put("dimming", dimming.coerceIn(10, 100))
        }
        return sendPilot(ip, params)
    }

    suspend fun setKelvin(ip: String, kelvin: Int): Boolean {
        val params = JSONObject().apply {
            put("state", true)
            put("temp", kelvin.coerceIn(2200, 6500))
        }
        return sendPilot(ip, params)
    }

    suspend fun setRgb(ip: String, r: Int, g: Int, b: Int): Boolean {
        val params = JSONObject().apply {
            put("state", true)
            put("r", r.coerceIn(0, 255))
            put("g", g.coerceIn(0, 255))
            put("b", b.coerceIn(0, 255))
        }
        return sendPilot(ip, params)
    }

    suspend fun setScene(ip: String, sceneId: Int): Boolean {
        val params = JSONObject().apply {
            put("state", true)
            put("sceneId", sceneId)
        }
        return sendPilot(ip, params)
    }
}
