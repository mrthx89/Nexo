"""Unit tests for Philips WiZ smart lighting integration in Nexo."""

import pytest
from nexo_server.iot import parse_iot_command, get_iot_state, update_iot_state
from nexo_server.mascot import MascotExpression
from nexo_server.wiz import wiz_controller, COLOR_MAP, SCENE_MAP


@pytest.fixture(autouse=True)
def setup_wiz():
    wiz_controller.register_bulb("192.168.1.150", "Lampu Ruang Kerja WiZ")
    yield


def test_wiz_controller_registration():
    ip = wiz_controller.get_target_ip()
    assert ip == "192.168.1.150"
    assert "192.168.1.150" in wiz_controller.bulbs


@pytest.mark.asyncio
async def test_wiz_turn_on_and_off():
    res_on = await wiz_controller.turn_on(dimming=90)
    assert res_on["success"] is True
    assert res_on["state"] == "on"
    assert res_on["params"]["dimming"] == 90
    assert res_on["relay_packet"]["action"] == "turn_on"

    res_off = await wiz_controller.turn_off()
    assert res_off["success"] is True
    assert res_off["state"] == "off"
    assert res_off["params"]["state"] is False


@pytest.mark.asyncio
async def test_wiz_color_and_scene():
    res_color = await wiz_controller.set_color("biru")
    assert res_color["success"] is True
    assert res_color["params"]["r"] == 0
    assert res_color["params"]["b"] == 255

    res_temp = await wiz_controller.set_color("warm white")
    assert res_temp["success"] is True
    assert res_temp["params"]["temp"] == 2700

    res_scene = await wiz_controller.set_scene("focus")
    assert res_scene["success"] is True
    assert res_scene["params"]["sceneId"] == 15


@pytest.mark.parametrize("cmd", [
    "nyalakan lampu philips",
    "hidupkan lampu wiz",
    "nyalakan lampu rumah",
    "nexo tolong nyalakan lampu philips wiz ya",
])
def test_parse_wiz_light_on(cmd):
    res = parse_iot_command(cmd, is_sleeping=False)
    assert res is not None
    reply, expr, event = res
    assert "Lampu studio telah dinyalakan" in reply
    assert expr == MascotExpression.SENANG
    assert event["action"] == "studio_light_on"
    assert event["wiz_action"] == "turn_on"


@pytest.mark.parametrize("cmd", [
    "matikan lampu philips",
    "padamkan lampu wiz",
    "matikan lampu rumah",
])
def test_parse_wiz_light_off(cmd):
    res = parse_iot_command(cmd, is_sleeping=False)
    assert res is not None
    reply, expr, event = res
    assert "Lampu studio telah dimatikan" in reply
    assert expr == MascotExpression.NETRAL
    assert event["action"] == "studio_light_off"
    assert event["wiz_action"] == "turn_off"


def test_parse_wiz_color_command():
    res = parse_iot_command("nexo tolong ubah warna lampu jadi biru", is_sleeping=False)
    assert res is not None
    reply, expr, event = res
    assert "biru" in reply
    assert expr == MascotExpression.KEREN
    assert event["action"] == "wiz_set_color"
    assert event["color"] == "biru"


def test_parse_wiz_brightness_command():
    res = parse_iot_command("atur kecerahan lampu 80 persen", is_sleeping=False)
    assert res is not None
    reply, expr, event = res
    assert "80 persen" in reply
    assert expr == MascotExpression.SENANG
    assert event["action"] == "wiz_set_brightness"
    assert event["dimming"] == 80


@pytest.mark.parametrize("cmd", [
    "tampilkan iot",
    "buka iot",
    "daftar iot",
    "list iot",
    "tampilkan lampu",
    "daftar lampu",
    "nexo tampilkan iot",
    "nexo buka iot",
    "tampilkan lampu wiz",
])
def test_parse_show_iot_command(cmd):
    res = parse_iot_command(cmd, is_sleeping=False)
    assert res is not None
    reply, expr, event = res
    assert "Membuka daftar perangkat IoT studio Anda" in reply
    assert expr == MascotExpression.SENANG
    assert event["type"] == "iot_dialog"
    assert event["action"] == "show_iot"
    assert event["target"] == "iot_activity"

