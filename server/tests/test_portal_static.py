"""Static regression test for Nexo Portal HTML dashboard."""

from nexo_server.portal import render_portal_html, AVAILABLE_EXPRESSIONS, load_mascot_svg


def test_render_portal_html_content_and_structure():
    html = render_portal_html()
    assert "<!DOCTYPE html>" in html
    assert "Nexo Dedicated Appliance" in html
    assert "Yanto Hariyono" in html
    assert "Berkarya dengan hati @ 2026" in html
    assert "Hermes Agent" in html
    assert "Philips WiZ" in html
    assert "Polytron" in html
    assert "LiveWaveformView" in html
    assert "Silero VAD" in html
    assert "MediaSession" in html
    assert "/ws/appliance" in html
    assert "/iot/status" in html
    assert "/iot/control" in html
    assert "/iot/ac/status" in html
    assert "nexo ac" in html
    assert "nexo iot" in html


def test_mascot_svg_loader():
    for expr in AVAILABLE_EXPRESSIONS:
        svg = load_mascot_svg(expr)
        assert "<svg" in svg
        assert "</svg>" in svg
