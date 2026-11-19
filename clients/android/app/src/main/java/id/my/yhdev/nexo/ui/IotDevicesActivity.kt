package id.my.yhdev.nexo.ui

import android.content.Context
import android.os.Bundle
import android.text.InputType
import android.view.LayoutInflater
import android.view.View
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.SeekBar
import android.widget.TextView
import android.widget.Toast
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import androidx.core.widget.NestedScrollView
import androidx.lifecycle.lifecycleScope
import com.google.android.material.button.MaterialButton
import com.google.android.material.switchmaterial.SwitchMaterial
import com.google.gson.Gson
import com.google.gson.JsonParser
import id.my.yhdev.nexo.R
import id.my.yhdev.nexo.iot.DiscoveredBulb
import id.my.yhdev.nexo.iot.WizUdpManager
import id.my.yhdev.nexo.security.DeviceCredentialStore
import kotlinx.coroutines.launch
import okhttp3.Call
import okhttp3.Callback
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import java.io.IOException
import java.util.concurrent.TimeUnit

class IotDevicesActivity : AppCompatActivity() {

    private lateinit var wizManager: WizUdpManager
    private val bulbsList = mutableListOf<DiscoveredBulb>()

    private lateinit var tvBulbCount: TextView
    private lateinit var pbScanning: ProgressBar
    private lateinit var ivSummaryLed: ImageView
    private lateinit var layoutBulbsContainer: LinearLayout
    private lateinit var layoutEmptyState: LinearLayout
    private lateinit var btnScanWifi: MaterialButton

    // Polytron AC State
    private var isAcAuthenticated: Boolean = false
    private var acPower: Boolean = false
    private var acTargetTemp: Int = 22
    private var acMode: String = "cool"
    private var acFanSpeed: String = "auto"
    private var acSwing: Boolean = false
    private var acPowerWatt: Double = 0.0
    private var acRoomTemp: Double = 26.0
    private var acEnergyKwh: Double = 0.0

    // Polytron AC Views
    private var layoutAcAuthWarning: View? = null
    private var tvAcAuthStatus: TextView? = null
    private var btnAcAuthLogin: MaterialButton? = null
    private var switchAcPower: SwitchMaterial? = null
    private var tvAcPowerWatt: TextView? = null
    private var tvAcRoomTemp: TextView? = null
    private var tvAcEnergy: TextView? = null
    private var tvAcTargetTemp: TextView? = null
    private var btnAcModeCool: MaterialButton? = null
    private var btnAcModeDry: MaterialButton? = null
    private var btnAcModeFan: MaterialButton? = null
    private var btnAcModeAuto: MaterialButton? = null
    private var btnAcFanSpeed: MaterialButton? = null
    private var btnAcSwingToggle: MaterialButton? = null

