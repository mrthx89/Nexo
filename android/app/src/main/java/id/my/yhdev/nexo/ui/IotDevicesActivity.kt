package id.my.yhdev.nexo.ui

import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.SeekBar
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import com.google.android.material.button.MaterialButton
import com.google.android.material.switchmaterial.SwitchMaterial
import id.my.yhdev.nexo.R
import id.my.yhdev.nexo.iot.DiscoveredBulb
import id.my.yhdev.nexo.iot.WizUdpManager
import kotlinx.coroutines.launch

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
    private var acPower: Boolean = false
    private var acTargetTemp: Int = 22
    private var acMode: String = "cool"
    private var acFanSpeed: String = "auto"
    private var acSwing: Boolean = false

    // Polytron AC Views
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

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_iot_devices)

        wizManager = WizUdpManager(this)

        initViews()
        initAcView()
        loadCachedBulbs()
        startWifiScan()
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
            acPower = isChecked
            updateAcDisplay()
        }

        findViewById<View>(R.id.btnAcTempDown)?.setOnClickListener {
            acTargetTemp = (acTargetTemp - 1).coerceAtLeast(16)
            updateAcDisplay()
        }

        findViewById<View>(R.id.btnAcTempUp)?.setOnClickListener {
            acTargetTemp = (acTargetTemp + 1).coerceAtMost(32)
            updateAcDisplay()
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
        }

        btnAcSwingToggle?.setOnClickListener {
            acSwing = !acSwing
            updateAcDisplay()
        }
    }

    private fun setAcMode(mode: String) {
        acMode = mode
        updateAcDisplay()
    }

    private fun updateAcDisplay() {
        tvAcTargetTemp?.text = "${acTargetTemp}°C"
        switchAcPower?.isChecked = acPower

        if (acPower) {
            tvAcPowerWatt?.text = "⚡ 310 W"
            tvAcRoomTemp?.text = "🌡️ 24.5°C"
            tvAcEnergy?.text = "📈 1.25 kWh"
        } else {
            tvAcPowerWatt?.text = "⚡ 0 W"
            tvAcRoomTemp?.text = "🌡️ 26.0°C"
            tvAcEnergy?.text = "📈 1.25 kWh"
        }

        btnAcFanSpeed?.text = "Fan: ${acFanSpeed.replaceFirstChar { it.uppercase() }}"
        btnAcSwingToggle?.text = if (acSwing) "Swing: Aktif" else "Louver Swing"
        btnAcSwingToggle?.setTextColor(if (acSwing) 0xFF00E5FF.toInt() else 0xFFE0E0E0.toInt())

        val cyanColor = 0xFF00E5FF.toInt()
        val mutedColor = 0xFF9E9E9E.toInt()
        val activeBg = 0xFF0E2333.toInt()
        val inactiveBg = 0xFF1A2633.toInt()

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
