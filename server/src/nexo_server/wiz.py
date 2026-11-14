"""Philips WiZ Smart Lighting Controller for Nexo Dedicated Appliance.

Integrates local UDP protocol (port 38899) and pywizlight with Nexo's
voice pipeline, WebSocket IoT bus, and Android relay gateway.

Author: Yanto Hariyono ({YH Dev})
Motto: "Berkarya dengan hati @ 2026"
"""

import asyncio
from dataclasses import dataclass, field
import json
import logging
import os
import re
import socket
import time
from typing import Any, Dict, List, Optional, Tuple, Union

try:
    from pywizlight import PilotBuilder, wizlight
    from pywizlight.exceptions import WizLightConnectionError, WizLightTimeOutError
    from pywizlight.scenes import SCENES as PYWIZ_SCENES
    _PYWIZ_AVAILABLE = True
except ImportError:  # pragma: no cover
    _PYWIZ_AVAILABLE = False
    PYWIZ_SCENES = {}

logger = logging.getLogger(__name__)

WIZ_UDP_PORT = 38899

# Color name mapping to RGB tuple or Kelvin temperature
COLOR_MAP: Dict[str, Union[Tuple[int, int, int], int]] = {
    # Indonesian primary & secondary colors -> RGB
    "merah": (255, 0, 0),
    "hijau": (0, 255, 0),
    "biru": (0, 110, 255),
    "kuning": (255, 215, 0),
    "ungu": (180, 0, 255),
    "oranye": (255, 120, 0),
    "jingga": (255, 120, 0),
    "merah muda": (255, 105, 180),
    "pink": (255, 105, 180),
    "sian": (0, 255, 255),
    "cyan": (0, 255, 255),
    "emas": (255, 190, 40),
    "gold": (255, 190, 40),
    "aqua": (0, 220, 255),
    "toska": (64, 224, 208),
    "tosca": (64, 224, 208),

    # Kelvin Whites (2200K - 6500K)
    "putih hangat": 2700,
    "warm white": 2700,
    "hangat": 2700,
    "putih natural": 4000,
    "natural white": 4000,
    "putih": 4500,
    "white": 4500,
    "putih terang": 5000,
    "daylight": 5500,
    "siang": 5500,
    "putih sejuk": 6500,
    "cool white": 6500,
    "sejuk": 6500,
}

# WiZ Standard Scene Name to Scene ID
SCENE_MAP: Dict[str, int] = {
    "ocean": 1,
    "samudra": 1,
    "laut": 1,
    "romance": 2,
    "romantis": 2,
    "sunset": 3,
    "matahari terbenam": 3,
    "senja": 3,
    "party": 4,
    "pesta": 4,
    "fireplace": 5,
    "api unggun": 5,
    "cozy": 6,
    "nyaman": 6,
    "forest": 7,
    "hutan": 7,
    "pastel": 8,
    "wake up": 9,
    "bangun": 9,
    "bedtime": 10,
    "waktu tidur": 10,
    "warm white": 11,
    "daylight": 12,
    "cool white": 13,
    "night light": 14,
    "lampu tidur": 14,
    "tidur": 14,
    "malam": 14,
    "focus": 15,
    "fokus": 15,
    "kerja": 15,
    "relax": 16,
    "santai": 16,
    "istirahat": 16,
    "true colors": 17,
    "tv": 18,
    "candlelight": 29,
    "lilin": 29,
    "golden white": 30,
}


@dataclass
class WizBulbConfig:
    """Configured or discovered WiZ light bulb."""
    ip: str
    name: str = "Lampu Philips WiZ"
    mac: Optional[str] = None
    last_seen: float = 0.0


@dataclass
class WizState:
    """Current in-memory state of a WiZ bulb."""
    state: bool = False
    dimming: int = 100
    temp: Optional[int] = None
    rgb: Optional[Tuple[int, int, int]] = None
    scene_id: Optional[int] = None
    scene_name: Optional[str] = None
    last_updated: float = field(default_factory=time.time)

    def to_dict(self) -> Dict[str, Any]:
        return {
            "state": self.state,
            "status_str": "ON" if self.state else "OFF",
            "dimming": self.dimming,
            "temp": self.temp,
            "rgb": list(self.rgb) if self.rgb else None,
            "scene_id": self.scene_id,
            "scene_name": self.scene_name,
            "last_updated": self.last_updated,
        }