    // Server Networking & Credentials
    private var serverUrl: String = "https://nexo.yhdev.my.id"
    private var deviceId: String = ""
    private var deviceToken: String = ""
    private val httpClient = OkHttpClient.Builder()
        .connectTimeout(10, TimeUnit.SECONDS)
        .readTimeout(15, TimeUnit.SECONDS)
        .build()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_iot_devices)

        wizManager = WizUdpManager(this)

        initViews()
        initAcView()
        loadCachedBulbs()
        startWifiScan()
        handleFocusIntent()
    }

    override fun onNewIntent(intent: Intent?) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleFocusIntent()
    }

    override fun onResume() {
        super.onResume()
        fetchAcLiveStatus()
    }

    private fun handleFocusIntent() {
        val focus = intent.getStringExtra("EXTRA_FOCUS") ?: "all"
        val scrollView = findViewById<NestedScrollView>(R.id.scrollBulbs) ?: return
        scrollView.post {
            when (focus) {
                "ac" -> {
                    val acCard = findViewById<View>(R.id.cardPolytronAc)
                    if (acCard != null) {
                        scrollView.smoothScrollTo(0, acCard.top)
                    }
                }
                "lighting" -> {
                    val bulbsContainer = findViewById<View>(R.id.layoutBulbsContainer)
                    if (bulbsContainer != null) {
                        scrollView.smoothScrollTo(0, bulbsContainer.top)
                    }
                }
            }
        }
    }

    private fun initViews() {
        findViewById<View>(R.id.btnBack).setOnClickListener {
            finish()
        }

        tvBulbCount = findViewById(R.id.tvBulbCount)
        pbScanning = findViewById(R.id.pbScanning)
        ivSummaryLed = findViewById(R.id.ivSummaryLed)
        layoutBulbsContainer = findViewById(R.id.layoutBulbsContainer)
        layoutEmptyState = findViewById(R.id.layoutEmptyState)
        btnScanWifi = findViewById(R.id.btnScanWifi)

        btnScanWifi.setOnClickListener {
            startWifiScan()
        }

        findViewById<View>(R.id.btnEmptyScan).setOnClickListener {
            startWifiScan()
        }

        // Global Batch Action Buttons
        findViewById<View>(R.id.btnAllOn).setOnClickListener {
            lifecycleScope.launch {
                bulbsList.forEach { bulb ->
                    bulb.state = true
                    wizManager.setPower(bulb.ip, true)
                }
                renderBulbs()
            }
        }

        findViewById<View>(R.id.btnAllOff).setOnClickListener {
            lifecycleScope.launch {
                bulbsList.forEach { bulb ->
                    bulb.state = false
                    wizManager.setPower(bulb.ip, false)
                }
                renderBulbs()
            }
        }

        findViewById<View>(R.id.btnModeFocus).setOnClickListener {
            lifecycleScope.launch {
                bulbsList.forEach { bulb ->
                    bulb.state = true
                    bulb.dimming = 100
                    bulb.temp = 6500
                    wizManager.setKelvin(bulb.ip, 6500)
                }
                renderBulbs()
            }
        }

        findViewById<View>(R.id.btnModeRelax).setOnClickListener {
            lifecycleScope.launch {
                bulbsList.forEach { bulb ->
                    bulb.state = true
                    bulb.dimming = 60
                    bulb.temp = 2700
                    wizManager.setKelvin(bulb.ip, 2700)
                }
                renderBulbs()
            }
        }
    }

    private fun initAcView() {
        val prefs = getSharedPreferences("nexo_voice_prefs", Context.MODE_PRIVATE)
        serverUrl = intent.getStringExtra("EXTRA_SERVER_URL")
            ?: prefs.getString("server_url", null)
            ?: "https://nexo.yhdev.my.id"
        deviceId = intent.getStringExtra("EXTRA_DEVICE_ID")
            ?: prefs.getString("device_id", null)
            ?: ""
        val wssUrl = if (serverUrl.startsWith("http://")) {
            serverUrl.replaceFirst("http://", "ws://")
        } else if (serverUrl.startsWith("https://")) {
            serverUrl.replaceFirst("https://", "wss://")
        } else {
            serverUrl
        }
        deviceToken = DeviceCredentialStore(this).read(wssUrl, deviceId) ?: ""

        layoutAcAuthWarning = findViewById(R.id.layoutAcAuthWarning)
        tvAcAuthStatus = findViewById(R.id.tvAcAuthStatus)
        btnAcAuthLogin = findViewById(R.id.btnAcAuthLogin)

        btnAcAuthLogin?.setOnClickListener {
            showPolytronAuthDialog()
        }

        switchAcPower = findViewById(R.id.switchAcPower)
        tvAcPowerWatt = findViewById(R.id.tvAcPowerWatt)
        tvAcRoomTemp = findViewById(R.id.tvAcRoomTemp)
        tvAcEnergy = findViewById(R.id.tvAcEnergy)
        tvAcTargetTemp = findViewById(R.id.tvAcTargetTemp)
        btnAcModeCool = findViewById(R.id.btnAcModeCool)
        btnAcModeDry = findViewById(R.id.btnAcModeDry)
        btnAcModeFan = findViewById(R.id.btnAcModeFan)
        btnAcModeAuto = findViewById(R.id.btnAcModeAuto)
        btnAcFanSpeed = findViewById(R.id.btnAcFanSpeed)
        btnAcSwingToggle = findViewById(R.id.btnAcSwingToggle)

        updateAcDisplay()

        switchAcPower?.setOnCheckedChangeListener { _, isChecked ->
            if (acPower != isChecked) {
                acPower = isChecked
                updateAcDisplay()
                sendAcControl(if (isChecked) "ac_on" else "ac_off")
            }
        }

        findViewById<View>(R.id.btnAcTempDown)?.setOnClickListener {
            acTargetTemp = (acTargetTemp - 1).coerceAtLeast(16)
            updateAcDisplay()
            sendAcControl("ac_temp", mapOf("temp" to acTargetTemp))
        }

        findViewById<View>(R.id.btnAcTempUp)?.setOnClickListener {
            acTargetTemp = (acTargetTemp + 1).coerceAtMost(32)
            updateAcDisplay()
            sendAcControl("ac_temp", mapOf("temp" to acTargetTemp))
        }

        btnAcModeCool?.setOnClickListener { setAcMode("cool") }
        btnAcModeDry?.setOnClickListener { setAcMode("dry") }
        btnAcModeFan?.setOnClickListener { setAcMode("fan") }
        btnAcModeAuto?.setOnClickListener { setAcMode("auto") }

        btnAcFanSpeed?.setOnClickListener {
            acFanSpeed = when (acFanSpeed) {
                "auto" -> "low"
                "low" -> "medium"
                "medium" -> "high"
                else -> "auto"
            }
            updateAcDisplay()
            sendAcControl("ac_fan_speed", mapOf("fan_speed" to acFanSpeed))
        }

        btnAcSwingToggle?.setOnClickListener {
            acSwing = !acSwing
            updateAcDisplay()
            sendAcControl("ac_swing", mapOf("swing" to acSwing))
        }

        fetchAcLiveStatus()
    }

    private fun setAcMode(mode: String) {
        acMode = mode
        updateAcDisplay()
        sendAcControl("ac_mode", mapOf("mode" to mode))
    }

    private fun updateAcDisplay() {
        tvAcTargetTemp?.text = "${acTargetTemp}°C"
        switchAcPower?.isChecked = acPower

        if (!isAcAuthenticated) {
            layoutAcAuthWarning?.visibility = View.VISIBLE
            tvAcPowerWatt?.text = "Belum Login"
            tvAcRoomTemp?.text = "-- °C"
            tvAcEnergy?.text = "-- kWh"
        } else {
            layoutAcAuthWarning?.visibility = View.GONE
            if (acPower) {
                val wattStr = if (acPowerWatt > 0) "${acPowerWatt.toInt()} W" else "Aktif"
                tvAcPowerWatt?.text = "⚡ $wattStr"
            } else {
                tvAcPowerWatt?.text = "Standby (0 W)"
            }
            tvAcRoomTemp?.text = if (acRoomTemp > 0) "${String.format("%.1f", acRoomTemp)}°C" else "-- °C"
            tvAcEnergy?.text = if (acEnergyKwh > 0) "${String.format("%.2f", acEnergyKwh)} kWh" else "0.00 kWh"
        }

        btnAcFanSpeed?.text = "Fan: ${acFanSpeed.replaceFirstChar { it.uppercase() }}"
        btnAcSwingToggle?.text = if (acSwing) "Swing: Aktif" else "Louver Swing"
        btnAcSwingToggle?.setTextColor(if (acSwing) 0xFF00E5FF.toInt() else 0xFFE0E0E0.toInt())

        val cyanColor = 0xFF00E5FF.toInt()
        val mutedColor = 0xFF9E9E9E.toInt()
        val activeBg = 0xFF0E2333.toInt()
        val inactiveBg = 0xFF141E2B.toInt()

        btnAcModeCool?.let {
            it.setBackgroundColor(if (acMode == "cool") activeBg else inactiveBg)
            it.setTextColor(if (acMode == "cool") cyanColor else mutedColor)
        }
        btnAcModeDry?.let {
            it.setBackgroundColor(if (acMode == "dry") activeBg else inactiveBg)
            it.setTextColor(if (acMode == "dry") cyanColor else mutedColor)
        }
        btnAcModeFan?.let {
            it.setBackgroundColor(if (acMode == "fan") activeBg else inactiveBg)
            it.setTextColor(if (acMode == "fan") cyanColor else mutedColor)
        }
        btnAcModeAuto?.let {
            it.setBackgroundColor(if (acMode == "auto") activeBg else inactiveBg)
            it.setTextColor(if (acMode == "auto") cyanColor else mutedColor)
        }
    }

    private fun getHttpBaseUrl(): String {
        val httpUrl = serverUrl
            .replaceFirst(Regex("^ws://", RegexOption.IGNORE_CASE), "http://")
            .replaceFirst(Regex("^wss://", RegexOption.IGNORE_CASE), "https://")
            .trimEnd('/')
        return runCatching {
            val uri = java.net.URI(httpUrl)
            val scheme = uri.scheme ?: "https"
            val authority = uri.authority ?: uri.host ?: ""
            if (authority.isNotEmpty()) "$scheme://$authority" else httpUrl
        }.getOrDefault(httpUrl)
    }

    private fun fetchAcLiveStatus() {
        val url = "${getHttpBaseUrl()}/iot/ac/status"
        val reqBuilder = Request.Builder().url(url).get()
        if (deviceId.isNotBlank()) reqBuilder.header("X-Device-Id", deviceId)
        if (deviceToken.isNotBlank()) reqBuilder.header("Authorization", "Bearer $deviceToken")

        httpClient.newCall(reqBuilder.build()).enqueue(object : Callback {
            override fun onFailure(call: Call, e: IOException) {
                // Offline or unreachable
            }

            override fun onResponse(call: Call, response: Response) {
                val bodyStr = response.body?.string() ?: ""
                response.close()
                runOnUiThread {
                    runCatching {
                        val json = JsonParser.parseString(bodyStr).asJsonObject
                        val auth = json.get("authenticated")?.asBoolean ?: false
                        isAcAuthenticated = auth
                        val state = if (json.has("state") && !json.get("state").isJsonNull) json.getAsJsonObject("state") else null
                        if (auth && state != null) {
                            if (state.has("power")) acPower = state.get("power").asBoolean
                            if (state.has("target_temp")) acTargetTemp = state.get("target_temp").asInt
                            if (state.has("mode")) acMode = state.get("mode").asString
                            if (state.has("fan_speed")) acFanSpeed = state.get("fan_speed").asString
                            if (state.has("swing")) acSwing = state.get("swing").asBoolean
                            if (state.has("power_watt")) acPowerWatt = state.get("power_watt").asDouble
                            if (state.has("room_temp")) acRoomTemp = state.get("room_temp").asDouble
                            if (state.has("energy_kwh")) acEnergyKwh = state.get("energy_kwh").asDouble
                        }
                        updateAcDisplay()
                    }
                }
            }
        })
    }

    private fun sendAcControl(action: String, extra: Map<String, Any> = emptyMap()) {
        val baseUrl = getHttpBaseUrl()
        val payload = mutableMapOf<String, Any>("action" to action)
        payload.putAll(extra)
        val jsonBody = Gson().toJson(payload)
        val mediaType = "application/json; charset=utf-8".toMediaTypeOrNull()
        val requestBody = jsonBody.toRequestBody(mediaType)
        val reqBuilder = Request.Builder()
            .url("$baseUrl/iot/control")
            .post(requestBody)
        if (deviceId.isNotBlank()) reqBuilder.header("X-Device-Id", deviceId)
        if (deviceToken.isNotBlank()) reqBuilder.header("Authorization", "Bearer $deviceToken")

        httpClient.newCall(reqBuilder.build()).enqueue(object : Callback {
            override fun onFailure(call: Call, e: IOException) {
                runOnUiThread {
                    Toast.makeText(this@IotDevicesActivity, "Gagal mengirim kontrol AC: ${e.message}", Toast.LENGTH_SHORT).show()
                }
            }
            override fun onResponse(call: Call, response: Response) {
                response.close()
                runOnUiThread {
                    fetchAcLiveStatus()
                }
            }
        })
    }

    private fun showPolytronAuthDialog() {
        val input = EditText(this).apply {
            hint = "No. WhatsApp (misal: 08123456789)"
            inputType = InputType.TYPE_CLASS_PHONE
            setTextColor(0xFFFFFFFF.toInt())
            setHintTextColor(0xFF888888.toInt())
            background = null
            setPadding(32, 24, 32, 24)
        }

        val container = FrameLayout(this).apply {
            setPadding(40, 20, 40, 10)
            addView(input)
        }

        AlertDialog.Builder(this)
            .setTitle("Login Polytron Fira Smart Home")
            .setMessage("Masukkan nomor WhatsApp yang terdaftar pada aplikasi Polytron Smart Home untuk menerima kode verifikasi OTP.")
            .setView(container)
            .setPositiveButton("Kirim OTP") { _, _ ->
                val phone = input.text.toString().trim()
                if (phone.isNotEmpty()) {
                    requestPolytronOtp(phone)
                }
            }
            .setNegativeButton("Batal", null)
            .show()
    }

    private fun requestPolytronOtp(phone: String) {
        val baseUrl = getHttpBaseUrl()
        val jsonBody = Gson().toJson(mapOf("phone" to phone))
        val mediaType = "application/json; charset=utf-8".toMediaTypeOrNull()
        val requestBody = jsonBody.toRequestBody(mediaType)
        val reqBuilder = Request.Builder()
            .url("$baseUrl/iot/ac/auth/request-otp")
            .post(requestBody)
        if (deviceId.isNotBlank()) reqBuilder.header("X-Device-Id", deviceId)
        if (deviceToken.isNotBlank()) reqBuilder.header("Authorization", "Bearer $deviceToken")

        httpClient.newCall(reqBuilder.build()).enqueue(object : Callback {
            override fun onFailure(call: Call, e: IOException) {
                runOnUiThread {
                    Toast.makeText(this@IotDevicesActivity, "Gagal request OTP: ${e.message}", Toast.LENGTH_LONG).show()
                }
            }
            override fun onResponse(call: Call, response: Response) {
                val bodyStr = response.body?.string() ?: ""
                response.close()
                runOnUiThread {
                    val jsonObj = runCatching { JsonParser.parseString(bodyStr).asJsonObject }.getOrNull()
                    if (jsonObj?.get("error")?.asString == "0") {
                        showPolytronOtpVerifyDialog(phone)
                    } else {
                        val msg = jsonObj?.get("message")?.asString ?: bodyStr
                        Toast.makeText(this@IotDevicesActivity, "Gagal: $msg", Toast.LENGTH_LONG).show()
                    }
                }
            }
        })
    }

    private fun showPolytronOtpVerifyDialog(phone: String) {
        val input = EditText(this).apply {
            hint = "Kode OTP (6 digit)"
            inputType = InputType.TYPE_CLASS_NUMBER
            setTextColor(0xFFFFFFFF.toInt())
            setHintTextColor(0xFF888888.toInt())
            background = null
            setPadding(32, 24, 32, 24)
        }

        val container = FrameLayout(this).apply {
            setPadding(40, 20, 40, 10)
            addView(input)
        }

        AlertDialog.Builder(this)
            .setTitle("Verifikasi Kode OTP")
            .setMessage("Kode OTP telah dikirimkan ke WhatsApp nomor $phone. Silakan masukkan kode tersebut:")
            .setView(container)
            .setPositiveButton("Verifikasi") { _, _ ->
                val otp = input.text.toString().trim()
                if (otp.isNotEmpty()) {
                    verifyPolytronOtp(phone, otp)
                }
            }
            .setNegativeButton("Batal", null)
            .show()
    }

    private fun verifyPolytronOtp(phone: String, otp: String) {
        val baseUrl = getHttpBaseUrl()
        val jsonBody = Gson().toJson(mapOf("phone" to phone, "otp_code" to otp))
        val mediaType = "application/json; charset=utf-8".toMediaTypeOrNull()
        val requestBody = jsonBody.toRequestBody(mediaType)
        val reqBuilder = Request.Builder()
            .url("$baseUrl/iot/ac/auth/verify-otp")
            .post(requestBody)
        if (deviceId.isNotBlank()) reqBuilder.header("X-Device-Id", deviceId)
        if (deviceToken.isNotBlank()) reqBuilder.header("Authorization", "Bearer $deviceToken")

        httpClient.newCall(reqBuilder.build()).enqueue(object : Callback {
            override fun onFailure(call: Call, e: IOException) {
                runOnUiThread {
                    Toast.makeText(this@IotDevicesActivity, "Verifikasi gagal: ${e.message}", Toast.LENGTH_LONG).show()
                }
            }
            override fun onResponse(call: Call, response: Response) {
                val bodyStr = response.body?.string() ?: ""
                response.close()
                runOnUiThread {
                    val jsonObj = runCatching { JsonParser.parseString(bodyStr).asJsonObject }.getOrNull()
                    val token = jsonObj?.get("token")?.asString ?: jsonObj?.getAsJsonObject("data")?.get("token")?.asString
                    if (!token.isNullOrEmpty()) {
                        Toast.makeText(this@IotDevicesActivity, "🎉 Akun Polytron Smart Home terhubung!", Toast.LENGTH_LONG).show()
                        fetchAcLiveStatus()
                    } else {
                        val msg = jsonObj?.get("message")?.asString ?: bodyStr
                        Toast.makeText(this@IotDevicesActivity, "Verifikasi gagal: $msg", Toast.LENGTH_LONG).show()
                    }
                }
            }
        })
    }

    private fun loadCachedBulbs() {
        val cached = wizManager.getCachedBulbs()
        if (cached.isNotEmpty()) {
            bulbsList.clear()
            bulbsList.addAll(cached)
            renderBulbs()
        }
    }

    private fun startWifiScan() {
        pbScanning.visibility = View.VISIBLE
        btnScanWifi.isEnabled = false
        tvBulbCount.setText(R.string.iot_scanning)
        ivSummaryLed.setColorFilter(0xFF00E5FF.toInt()) // Cyan scanning

        lifecycleScope.launch {
            val results = wizManager.scanBulbs()
            bulbsList.clear()
            bulbsList.addAll(results)
            renderBulbs()
            pbScanning.visibility = View.GONE
            btnScanWifi.isEnabled = true
        }
    }

    private fun renderBulbs() {
        layoutBulbsContainer.removeAllViews()

        if (bulbsList.isEmpty()) {
            layoutEmptyState.visibility = View.VISIBLE
            layoutBulbsContainer.visibility = View.GONE
            tvBulbCount.setText(R.string.iot_empty_title)
            ivSummaryLed.setColorFilter(0xFFFF5252.toInt()) // Red
            return
        }

        layoutEmptyState.visibility = View.GONE
        layoutBulbsContainer.visibility = View.VISIBLE
        tvBulbCount.text = getString(R.string.iot_found_count, bulbsList.size)
        ivSummaryLed.setColorFilter(0xFF00E676.toInt()) // Emerald Green

        val inflater = LayoutInflater.from(this)

        for (bulb in bulbsList) {
            val itemView = inflater.inflate(R.layout.item_wiz_bulb, layoutBulbsContainer, false)
            bindBulbItem(itemView, bulb)
            layoutBulbsContainer.addView(itemView)
        }
    }

    private fun bindBulbItem(view: View, bulb: DiscoveredBulb) {
        val ivIcon = view.findViewById<ImageView>(R.id.ivBulbIcon)
        val tvName = view.findViewById<TextView>(R.id.tvBulbName)
        val tvIp = view.findViewById<TextView>(R.id.tvBulbIp)
        val tvMac = view.findViewById<TextView>(R.id.tvBulbMac)
        val switchPower = view.findViewById<SwitchMaterial>(R.id.switchBulbPower)
        val tvBrightness = view.findViewById<TextView>(R.id.tvBulbBrightness)
        val sbBrightness = view.findViewById<SeekBar>(R.id.sbBulbBrightness)

        tvName.text = bulb.name
        tvIp.text = "IP: ${bulb.ip}:${WizUdpManager.WIZ_PORT}"
        tvMac.text = if (bulb.mac.isNotBlank()) "MAC: ${bulb.mac}" else "Zero-Cloud UDP"

        switchPower.isChecked = bulb.state
        updateIconGlow(ivIcon, bulb.state)

        switchPower.setOnCheckedChangeListener { _, isChecked ->
            bulb.state = isChecked
            updateIconGlow(ivIcon, isChecked)
            lifecycleScope.launch {
                wizManager.setPower(bulb.ip, isChecked)
            }
        }

        tvBrightness.text = getString(R.string.iot_brightness_label, bulb.dimming)
        sbBrightness.progress = bulb.dimming

        sbBrightness.setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
            override fun onProgressChanged(seekBar: SeekBar?, progress: Int, fromUser: Boolean) {
                if (fromUser) {
                    val value = progress.coerceIn(10, 100)
                    tvBrightness.text = getString(R.string.iot_brightness_label, value)
                }
            }

            override fun onStartTrackingTouch(seekBar: SeekBar?) {}

            override fun onStopTrackingTouch(seekBar: SeekBar?) {
                val value = (seekBar?.progress ?: 100).coerceIn(10, 100)
                bulb.dimming = value
                bulb.state = true
                switchPower.isChecked = true
                updateIconGlow(ivIcon, true)
                lifecycleScope.launch {
                    wizManager.setDimming(bulb.ip, value)
                }
            }
        })

        // Preset Chips
        view.findViewById<View>(R.id.chipWarmWhite).setOnClickListener {
            setPreset(bulb, switchPower, ivIcon) { wizManager.setKelvin(bulb.ip, 2700) }
        }
        view.findViewById<View>(R.id.chipDaylight).setOnClickListener {
            setPreset(bulb, switchPower, ivIcon) { wizManager.setKelvin(bulb.ip, 4000) }
        }
        view.findViewById<View>(R.id.chipCoolWhite).setOnClickListener {
            setPreset(bulb, switchPower, ivIcon) { wizManager.setKelvin(bulb.ip, 6500) }
        }
        view.findViewById<View>(R.id.chipCyan).setOnClickListener {
            setPreset(bulb, switchPower, ivIcon) { wizManager.setRgb(bulb.ip, 0, 229, 255) }
        }
        view.findViewById<View>(R.id.chipEmerald).setOnClickListener {
            setPreset(bulb, switchPower, ivIcon) { wizManager.setRgb(bulb.ip, 0, 230, 118) }
        }
        view.findViewById<View>(R.id.chipRed).setOnClickListener {
            setPreset(bulb, switchPower, ivIcon) { wizManager.setRgb(bulb.ip, 255, 82, 82) }
        }
        view.findViewById<View>(R.id.chipPurple).setOnClickListener {
            setPreset(bulb, switchPower, ivIcon) { wizManager.setRgb(bulb.ip, 156, 39, 176) }
        }
        view.findViewById<View>(R.id.chipAmber).setOnClickListener {
            setPreset(bulb, switchPower, ivIcon) { wizManager.setRgb(bulb.ip, 255, 179, 0) }
        }
    }

    private fun setPreset(bulb: DiscoveredBulb, switchPower: SwitchMaterial, ivIcon: ImageView, action: suspend () -> Unit) {
        bulb.state = true
        switchPower.isChecked = true
        updateIconGlow(ivIcon, true)
        lifecycleScope.launch {
            action()
        }
    }

    private fun updateIconGlow(ivIcon: ImageView, isPowerOn: Boolean) {
        if (isPowerOn) {
            ivIcon.setColorFilter(0xFF00E5FF.toInt()) // Cyan glow
            ivIcon.alpha = 1.0f
        } else {
            ivIcon.setColorFilter(0xFF5A6882.toInt()) // Muted dark slate
            ivIcon.alpha = 0.5f
        }
    }
}