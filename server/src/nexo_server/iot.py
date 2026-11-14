"""Smart Appliance IoT Control, Studio Scenes, and Real-Time Server Telemetry."""

import asyncio
import json
import logging
import os
from pathlib import Path
import re
import shutil
import time
import urllib.request
from typing import Any, Dict, Optional, Tuple

from .mascot import MascotExpression, _COMMAND
from .wiz import wiz_controller, COLOR_MAP, SCENE_MAP
from .polytron_ac import polytron_ac_controller, HVAC_MODE_TO_POLYTRON, FAN_SPEED_TO_POLYTRON

logger = logging.getLogger(__name__)

# In-memory state of smart studio devices and active scene
_IOT_STATE: Dict[str, Any] = {
    "active_scene": "normal",
    "devices": {
        "studio_light": "off",
        "ambient_backlight": "on",
    },
    "last_updated": 0,
}

_SERVER_STATUS_COMMANDS = {
    "cek status server",
    "status server",
    "kondisi server",
    "info server",
    "cek server",
    "status sistem",
    "cek sistem",
    "kesehatan server",
    "berapa suhu server",
    "berapa suhu server saat ini",
    "suhu server",
    "suhu server saat ini",
    "cek memori",
    "penggunaan memori",
    "berapa penggunaan memori",
    "cek ram",
    "penggunaan ram",
    "cek cpu",
    "beban cpu",
    "uptime server",
}

_FOCUS_SCENE_COMMANDS = {
    "mode fokus",
    "aktifkan mode fokus",
    "pasang mode fokus",
    "nyalakan mode fokus",
    "masuk mode fokus",
    "fokus kerja",
}

_RELAX_SCENE_COMMANDS = {
    "mode santai",
    "aktifkan mode santai",
    "pasang mode santai",
    "nyalakan mode santai",
    "mode istirahat",
    "aktifkan mode istirahat",
    "suasana santai",
}

_LIGHT_ON_COMMANDS = {
    "nyalakan lampu",
    "nyalakan lampu studio",
    "hidupkan lampu",
    "hidupkan lampu studio",
    "nyalakan lampu kerja",
    "hidupkan lampu kerja",
    "nyalakan lampu rumah",
    "hidupkan lampu rumah",
    "nyalakan lampu wiz",
    "hidupkan lampu wiz",
    "nyalakan lampu philips",
    "hidupkan lampu philips",
    "nyalakan lampu philips wiz",
    "hidupkan lampu philips wiz",
    "nyalakan iot",
    "hidupkan iot",
    "iot on",
    "aktifkan iot",
    "pidupkan iot",
    "hidupkan semua perangkat",
    "nyalakan semua perangkat",
    "hidupkan perangkat iot",
    "nyalakan perangkat iot",
}

_LIGHT_OFF_COMMANDS = {
    "matikan lampu",
    "matikan lampu studio",
    "padamkan lampu",
    "padamkan lampu studio",
    "matikan lampu kerja",
    "padamkan lampu kerja",
    "matikan lampu rumah",
    "padamkan lampu rumah",
    "matikan lampu wiz",
    "padamkan lampu wiz",
    "matikan lampu philips",
    "padamkan lampu philips",
    "matikan lampu philips wiz",
    "padamkan lampu philips wiz",
    "matikan iot",
    "padamkan iot",
    "iot off",
    "eot off",
    "ert off",
    "matikan semua perangkat",
    "matikan perangkat",
    "matikan perangkat iot",
}

_COMBO_FOCUS_LIGHT_COMMANDS = {
    "nyalakan lampu studio dan pasang mode fokus",
    "nyalakan lampu dan pasang mode fokus",
    "nyalakan lampu dan aktifkan mode fokus",
    "pasang mode fokus dan nyalakan lampu studio",
    "pasang mode fokus dan nyalakan lampu",
}