class WizController:
    """High-level WiZ Smart Lighting manager."""

    def __init__(self, default_ip: Optional[str] = None):
        self.bulbs: Dict[str, WizBulbConfig] = {}
        self.states: Dict[str, WizState] = {}
        self.default_ip: Optional[str] = default_ip or os.getenv("NEXO_WIZ_IP")
        if self.default_ip:
            self.register_bulb(self.default_ip, "Lampu Philips WiZ")

    def register_bulb(self, ip: str, name: str = "Lampu Philips WiZ", mac: Optional[str] = None) -> WizBulbConfig:
        """Register a known bulb IP."""
        clean_ip = ip.strip()
        config = WizBulbConfig(ip=clean_ip, name=name, mac=mac, last_seen=time.time())
        self.bulbs[clean_ip] = config
        if clean_ip not in self.states:
            self.states[clean_ip] = WizState()
        if not self.default_ip:
            self.default_ip = clean_ip
        return config

    def get_target_ip(self, ip: Optional[str] = None) -> Optional[str]:
        """Resolve the bulb IP to target."""
        if ip and ip.strip():
            return ip.strip()
        if self.default_ip:
            return self.default_ip
        if self.bulbs:
            return next(iter(self.bulbs.keys()))
        return None

    def build_udp_packet(self, method: str, params: Optional[Dict[str, Any]] = None) -> bytes:
        """Build standard WiZ JSON datagram."""
        payload = {
            "method": method,
            "env": "pro",
            "params": params or {}
        }
        return json.dumps(payload).encode("utf-8")

    async def send_udp_raw(
        self,
        ip: str,
        method: str,
        params: Optional[Dict[str, Any]] = None,
        timeout: float = 1.5,
    ) -> Optional[Dict[str, Any]]:
        """Send a direct UDP packet to a WiZ bulb with timeout."""
        def _send():
            sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            sock.settimeout(timeout)
            packet = self.build_udp_packet(method, params)
            try:
                sock.sendto(packet, (ip, WIZ_UDP_PORT))
                data, _ = sock.recvfrom(2048)
                return json.loads(data.decode("utf-8"))
            except (socket.timeout, OSError) as exc:
                logger.debug("WiZ UDP to %s (%s) timed out or failed: %s", ip, method, exc)
                return None
            finally:
                sock.close()

        try:
            return await asyncio.to_thread(_send)
        except Exception as exc:
            logger.warning("Error during WiZ UDP communication to %s: %s", ip, exc)
            return None

    async def turn_on(
        self,
        ip: Optional[str] = None,
        dimming: Optional[int] = None,
        temp: Optional[int] = None,
        rgb: Optional[Tuple[int, int, int]] = None,
        scene: Optional[Union[str, int]] = None,
    ) -> Dict[str, Any]:
        """Turn on the Philips WiZ bulb with optional parameters."""
        target_ip = self.get_target_ip(ip)
        params: Dict[str, Any] = {"state": True}

        if dimming is not None:
            clamped_dim = max(10, min(100, int(dimming)))
            params["dimming"] = clamped_dim

        if temp is not None:
            clamped_temp = max(2200, min(6500, int(temp)))
            params["temp"] = clamped_temp

        if rgb is not None:
            r, g, b = rgb
            params["r"] = max(0, min(255, int(r)))
            params["g"] = max(0, min(255, int(g)))
            params["b"] = max(0, min(255, int(b)))

        scene_id = None
        scene_name = None
        if scene is not None:
            if isinstance(scene, int):
                scene_id = scene
            elif isinstance(scene, str):
                scene_clean = scene.lower().strip()
                scene_id = SCENE_MAP.get(scene_clean)
            if scene_id is not None:
                params["sceneId"] = scene_id
                scene_name = str(scene)

        # Update cached state
        if target_ip:
            st = self.states.setdefault(target_ip, WizState())
            st.state = True
            if "dimming" in params:
                st.dimming = params["dimming"]
            if "temp" in params:
                st.temp = params["temp"]
                st.rgb = None
                st.scene_id = None
            if "r" in params:
                st.rgb = (params["r"], params["g"], params["b"])
                st.temp = None
                st.scene_id = None
            if scene_id is not None:
                st.scene_id = scene_id
                st.scene_name = scene_name
            st.last_updated = time.time()

        # Send via UDP if IP is known
        result = None
        if target_ip:
            result = await self.send_udp_raw(target_ip, "setPilot", params)

        relay_packet = {
            "type": "iot_relay",
            "target": "wiz",
            "action": "turn_on",
            "ip": target_ip,
            "params": params,
        }

        return {
            "success": True,
            "ip": target_ip,
            "state": "on",
            "params": params,
            "raw_result": result,
            "relay_packet": relay_packet,
        }

    async def turn_off(self, ip: Optional[str] = None) -> Dict[str, Any]:
        """Turn off the Philips WiZ bulb."""
        target_ip = self.get_target_ip(ip)
        params = {"state": False}

        if target_ip:
            st = self.states.setdefault(target_ip, WizState())
            st.state = False
            st.last_updated = time.time()

        result = None
        if target_ip:
            result = await self.send_udp_raw(target_ip, "setPilot", params)

        relay_packet = {
            "type": "iot_relay",
            "target": "wiz",
            "action": "turn_off",
            "ip": target_ip,
            "params": params,
        }

        return {
            "success": True,
            "ip": target_ip,
            "state": "off",
            "params": params,
            "raw_result": result,
            "relay_packet": relay_packet,
        }

    async def set_brightness(self, percent: int, ip: Optional[str] = None) -> Dict[str, Any]:
        """Set brightness dimming percentage (10 - 100%)."""
        clamped = max(10, min(100, int(percent)))
        return await self.turn_on(ip=ip, dimming=clamped)

    async def set_color(self, color: Union[str, Tuple[int, int, int]], ip: Optional[str] = None) -> Dict[str, Any]:
        """Set bulb color from name or RGB tuple."""
        if isinstance(color, (tuple, list)) and len(color) == 3:
            rgb_tuple = (int(color[0]), int(color[1]), int(color[2]))
            return await self.turn_on(ip=ip, rgb=rgb_tuple)

        clean = str(color).lower().strip()
        val = COLOR_MAP.get(clean)
        if isinstance(val, int):
            return await self.turn_on(ip=ip, temp=val)
        elif isinstance(val, tuple):
            return await self.turn_on(ip=ip, rgb=val)
        else:
            # Fallback to white natural if unrecognized
            return await self.turn_on(ip=ip, temp=4000)

    async def set_scene(self, scene: Union[str, int], ip: Optional[str] = None) -> Dict[str, Any]:
        """Activate a WiZ scene (e.g. 'focus', 'relax', 'candlelight')."""
        return await self.turn_on(ip=ip, scene=scene)

    async def get_status(self, ip: Optional[str] = None) -> Dict[str, Any]:
        """Query current bulb status."""
        target_ip = self.get_target_ip(ip)
        if not target_ip:
            return {
                "available": False,
                "ip": None,
                "message": "Belum ada IP lampu Philips WiZ yang terdaftar.",
            }

        reply = await self.send_udp_raw(target_ip, "getPilot", {})
        st = self.states.setdefault(target_ip, WizState())

        if reply and "result" in reply:
            res = reply["result"]
            st.state = bool(res.get("state", False))
            if "dimming" in res:
                st.dimming = int(res["dimming"])
            if "temp" in res:
                st.temp = int(res["temp"])
            if "r" in res and "g" in res and "b" in res:
                st.rgb = (res["r"], res["g"], res["b"])
            if "sceneId" in res and res["sceneId"] != 0:
                st.scene_id = int(res["sceneId"])
            st.last_updated = time.time()
            return {
                "available": True,
                "ip": target_ip,
                "live": True,
                "data": st.to_dict(),
            }

        return {
            "available": True,
            "ip": target_ip,
            "live": False,
            "cached": True,
            "data": st.to_dict(),
        }

    async def discover_bulbs(self, timeout: float = 2.5, broadcast_address: str = "255.255.255.255") -> List[Dict[str, Any]]:
        """Broadcast UDP packet on local LAN to discover active WiZ bulbs."""
        discovered: List[Dict[str, Any]] = []

        def _broadcast():
            sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            sock.settimeout(timeout)

            # Registration / System config probe message
            msg = json.dumps({"method": "getSystemConfig", "params": {}}).encode("utf-8")
            try:
                sock.sendto(msg, (broadcast_address, WIZ_UDP_PORT))
            except Exception as e:
                logger.debug("Broadcast send error: %s", e)

            start = time.time()
            seen_ips = set()
            while time.time() - start < timeout:
                try:
                    data, addr = sock.recvfrom(2048)
                    bulb_ip = addr[0]
                    if bulb_ip in seen_ips:
                        continue
                    seen_ips.add(bulb_ip)
                    payload = json.loads(data.decode("utf-8"))
                    res = payload.get("result", {})
                    mac = res.get("mac")
                    bulb_name = f"Philips WiZ ({bulb_ip})"
                    discovered.append({
                        "ip": bulb_ip,
                        "mac": mac,
                        "name": bulb_name,
                        "data": res,
                    })
                except socket.timeout:
                    break
                except Exception as exc:
                    logger.debug("Error receiving discovery packet: %s", exc)
                    break
            sock.close()
            return discovered

        results = await asyncio.to_thread(_broadcast)
        for d in results:
            self.register_bulb(d["ip"], d["name"], d.get("mac"))
        return results


# Global singleton instance
wiz_controller = WizController()
