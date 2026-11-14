"""Polytron Smart Neuva Pro AC Controller for Nexo Dedicated Appliance.

Integrates Polytron IoT23 / Fira Cloud API with Nexo's voice pipeline,
telemetry monitoring, and Android relay gateway.

Supports:
- Remote climate control (Power, Temp 16-32°C, Mode, Fan Speed, Swing)
- Real-time electrical telemetry (Active Power Watt, Energy kWh, Room Temperature)
- Secure WhatsApp OTP authentication persistence (~/.config/nexo-auth/polytron.json)

Author: Yanto Hariyono ({YH Dev})
Motto: "Berkarya dengan hati @ 2026"
"""

import asyncio
import base64
import json
import logging
import os
import urllib.parse
import urllib.request
import ssl
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

logger = logging.getLogger(__name__)

# Cloud API Endpoints
AUTH_BASE = "https://ism.fira.id"
USRSAPP_BASE = "https://usrsapp.iot.polytron.co.id"
DEVSAPP_BASE = "https://devsapp.iot.polytron.co.id"

# Gateway Basic Auth Tokens
GATEWAY_AUTH_FIRA = base64.b64encode(b"Smart_home:Smart_home132").decode()
GATEWAY_AUTH_USRSAPP = base64.b64encode(b"usrsappUser:usrsappPass").decode()
GATEWAY_AUTH_DEVSAPP = base64.b64encode(b"devsappUser:devsappPass").decode()

# HVAC Mappings
HVAC_MODE_TO_POLYTRON: Dict[str, int] = {
    "fan_only": 1,
    "fan": 1,
    "kipas": 1,
    "cool": 2,
    "dingin": 2,
    "dry": 3,
    "kering": 3,
    "auto": 4,
    "otomatis": 4,
    "smart": 4,
}

POLYTRON_TO_HVAC_MODE: Dict[int, str] = {
    1: "fan_only",
    2: "cool",
    3: "dry",
    4: "auto",
}

FAN_SPEED_TO_POLYTRON: Dict[str, int] = {
    "low": 0,
    "rendah": 0,
    "pelan": 0,
    "auto": 1,
    "otomatis": 1,
    "medium": 2,
    "sedang": 2,
    "high": 3,
    "tinggi": 3,
    "kencang": 3,
}

POLYTRON_TO_FAN_SPEED: Dict[int, str] = {
    0: "low",
    1: "auto",
    2: "medium",
    3: "high",
}


@dataclass
class PolytronAcState:
    serial_number: str
    label: str = "Polytron Smart Neuva Pro"
    power: bool = False
    mode: str = "cool"
    mode_code: int = 2
    target_temp: int = 24
    fan_speed: str = "auto"
    fan_speed_code: int = 1
    swing: bool = False
    room_temp: Optional[float] = None
    power_watt: float = 0.0
    energy_kwh: float = 0.0
    rssi: int = 0
    online: bool = False
    updated_at: float = 0.0

    def to_dict(self) -> Dict[str, Any]:
        return {
            "serial_number": self.serial_number,
            "label": self.label,
            "power": self.power,
            "mode": self.mode,
            "mode_code": self.mode_code,
            "target_temp": self.target_temp,
            "fan_speed": self.fan_speed,
            "fan_speed_code": self.fan_speed_code,
            "swing": self.swing,
            "room_temp": self.room_temp,
            "power_watt": self.power_watt,
            "energy_kwh": self.energy_kwh,
            "rssi": self.rssi,
            "online": self.online,
            "updated_at": self.updated_at,
        }