_SHOW_IOT_COMMANDS = {
    "tampilkan iot",
    "buka iot",
    "daftar iot",
    "list iot",
    "kelola iot",
    "kontrol iot",
    "kendali iot",
    "panel iot",
    "pengaturan iot",
    "setting iot",
    "tampilkan lampu",
    "buka lampu",
    "daftar lampu",
    "list lampu",
    "kelola lampu",
    "kontrol lampu",
    "kendali lampu",
    "panel lampu",
    "pengaturan lampu",
    "setting lampu",
    "settingan lampu",
    "buka kontrol lampu",
    "buka kendali lampu",
    "buka pengaturan lampu",
    "buka setting lampu",
    "buka settingan lampu",
    "buka panel lampu",
    "tampilkan kontrol lampu",
    "tampilkan pengaturan lampu",
    "tampilkan setting lampu",
    "tampilkan settingan lampu",
    "tampilkan panel lampu",
    "tampilkan perangkat iot",
    "buka perangkat iot",
    "daftar perangkat iot",
    "perangkat iot",
    "tampilkan lampu wiz",
    "buka lampu wiz",
    "daftar lampu wiz",
    "kontrol lampu wiz",
    "buka kontrol lampu wiz",
    "tampilkan ac",
    "buka ac",
    "daftar ac",
    "kontrol ac",
    "kendali ac",
    "panel ac",
    "pengaturan ac",
    "setting ac",
    "settingan ac",
    "buka kontrol ac",
    "buka kendali ac",
    "buka pengaturan ac",
    "buka setting ac",
    "buka settingan ac",
    "buka panel ac",
    "tampilkan kontrol ac",
    "tampilkan pengaturan ac",
    "tampilkan setting ac",
    "tampilkan panel ac",
    "buka smart home",
    "tampilkan smart home",
    "smart home",
    "panel kontrol",
    "buka panel kontrol",
    "tampilkan panel kontrol",
}

_SHOW_IOT_PATTERNS = [
    re.compile(r"^(buka|pukak|tampilkan|lihat|daftar|list|kelola)\s+(kontrol|kendali|panel|pengaturan|settingan|setting)?\s*(lampu|lighting|wiz|iot|ac|pendingin|smart\s*home|perangkat)", re.I),
    re.compile(r"^(kontrol|kendali|panel|pengaturan|settingan|setting)\s+(lampu|lighting|wiz|iot|ac|pendingin|smart\s*home|perangkat)", re.I),
    re.compile(r"^panel\s+kontrol(\s+(lampu|lighting|wiz|iot|ac|pendingin|smart\s*home|perangkat))?", re.I),
    re.compile(r"^(smart\s*home|perangkat\s*iot)$", re.I),
]

_AC_ON_COMMANDS = {
    "nyalakan ac",
    "hidupkan ac",
    "nyalakan ac studio",
    "hidupkan ac studio",
    "nyalakan ac kamar",
    "hidupkan ac kamar",
    "nyalakan ac polytron",
    "hidupkan ac polytron",
    "nyalakan pendingin",
    "hidupkan pendingin",
    "nyalakan pendingin ruangan",
    "hidupkan pendingin ruangan",
}

_AC_OFF_COMMANDS = {
    "matikan ac",
    "padamkan ac",
    "matikan ac studio",
    "padamkan ac studio",
    "matikan ac kamar",
    "padamkan ac kamar",
    "matikan ac polytron",
    "padamkan ac polytron",
    "matikan pendingin",
    "matikan pendingin ruangan",
    "padamkan pendingin ruangan",
}

_AC_STATUS_COMMANDS = {
    "cek ac",
    "status ac",
    "kondisi ac",
    "suhu ac",
    "daya ac",
    "watt ac",
    "berapa watt ac",
    "berapa watt ac sekarang",
    "konsumsi ac",
    "konsumsi listrik ac",
    "cek ac polytron",
    "status ac polytron",
}


def get_iot_state() -> Dict[str, Any]:
    """Return a snapshot of the current smart studio state."""
    wiz_target = wiz_controller.get_target_ip()
    wiz_st = wiz_controller.states.get(wiz_target) if wiz_target else None

    ac_sn = polytron_ac_controller.default_serial
    ac_st = polytron_ac_controller.states.get(ac_sn) if ac_sn else None

    return {
        "active_scene": _IOT_STATE["active_scene"],
        "devices": dict(_IOT_STATE["devices"]),
        "wiz": {
            "configured_ip": wiz_target,
            "state": wiz_st.to_dict() if wiz_st else None,
            "registered_bulbs": list(wiz_controller.bulbs.keys()),
        },
        "ac": {
            "authenticated": polytron_ac_controller.is_authenticated(),
            "default_serial": ac_sn,
            "state": ac_st.to_dict() if ac_st else None,
            "device_count": len(polytron_ac_controller.devices),
        },
        "last_updated": _IOT_STATE["last_updated"],
    }


