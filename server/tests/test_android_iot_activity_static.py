"""Static regression tests for Android IoT Activity and WiZ Lighting list UI."""
from pathlib import Path
import xml.etree.ElementTree as ET

MAIN = Path(__file__).resolve().parents[2] / "android/app/src/main"
A = "{http://schemas.android.com/apk/res/android}"


def by_id(root, name):
    return next((e for e in root.iter() if e.get(A + "id") == "@+id/" + name), None)


def test_android_manifest_declares_iot_activity_and_wifi_permissions():
    manifest = ET.parse(MAIN / "AndroidManifest.xml").getroot()
    permissions = [e.get(A + "name") for e in manifest.findall("uses-permission")]
    assert "android.permission.ACCESS_WIFI_STATE" in permissions
    assert "android.permission.CHANGE_WIFI_MULTICAST_STATE" in permissions

    activities = [e.get(A + "name") for e in manifest.findall("application/activity")]
    assert ".ui.IotDevicesActivity" in activities


def test_activity_iot_devices_layout_elements():
    root = ET.parse(MAIN / "res/layout/activity_iot_devices.xml").getroot()
    expected_ids = (
        "topBar",
        "btnBack",
        "tvActivityTitle",
        "tvActivitySubtitle",
        "btnScanWifi",
        "cardSummary",
        "ivSummaryLed",
        "tvBulbCount",
        "pbScanning",
        "layoutGlobalActions",
        "btnAllOn",
        "btnAllOff",
        "btnModeFocus",
        "btnModeRelax",
        "layoutEmptyState",
        "ivEmptyIcon",
        "tvEmptyTitle",
        "tvEmptyDesc",
        "btnEmptyScan",
        "scrollBulbs",
        "layoutBulbsContainer",
    )
    for name in expected_ids:
        elem = by_id(root, name)
        assert elem is not None, f"Missing element @+id/{name} in activity_iot_devices.xml"


def test_item_polytron_ac_layout_elements():
    root = ET.parse(MAIN / "res/layout/item_polytron_ac.xml").getroot()
    expected_ids = (
        "cardPolytronAc",
        "ivAcIcon",
        "tvAcTitle",
        "tvAcSubtitle",
        "switchAcPower",
        "tvAcPowerWatt",
        "tvAcRoomTemp",
        "tvAcEnergy",
        "btnAcTempDown",
        "tvAcTargetTemp",
        "btnAcTempUp",
        "btnAcModeCool",
        "btnAcModeDry",
        "btnAcModeFan",
        "btnAcModeAuto",
        "btnAcFanSpeed",
        "btnAcSwingToggle",
    )
    for name in expected_ids:
        elem = by_id(root, name)
        assert elem is not None, f"Missing element @+id/{name} in item_polytron_ac.xml"


def test_item_wiz_bulb_layout_elements():
    root = ET.parse(MAIN / "res/layout/item_wiz_bulb.xml").getroot()
    expected_ids = (
        "cardBulbItem",
        "ivBulbIcon",
        "tvBulbName",
        "tvBulbIp",
        "tvBulbMac",
        "switchBulbPower",
        "layoutBrightness",
        "tvBulbBrightness",
        "sbBulbBrightness",
        "chipWarmWhite",
        "chipDaylight",
        "chipCoolWhite",
        "chipCyan",
        "chipEmerald",
        "chipRed",
        "chipPurple",
        "chipAmber",
    )
    for name in expected_ids:
        elem = by_id(root, name)
        assert elem is not None, f"Missing element @+id/{name} in item_wiz_bulb.xml"


def test_protocol_models_contain_iot_dialog():
    models_file = MAIN / "java/id/my/yhdev/nexo/network/ProtocolModels.kt"
    content = models_file.read_text()
    assert "data class IotDialog" in content
    assert "data class WizBulbItem" in content


def test_websocket_client_handles_iot_dialog():
    ws_file = MAIN / "java/id/my/yhdev/nexo/network/NexoWebSocketClient.kt"
    content = ws_file.read_text()
    assert "fun onIotDialogReceived(dialog: IotDialog)" in content
    assert '"iot_dialog" ->' in content
    assert "listener.onIotDialogReceived(dialog)" in content


def test_kiosk_service_delegates_iot_dialog():
    service_file = MAIN / "java/id/my/yhdev/nexo/service/NexoKioskService.kt"
    content = service_file.read_text()
    assert "fun onIotDialogReceived(dialog: id.my.yhdev.nexo.network.IotDialog)" in content
    assert "serviceCallback?.onIotDialogReceived(dialog)" in content


def test_main_activity_launches_iot_devices_activity():
    activity_file = MAIN / "java/id/my/yhdev/nexo/MainActivity.kt"
    content = activity_file.read_text()
    assert "onIotDialogReceived" in content
    assert "IotDevicesActivity::class.java" in content
    assert "startActivity(intent)" in content


def test_iot_devices_activity_code_structure():
    activity_file = MAIN / "java/id/my/yhdev/nexo/ui/IotDevicesActivity.kt"
    assert activity_file.exists()
    content = activity_file.read_text()
    assert "class IotDevicesActivity : AppCompatActivity()" in content
    assert "R.layout.activity_iot_devices" in content
    assert "WizUdpManager" in content
    assert "startWifiScan()" in content


def test_wiz_udp_manager_code_structure():
    manager_file = MAIN / "java/id/my/yhdev/nexo/iot/WizUdpManager.kt"
    assert manager_file.exists()
    content = manager_file.read_text()
    assert "38899" in content
    assert "255.255.255.255" in content
    assert "getSystemConfig" in content
    assert "getPilot" in content
    assert "setPilot" in content