class PolytronAcController:
    """Controller for Polytron Smart Neuva Pro Air Conditioners."""

    def __init__(self, config_path: Optional[str] = None):
        self.config_path = Path(
            config_path
            or os.environ.get("POLYTRON_AUTH_FILE")
            or os.path.expanduser("~/.config/nexo-auth/polytron.json")
        )
        self.phone: str = ""
        self.token: str = ""
        self.user_b: str = ""
        self.default_serial: str = ""
        self.devices: List[Dict[str, Any]] = []
        self.states: Dict[str, PolytronAcState] = {}
        self.ssl_ctx = ssl.create_default_context()
        self._load_config()

    def _load_config(self) -> None:
        """Load persisted phone, token, and serial from credentials file."""
        if not self.config_path.exists():
            return
        try:
            with open(self.config_path, "r", encoding="utf-8") as f:
                data = json.load(f)
                self.phone = data.get("phone", "")
                self.token = data.get("token", "")
                self.user_b = data.get("user_b", "")
                self.default_serial = data.get("default_serial", "")
        except Exception as e:
            logger.warning("Failed to load Polytron credentials from %s: %s", self.config_path, e)

    def _save_config(self) -> None:
        """Persist credentials with strict file permissions (0600)."""
        try:
            self.config_path.parent.mkdir(parents=True, exist_ok=True)
            self.config_path.parent.chmod(0o700)
            data = {
                "phone": self.phone,
                "token": self.token,
                "user_b": self.user_b,
                "default_serial": self.default_serial,
            }
            tmp = self.config_path.with_suffix(".tmp")
            with open(tmp, "w", encoding="utf-8") as f:
                json.dump(data, f, indent=2)
            tmp.chmod(0o600)
            tmp.replace(self.config_path)
        except Exception as e:
            logger.error("Failed to save Polytron credentials: %s", e)

    def is_authenticated(self) -> bool:
        return bool(self.phone and self.token)

    # --- Authentication Methods ---

    def request_wa_otp(self, phone: str) -> Dict[str, Any]:
        """Request WhatsApp OTP from Polytron / Fira ISM service."""
        clean_phone = phone.strip().lstrip("+")
        url = f"{AUTH_BASE}/api"
        payload = {
            "username": clean_phone,
            "app_name": "smarthome",
            "app_action": "wa_otp",
            "app_language": "1",
            "sms_header": "<#> Polytron IOT",
            "sms_footer": "",
        }
        data = urllib.parse.urlencode(payload).encode("utf-8")
        req = urllib.request.Request(
            url,
            data=data,
            headers={
                "Authorization": f"Basic {GATEWAY_AUTH_FIRA}",
                "Content-Type": "application/x-www-form-urlencoded",
                "User-Agent": "okhttp/4.9.0",
            },
        )
        try:
            with urllib.request.urlopen(req, timeout=12) as resp:
                result = json.loads(resp.read().decode("utf-8"))
                if result.get("error") == "0":
                    self.phone = clean_phone
                return result
        except Exception as e:
            logger.error("Error requesting WhatsApp OTP for %s: %s", clean_phone, e)
            return {"error": "1", "message": str(e)}

    def verify_otp(self, phone: str, otp_code: str) -> Dict[str, Any]:
        """Verify WhatsApp OTP and store session token."""
        clean_phone = phone.strip().lstrip("+")
        url = f"{AUTH_BASE}/api"
        payload = {
            "username": clean_phone,
            "otp_code": otp_code.strip(),
            "app_name": "smarthome",
            "app_action": "verify_otp",
            "app_language": "1",
        }
        data = urllib.parse.urlencode(payload).encode("utf-8")
        req = urllib.request.Request(
            url,
            data=data,
            headers={
                "Authorization": f"Basic {GATEWAY_AUTH_FIRA}",
                "Content-Type": "application/x-www-form-urlencoded",
                "User-Agent": "okhttp/4.9.0",
            },
        )
        try:
            with urllib.request.urlopen(req, timeout=12) as resp:
                result = json.loads(resp.read().decode("utf-8"))
                token = result.get("token") or result.get("data", {}).get("token")
                if token:
                    self.phone = clean_phone
                    self.token = token
                    self.user_b = result.get("userB", "")
                    self._save_config()
                return result
        except Exception as e:
            logger.error("Error verifying OTP for %s: %s", clean_phone, e)
            return {"error": "1", "message": str(e)}

    # --- Device Discovery & State ---

    def fetch_devices(self) -> List[Dict[str, Any]]:
        """Fetch list of smart appliances paired with this account."""
        if not self.is_authenticated():
            return []
        query = urllib.parse.urlencode({"uname": self.phone, "token": self.token})
        url = f"{USRSAPP_BASE}/users/home?{query}"
        req = urllib.request.Request(
            url,
            headers={
                "Authorization": f"Basic {GATEWAY_AUTH_USRSAPP}",
                "Accept": "*/*",
                "User-Agent": "okhttp/4.9.0",
            },
        )
        try:
            with urllib.request.urlopen(req, context=self.ssl_ctx, timeout=12) as resp:
                data = json.loads(resp.read().decode("utf-8"))
                devs = data.get("message", {}).get("devices", [])
                self.devices = devs
                if devs and not self.default_serial:
                    self.default_serial = devs[0].get("serialNumber", "")
                    self._save_config()
                return devs
        except Exception as e:
            logger.error("Failed to fetch Polytron devices: %s", e)
            return []

    def fetch_live_state(self, serial: Optional[str] = None, tz: str = "Asia/Jakarta") -> Optional[PolytronAcState]:
        """Fetch real-time state, sensors, and power draw for an AC unit."""
        sn = serial or self.default_serial
        if not sn or not self.is_authenticated():
            return self.states.get(sn) if sn else None

        query = urllib.parse.urlencode({
            "uname": self.phone,
            "token": self.token,
            "sn": sn,
            "tz": tz,
        })
        url = f"{DEVSAPP_BASE}/devices/home?{query}"
        req = urllib.request.Request(
            url,
            headers={
                "Authorization": f"Basic {GATEWAY_AUTH_DEVSAPP}",
                "Accept": "*/*",
                "User-Agent": "okhttp/4.9.0",
            },
        )
        try:
            with urllib.request.urlopen(req, context=self.ssl_ctx, timeout=12) as resp:
                raw_json = json.loads(resp.read().decode("utf-8"))
                data = raw_json.get("data", {})

                power = False
                mode_code = 2
                target_temp = 24
                fan_code = 1
                swing = False

                for item in data.get("con", []):
                    e = str(item.get("enum"))
                    d = str(item.get("data", ""))
                    if e == "0":
                        power = (d == "1")
                    elif e == "1":
                        mode_code = int(d) if d.isdigit() else 2
                    elif e == "3":
                        target_temp = int(d) if d.isdigit() else 24
                    elif e == "4":
                        fan_code = int(d) if d.isdigit() else 1
                    elif e == "5":
                        swing = (d == "1")

                room_temp = None
                power_watt = 0.0
                energy_kwh = 0.0

                for item in data.get("sen", []):
                    e = str(item.get("enum"))
                    d = str(item.get("data", ""))
                    if e == "0" and d:
                        try:
                            room_temp = float(d) / 10.0 if len(d) == 3 else float(d)
                        except ValueError:
                            pass
                    elif e == "1" and d:
                        try:
                            power_watt = float(d)
                        except ValueError:
                            pass
                    elif e == "2" and d:
                        try:
                            energy_kwh = float(d) / 1000.0
                        except ValueError:
                            pass

                rssi = 0
                for item in data.get("inf", []):
                    if str(item.get("enum")) == "7":
                        try:
                            rssi = int(item.get("data", 0))
                        except ValueError:
                            pass

                label = "Polytron Smart Neuva Pro"
                for dev in self.devices:
                    if dev.get("serialNumber") == sn:
                        label = dev.get("label", label)
                        break

                state = PolytronAcState(
                    serial_number=sn,
                    label=label,
                    power=power,
                    mode=POLYTRON_TO_HVAC_MODE.get(mode_code, "cool"),
                    mode_code=mode_code,
                    target_temp=target_temp,
                    fan_speed=POLYTRON_TO_FAN_SPEED.get(fan_code, "auto"),
                    fan_speed_code=fan_code,
                    swing=swing,
                    room_temp=room_temp,
                    power_watt=power_watt,
                    energy_kwh=energy_kwh,
                    rssi=rssi,
                    online=True,
                    updated_at=asyncio.get_event_loop().time() if asyncio.get_event_loop().is_running() else 0.0,
                )
                self.states[sn] = state
                return state
        except Exception as e:
            logger.error("Failed to fetch Polytron AC state for %s: %s", sn, e)
            st = self.states.get(sn)
            if st:
                st.online = False
            return st

    # --- Control Methods ---

    def _send_control(self, enum_id: int, dtyp: int, data_val: int, serial: Optional[str] = None) -> bool:
        """Send raw parameter control to Polytron Smart AC."""
        sn = serial or self.default_serial
        if not sn or not self.is_authenticated():
            logger.warning("Cannot send control: unauthenticated or no serial.")
            return False

        url = f"{DEVSAPP_BASE}/devices/control"
        payload = {
            "uname": self.phone,
            "token": self.token,
            "sn": sn,
            "enum": str(enum_id),
            "dtyp": str(dtyp),
            "data": str(data_val),
        }
        data = urllib.parse.urlencode(payload).encode("utf-8")
        req = urllib.request.Request(
            url,
            data=data,
            headers={
                "Authorization": f"Basic {GATEWAY_AUTH_DEVSAPP}",
                "Content-Type": "application/x-www-form-urlencoded",
                "Accept": "*/*",
                "User-Agent": "okhttp/4.9.0",
            },
        )
        try:
            with urllib.request.urlopen(req, context=self.ssl_ctx, timeout=12) as resp:
                res = json.loads(resp.read().decode("utf-8"))
                return res.get("errc") == "0"
        except Exception as e:
            logger.error("Error sending control to Polytron AC %s: %s", sn, e)
            return False

    async def async_turn_on(self, serial: Optional[str] = None) -> bool:
        loop = asyncio.get_running_loop()
        ok = await loop.run_in_executor(None, self._send_control, 0, 2, 1, serial)
        sn = serial or self.default_serial
        if ok and sn in self.states:
            self.states[sn].power = True
        return ok

    async def async_turn_off(self, serial: Optional[str] = None) -> bool:
        loop = asyncio.get_running_loop()
        ok = await loop.run_in_executor(None, self._send_control, 0, 2, 0, serial)
        sn = serial or self.default_serial
        if ok and sn in self.states:
            self.states[sn].power = False
        return ok

    async def async_set_temperature(self, temp: int, serial: Optional[str] = None) -> bool:
        temp_clamped = max(16, min(32, int(temp)))
        loop = asyncio.get_running_loop()
        ok = await loop.run_in_executor(None, self._send_control, 3, 3, temp_clamped, serial)
        sn = serial or self.default_serial
        if ok and sn in self.states:
            self.states[sn].target_temp = temp_clamped
            self.states[sn].power = True
        return ok

    async def async_set_mode(self, mode: str, serial: Optional[str] = None) -> bool:
        mode_code = HVAC_MODE_TO_POLYTRON.get(mode.lower().strip(), 2)
        loop = asyncio.get_running_loop()
        ok = await loop.run_in_executor(None, self._send_control, 1, 3, mode_code, serial)
        sn = serial or self.default_serial
        if ok and sn in self.states:
            self.states[sn].mode = POLYTRON_TO_HVAC_MODE.get(mode_code, "cool")
            self.states[sn].mode_code = mode_code
            self.states[sn].power = True
        return ok

    async def async_set_fan_speed(self, speed: str, serial: Optional[str] = None) -> bool:
        speed_code = FAN_SPEED_TO_POLYTRON.get(speed.lower().strip(), 1)
        loop = asyncio.get_running_loop()
        ok = await loop.run_in_executor(None, self._send_control, 4, 3, speed_code, serial)
        sn = serial or self.default_serial
        if ok and sn in self.states:
            self.states[sn].fan_speed = POLYTRON_TO_FAN_SPEED.get(speed_code, "auto")
            self.states[sn].fan_speed_code = speed_code
        return ok

    async def async_set_swing(self, vertical: bool, serial: Optional[str] = None) -> bool:
        swing_val = 1 if vertical else 0
        loop = asyncio.get_running_loop()
        ok = await loop.run_in_executor(None, self._send_control, 5, 2, swing_val, serial)
        sn = serial or self.default_serial
        if ok and sn in self.states:
            self.states[sn].swing = vertical
        return ok

    async def async_refresh_state(self, serial: Optional[str] = None) -> Optional[PolytronAcState]:
        loop = asyncio.get_running_loop()
        return await loop.run_in_executor(None, self.fetch_live_state, serial)


# Singleton controller instance
polytron_ac_controller = PolytronAcController()