def update_iot_state(
    scene: Optional[str] = None,
    device: Optional[str] = None,
    state: Optional[str] = None,
) -> Dict[str, Any]:
    """Update smart studio state and return the updated snapshot."""
    if scene is not None:
        _IOT_STATE["active_scene"] = scene
    if device is not None and state is not None:
        _IOT_STATE["devices"][device] = state
    _IOT_STATE["last_updated"] = int(time.time() * 1000)
    return get_iot_state()


def get_system_telemetry() -> Dict[str, Any]:
    """Read real Linux server telemetry (CPU load, RAM, Disk, Uptime, Thermal)."""
    load_1m, load_5m, load_15m = 0.0, 0.0, 0.0
    try:
        load_1m, load_5m, load_15m = os.getloadavg()
    except OSError:
        pass

    mem_total_mb = 0
    mem_avail_mb = 0
    mem_used_pct = 0.0
    try:
        meminfo = Path("/proc/meminfo").read_text(encoding="utf-8")
        total_kb = 0
        avail_kb = 0
        for line in meminfo.splitlines():
            if line.startswith("MemTotal:"):
                total_kb = int(line.split()[1])
            elif line.startswith("MemAvailable:"):
                avail_kb = int(line.split()[1])
        if total_kb > 0:
            mem_total_mb = round(total_kb / 1024)
            mem_avail_mb = round(avail_kb / 1024)
            mem_used_pct = round((total_kb - avail_kb) / total_kb * 100, 1)
    except OSError:
        pass

    uptime_seconds = 0.0
    try:
        raw_uptime = Path("/proc/uptime").read_text(encoding="utf-8").split()[0]
        uptime_seconds = float(raw_uptime)
    except (OSError, ValueError, IndexError):
        pass

    disk_free_gb = 0.0
    disk_total_gb = 0.0
    try:
        usage = shutil.disk_usage("/")
        disk_free_gb = round(usage.free / (1024 ** 3), 1)
        disk_total_gb = round(usage.total / (1024 ** 3), 1)
    except OSError:
        pass

    temp_c: Optional[float] = None
    try:
        for zone in sorted(Path("/sys/class/thermal").glob("thermal_zone*/temp")):
            raw_temp = int(zone.read_text(encoding="utf-8").strip())
            if raw_temp > 0:
                temp_c = round(raw_temp / 1000.0, 1)
                break
    except (OSError, ValueError):
        pass

    return {
        "cpu_load_1m": round(load_1m, 2),
        "cpu_load_5m": round(load_5m, 2),
        "cpu_load_15m": round(load_15m, 2),
        "ram_total_mb": mem_total_mb,
        "ram_available_mb": mem_avail_mb,
        "ram_used_percent": mem_used_pct,
        "disk_free_gb": disk_free_gb,
        "disk_total_gb": disk_total_gb,
        "uptime_seconds": round(uptime_seconds),
        "uptime_days": round(uptime_seconds / 86400, 1),
        "temperature_c": temp_c,
    }


