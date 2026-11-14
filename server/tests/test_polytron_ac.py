"""Unit tests for Polytron Smart Neuva Pro AC integration and voice intents."""
import json
import pytest
from unittest.mock import MagicMock, patch
from nexo_server.polytron_ac import (
    PolytronAcController,
    PolytronAcState,
    HVAC_MODE_TO_POLYTRON,
    FAN_SPEED_TO_POLYTRON,
    polytron_ac_controller,
)
from nexo_server.iot import parse_iot_command, get_iot_state
from nexo_server.mascot import MascotExpression


def test_polytron_ac_state_to_dict():
    st = PolytronAcState(
        serial_number="PAC05VZI12345",
        label="AC Studio",
        power=True,
        mode="cool",
        target_temp=22,
        fan_speed="auto",
        room_temp=24.5,
        power_watt=310.0,
        energy_kwh=1.25,
        online=True,
    )
    d = st.to_dict()
    assert d["serial_number"] == "PAC05VZI12345"
    assert d["power"] is True
    assert d["target_temp"] == 22
    assert d["power_watt"] == 310.0
    assert d["room_temp"] == 24.5
    assert d["energy_kwh"] == 1.25


def test_hvac_mappings():
    assert HVAC_MODE_TO_POLYTRON["cool"] == 2
    assert HVAC_MODE_TO_POLYTRON["dingin"] == 2
    assert HVAC_MODE_TO_POLYTRON["dry"] == 3
    assert HVAC_MODE_TO_POLYTRON["fan"] == 1
    assert HVAC_MODE_TO_POLYTRON["auto"] == 4

    assert FAN_SPEED_TO_POLYTRON["low"] == 0
    assert FAN_SPEED_TO_POLYTRON["auto"] == 1
    assert FAN_SPEED_TO_POLYTRON["medium"] == 2
    assert FAN_SPEED_TO_POLYTRON["high"] == 3


def test_polytron_controller_config(tmp_path):
    cfg_file = tmp_path / "polytron_test.json"
    ctrl = PolytronAcController(config_path=str(cfg_file))
    assert not ctrl.is_authenticated()

    ctrl.phone = "08123456789"
    ctrl.token = "token_test_123"
    ctrl.default_serial = "PAC09_TEST"
    ctrl._save_config()

    assert cfg_file.exists()
    loaded_ctrl = PolytronAcController(config_path=str(cfg_file))
    assert loaded_ctrl.is_authenticated()
    assert loaded_ctrl.phone == "08123456789"
    assert loaded_ctrl.token == "token_test_123"
    assert loaded_ctrl.default_serial == "PAC09_TEST"


@pytest.mark.parametrize("cmd", [
    "nyalakan ac",
    "hidupkan ac",
    "nyalakan ac polytron",
    "hidupkan ac studio",
    "nyalakan pendingin ruangan",
    "nexo nyalakan ac",
])
def test_parse_ac_on_commands(cmd):
    res = parse_iot_command(cmd, is_sleeping=False)
    assert res is not None
    reply, expr, event = res
    assert "AC Polytron Smart Neuva Pro telah dinyalakan" in reply
    assert expr == MascotExpression.SENANG
    assert event["action"] == "ac_turn_on"
    assert event["device"] == "ac"


@pytest.mark.parametrize("cmd", [
    "matikan ac",
    "padamkan ac",
    "matikan ac polytron",
    "matikan ac kamar",
    "matikan pendingin ruangan",
    "nexo matikan ac",
])
def test_parse_ac_off_commands(cmd):
    res = parse_iot_command(cmd, is_sleeping=False)
    assert res is not None
    reply, expr, event = res
    assert "AC telah dimatikan" in reply
    assert expr == MascotExpression.NETRAL
    assert event["action"] == "ac_turn_off"
    assert event["device"] == "ac"


def test_parse_ac_set_temperature():
    res = parse_iot_command("setel suhu ac 22 derajat", is_sleeping=False)
    assert res is not None
    reply, expr, event = res
    assert "22 derajat Celsius" in reply
    assert expr == MascotExpression.SENANG
    assert event["action"] == "ac_set_temperature"
    assert event["target_temp"] == 22


def test_parse_ac_set_mode():
    res = parse_iot_command("ac mode dingin", is_sleeping=False)
    assert res is not None
    reply, expr, event = res
    assert "dingin" in reply
    assert expr == MascotExpression.KEREN
    assert event["action"] == "ac_set_mode"
    assert event["mode"] == "dingin"


def test_parse_ac_status():
    res = parse_iot_command("berapa watt ac sekarang", is_sleeping=False)
    assert res is not None
    reply, expr, event = res
    assert expr == MascotExpression.SENANG
    assert event["type"] == "iot_telemetry"
    assert event["action"] == "ac_status"


def test_get_iot_state_contains_ac():
    st = get_iot_state()
    assert "ac" in st
    assert "authenticated" in st["ac"]
    assert "default_serial" in st["ac"]
