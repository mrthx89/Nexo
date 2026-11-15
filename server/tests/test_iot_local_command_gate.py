"""Tests for the pre-flight local-command gate (`looks_like_local_command`).

The gate is the real defense against the Hermes LLM "absorbing" a device
command: any utterance that carries a strong device/IoT signal must be routed to
the local parser and never forwarded to the LLM, even for phrasings the exact
keyword matcher has not yet covered.
"""

import pytest

from nexo_server.iot import looks_like_local_command


@pytest.mark.parametrize(
    "text",
    [
        "Nexo, buka settingan IoT",
        "Nexo, matikan semua lampu",
        "Nexo, nyalakan semua lampu",
        "buka settingan",
        "matikan lampu",
        "nyalakan lampu philips",
        "cek status server",
        "tampilkan smart home",
        "atur suhu ac",
        "nyalakan AC kamar",
        "tampilkan menu iot",
    ],
)
def test_device_commands_are_gated(text):
    assert looks_like_local_command(text) is True


@pytest.mark.parametrize(
    "text",
    [
        "Nexo, buka email",
        "Nexo, cek cuaca hari ini",
        "buka aplikasi whatsapp",
        "apa kabar",
        "halo nexo",
        "nexo saja",
        "terima kasih banyak",
        "siapa nama kamu",
    ],
)
def test_generic_tasks_are_not_gated(text):
    # Generic LLM tasks must NOT be swallowed by the local gate.
    assert looks_like_local_command(text) is False


def test_gate_requires_verb_and_domain():
    # A bare wakeword alone (no verb + device domain) must not trip the gate.
    assert looks_like_local_command("Nexo") is False
    assert looks_like_local_command("halo") is False
    # A device domain word with no action verb must not trip the gate.
    assert looks_like_local_command("lampu") is False
    assert looks_like_local_command("settingan iot") is False