def format_system_telemetry_speech(metrics: Optional[Dict[str, Any]] = None) -> str:
    """Format system telemetry into natural, concise Indonesian speech for TTS."""
    data = metrics or get_system_telemetry()
    load_str = str(data["cpu_load_1m"]).replace(".", " koma ")
    ram_pct = round(data["ram_used_percent"])
    disk_free = str(data["disk_free_gb"]).replace(".", " koma ")

    uptime_sec = data.get("uptime_seconds", 0)
    days = uptime_sec // 86400
    hours = (uptime_sec % 86400) // 3600
    if days > 0:
        uptime_str = f"{days} hari {hours} jam" if hours > 0 else f"{days} hari"
    elif hours > 0:
        uptime_str = f"{hours} jam"
    else:
        mins = max(1, uptime_sec // 60)
        uptime_str = f"{mins} menit"

    temp_part = ""
    if data.get("temperature_c") is not None:
        temp_str = str(data["temperature_c"]).replace(".", " koma ")
        temp_part = f" Suhu prosesor {temp_str} derajat Celcius."

    return (
        f"Status server saat ini beroperasi prima.{temp_part} "
        f"Beban CPU {load_str}, pemakaian memori RAM {ram_pct} persen, "
        f"ruang penyimpanan bebas {disk_free} gigabita, "
        f"dan sistem telah aktif selama {uptime_str}. "
        "Seluruh layanan Nexo siap mendampingi Anda, Pak Yanto."
    )


async def trigger_external_iot_webhook(event_payload: Dict[str, Any]) -> bool:
    """Optionally forward IoT state changes to a configured Home Assistant / webhook URL."""
    webhook_url = os.getenv("NEXO_IOT_WEBHOOK_URL", "").strip()
    if not webhook_url or not webhook_url.startswith(("http://", "https://")):
        return False

    def _post():
        body = json.dumps(event_payload).encode("utf-8")
        req = urllib.request.Request(
            webhook_url,
            data=body,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        with urllib.request.urlopen(req, timeout=4.0) as resp:
            return 200 <= resp.status < 300

    try:
        return await asyncio.to_thread(_post)
    except Exception as exc:
        logger.warning("External IoT webhook delivery failed: %s", exc)
        return False


def _safe_create_task(coro):
    """Schedule coroutine safely if an event loop is running."""
    try:
        loop = asyncio.get_running_loop()
        return loop.create_task(coro)
    except RuntimeError:
        coro.close()
        return None


def parse_iot_command(
    text: str, is_sleeping: bool = False
) -> Optional[Tuple[str, MascotExpression, Dict[str, Any]]]:
    """Parse standalone smart home, studio mode, and server telemetry voice commands.

    Returns:
        (reply_text, expression, iot_event_dict) or None if not a recognized IoT command.
    """
    if not text or is_sleeping:
        return None

    clean = " ".join(text.casefold().strip().split())
    clean = re.sub(r"[.?!,]+$", "", clean).strip()
    clean_body = clean
    match = _COMMAND.fullmatch(clean)
    if match:
        clean_body = re.sub(r"[.?!,]+$", "", match["body"].strip()).strip()

    # 0. Show IoT / Smart Home / WiZ Lighting Activity
    is_show_iot = (
        clean in _SHOW_IOT_COMMANDS
        or clean_body in _SHOW_IOT_COMMANDS
        or any(p.search(clean) or p.search(clean_body) for p in _SHOW_IOT_PATTERNS)
    )
    if is_show_iot:
        target_entity = clean_body or clean
        if any(w in target_entity for w in ("ac", "pendingin", "suhu", "neuva")):
            focus = "ac"
            reply = "Membuka kontrol pendingin udara AC Polytron studio Anda, Pak Yanto."
        elif any(w in target_entity for w in ("lampu", "lighting", "wiz")):
            focus = "lighting"
            reply = "Membuka kontrol lampu pintar Philips WiZ studio Anda, Pak Yanto."
        else:
            focus = "all"
            reply = "Membuka kontrol dan daftar perangkat IoT studio Anda, Pak Yanto."

        state_snap = get_iot_state()
        event = {
            "type": "iot_dialog",
            "action": "show_iot",
            "target": "iot_activity",
            "focus": focus,
            "iot_state": state_snap,
        }
        return reply, MascotExpression.SENANG, event

    # 1. Combo: Studio Light + Focus Mode
    if clean in _COMBO_FOCUS_LIGHT_COMMANDS or clean_body in _COMBO_FOCUS_LIGHT_COMMANDS:
        state_snap = update_iot_state(scene="focus", device="studio_light", state="on")
        wiz_target = wiz_controller.get_target_ip()
        _safe_create_task(wiz_controller.set_scene("focus"))
        event = {
            "type": "iot_control",
            "action": "studio_focus_combo",
            "iot_state": state_snap,
            "wiz_action": "set_scene",
            "wiz_scene": "focus",
            "wiz_ip": wiz_target,
        }
        reply = (
            "Lampu studio dinyalakan dan mode fokus diaktifkan. "
            "Seluruh instrumen kerja siap mendampingi Anda berkarya dengan hati, Pak Yanto!"
        )
        return reply, MascotExpression.SANGAT_SENANG, event

    # 2. Focus Mode
    if clean in _FOCUS_SCENE_COMMANDS or clean_body in _FOCUS_SCENE_COMMANDS:
        state_snap = update_iot_state(scene="focus", device="ambient_backlight", state="focus_cyan")
        wiz_target = wiz_controller.get_target_ip()
        _safe_create_task(wiz_controller.set_scene("focus"))
        event = {
            "type": "iot_control",
            "action": "focus_mode",
            "iot_state": state_snap,
            "wiz_action": "set_scene",
            "wiz_scene": "focus",
            "wiz_ip": wiz_target,
        }
        reply = (
            "Mode fokus diaktifkan. Suasana studio disiapkan untuk konsentrasi penuh. "
            "Selamat berkarya dengan hati, Pak Yanto!"
        )
        return reply, MascotExpression.KEREN, event

    # 3. Relax Mode
    if clean in _RELAX_SCENE_COMMANDS or clean_body in _RELAX_SCENE_COMMANDS:
        state_snap = update_iot_state(scene="relax", device="ambient_backlight", state="warm_amber")
        wiz_target = wiz_controller.get_target_ip()
        _safe_create_task(wiz_controller.set_scene("relax"))
        event = {
            "type": "iot_control",
            "action": "relax_mode",
            "iot_state": state_snap,
            "wiz_action": "set_scene",
            "wiz_scene": "relax",
            "wiz_ip": wiz_target,
        }
        reply = (
            "Mode santai diaktifkan. Suasana ruangan dibuat lebih tenang dan hangat. "
            "Selamat beristirahat sejenak, Pak Yanto."
        )
        return reply, MascotExpression.SENANG, event

    # 4. Studio Light ON (including WiZ light)
    if clean in _LIGHT_ON_COMMANDS or clean_body in _LIGHT_ON_COMMANDS:
        state_snap = update_iot_state(device="studio_light", state="on")
        wiz_target = wiz_controller.get_target_ip()
        event = {
            "type": "iot_control",
            "action": "studio_light_on",
            "iot_state": state_snap,
            "wiz_action": "turn_on",
            "wiz_ip": wiz_target,
        }
        _safe_create_task(wiz_controller.turn_on())
        reply = "Lampu studio telah dinyalakan. Ruangan kerja siap digunakan, Pak Yanto."
        return reply, MascotExpression.SENANG, event

    # 5. Studio Light OFF (including WiZ light)
    if clean in _LIGHT_OFF_COMMANDS or clean_body in _LIGHT_OFF_COMMANDS:
        update_iot_state(device="ambient_backlight", state="off")
        state_snap = update_iot_state(device="studio_light", state="off")
        wiz_target = wiz_controller.get_target_ip()
        event = {
            "type": "iot_control",
            "action": "studio_light_off",
            "iot_state": state_snap,
            "wiz_action": "turn_off",
            "wiz_ip": wiz_target,
        }
        _safe_create_task(wiz_controller.turn_off())
        reply = "Lampu studio telah dimatikan."
        return reply, MascotExpression.NETRAL, event

    # 5b. WiZ Dynamic Control: Color (e.g. "ubah warna lampu jadi biru", "ganti warna lampu merah")
    color_match = re.search(
        r"(?:ubah|ganti|pasang|atur)?\s*(?:warna\s+)?lampu\s+(?:menjadi|jadi|ke)?\s*([a-z\s]+)",
        clean_body,
    )
    if color_match:
        target_color = color_match.group(1).strip()
        if target_color in COLOR_MAP:
            wiz_target = wiz_controller.get_target_ip()
            _safe_create_task(wiz_controller.set_color(target_color))
            state_snap = update_iot_state(device="studio_light", state="on")
            event = {
                "type": "iot_control",
                "action": "wiz_set_color",
                "color": target_color,
                "iot_state": state_snap,
                "wiz_ip": wiz_target,
            }
            reply = f"Warna lampu Philips WiZ diubah menjadi {target_color}."
            return reply, MascotExpression.KEREN, event

    # 5c. WiZ Dynamic Control: Brightness (e.g. "atur kecerahan lampu 80 persen", "terangkan lampu 50 persen")
    dim_match = re.search(
        r"(?:atur|ubah|set|bikin)?\s*(?:kecerahan|terang|dim)?\s*lampu\s+(?:menjadi|jadi|ke)?\s*(\d{1,3})\s*(?:%|persen)?",
        clean_body,
    )
    if dim_match:
        pct = int(dim_match.group(1))
        if 5 <= pct <= 100:
            wiz_target = wiz_controller.get_target_ip()
            _safe_create_task(wiz_controller.set_brightness(pct))
            state_snap = update_iot_state(device="studio_light", state="on")
            event = {
                "type": "iot_control",
                "action": "wiz_set_brightness",
                "dimming": pct,
                "iot_state": state_snap,
                "wiz_ip": wiz_target,
            }
            reply = f"Kecerahan lampu diatur ke {pct} persen, Pak Yanto."
            return reply, MascotExpression.SENANG, event

    # 5d. Polytron AC Power ON
    if clean in _AC_ON_COMMANDS or clean_body in _AC_ON_COMMANDS:
        _safe_create_task(polytron_ac_controller.async_turn_on())
        state_snap = update_iot_state(device="ac", state="on")
        event = {
            "type": "iot_control",
            "action": "ac_turn_on",
            "iot_state": state_snap,
            "device": "ac",
        }
        reply = "AC Polytron Smart Neuva Pro telah dinyalakan. Ruangan akan segera sejuk, Pak Yanto."
        return reply, MascotExpression.SENANG, event

    # 5e. Polytron AC Power OFF
    if clean in _AC_OFF_COMMANDS or clean_body in _AC_OFF_COMMANDS:
        _safe_create_task(polytron_ac_controller.async_turn_off())
        state_snap = update_iot_state(device="ac", state="off")
        event = {
            "type": "iot_control",
            "action": "ac_turn_off",
            "iot_state": state_snap,
            "device": "ac",
        }
        reply = "AC telah dimatikan."
        return reply, MascotExpression.NETRAL, event

    # 5f. Polytron AC Set Temperature (e.g. "setel suhu ac 22 derajat", "atur ac 20", "suhu ac 24 derajat")
    ac_temp_match = re.search(
        r"(?:atur|setel|ubah|set|bikin)?\s*(?:suhu\s+)?ac\s+(?:ke|menjadi|jadi)?\s*(\d{2})\s*(?:derajat|celsius|c)?",
        clean_body,
    )
    if ac_temp_match:
        temp_val = int(ac_temp_match.group(1))
        if 16 <= temp_val <= 32:
            _safe_create_task(polytron_ac_controller.async_set_temperature(temp_val))
            state_snap = update_iot_state(device="ac", state="on")
            event = {
                "type": "iot_control",
                "action": "ac_set_temperature",
                "target_temp": temp_val,
                "iot_state": state_snap,
                "device": "ac",
            }
            reply = f"Suhu AC diatur ke {temp_val} derajat Celsius, Pak Yanto."
            return reply, MascotExpression.SENANG, event

    # 5g. Polytron AC Mode (e.g. "ac mode dingin", "mode ac dingin", "setel ac ke mode cool")
    ac_mode_match = re.search(
        r"(?:mode\s+ac|ac\s+mode)\s+(dingin|cool|dry|kering|fan|kipas|auto|otomatis)",
        clean_body,
    )
    if ac_mode_match:
        mode_val = ac_mode_match.group(1).strip()
        _safe_create_task(polytron_ac_controller.async_set_mode(mode_val))
        state_snap = update_iot_state(device="ac", state="on")
        event = {
            "type": "iot_control",
            "action": "ac_set_mode",
            "mode": mode_val,
            "iot_state": state_snap,
            "device": "ac",
        }
        reply = f"Mode AC diubah menjadi {mode_val}, Pak Yanto."
        return reply, MascotExpression.KEREN, event

    # 5h. Polytron AC Status / Telemetry Check
    if clean in _AC_STATUS_COMMANDS or clean_body in _AC_STATUS_COMMANDS:
        sn = polytron_ac_controller.default_serial
        st = polytron_ac_controller.states.get(sn)
        if st and st.power:
            room_str = f"Suhu ruangan saat ini {st.room_temp:.1f} derajat. " if st.room_temp is not None else ""
            watt_str = f"Daya aktif AC {int(st.power_watt)} Watt." if st.power_watt > 0 else ""
            reply = f"AC Polytron menyala di {st.target_temp} derajat, mode {st.mode}. {room_str}{watt_str}".strip()
        elif st and not st.power:
            reply = "AC Polytron saat ini dalam kondisi mati (standby)."
        else:
            reply = "AC Polytron Smart Neuva Pro terhubung ke sistem Nexo."
        event = {
            "type": "iot_telemetry",
            "action": "ac_status",
            "ac_state": st.to_dict() if st else None,
        }
        return reply, MascotExpression.SENANG, event

    # 6. Server Telemetry & Health Check
    if clean in _SERVER_STATUS_COMMANDS or clean_body in _SERVER_STATUS_COMMANDS:
        metrics = get_system_telemetry()
        event = {
            "type": "iot_telemetry",
            "action": "server_status",
            "metrics": metrics,
        }
        reply = format_system_telemetry_speech(metrics)
        return reply, MascotExpression.KEREN, event

    return None
