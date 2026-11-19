"""Nexo Portal: Interactive Web Landing & Diagnostic Dashboard.

Author: Yanto Hariyono ({YH Dev})
Motto: "Berkarya dengan hati @ 2026"
"""

from pathlib import Path
from typing import Dict

# Mascot SVGs directory
MASCOT_DIR = Path(__file__).resolve().parent.parent.parent.parent / "resource" / "imoticon" / "svg"

AVAILABLE_EXPRESSIONS = [
    "senang", "keren", "sangat-senang", "cinta", "tertawa",
    "netral", "bingung", "mengantuk", "sleeping", "jahil",
    "malu", "terkejut", "sedih", "takut", "marah", "menangis"
]


def load_mascot_svg(expression: str = "senang") -> str:
    """Load an SVG icon from the mascot resource folder with fallback."""
    clean_expr = expression.lower().strip().replace("_", "-")
    target_file = MASCOT_DIR / f"{clean_expr}.svg"
    if not target_file.exists():
        target_file = MASCOT_DIR / "senang.svg"
    if target_file.exists():
        try:
            return target_file.read_text(encoding="utf-8")
        except Exception:
            pass
    return '<svg width="200" height="200" viewBox="0 0 200 200"><circle cx="100" cy="100" r="80" fill="#38bdf8"/></svg>'


def render_portal_html() -> str:
    """Generate the modern, high-contrast, zero-slop portal dashboard."""
    initial_svg = load_mascot_svg("senang")
    
    html = f"""<!DOCTYPE html>
<html lang="id">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Nexo Dedicated Appliance — Ambient AI Companion</title>
    <link rel="icon" href="/mascot/senang.svg" type="image/svg+xml">
    <style>
        :root {{
            --bg-base: #06090f;
            --bg-surface: #0c1322;
            --bg-surface-elevated: #111b30;
            --border-subtle: #1e293b;
            --border-bright: #334155;
            --text-main: #f8fafc;
            --text-muted: #94a3b8;
            --text-dim: #64748b;
            --accent-cyan: #38bdf8;
            --accent-silver: #cbd5e1;
            --accent-emerald: #10b981;
            --accent-amber: #f59e0b;
            --accent-coral: #ff5252;
            --font-stack: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
            --font-mono: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
        }}
        * {{ box-sizing: border-box; margin: 0; padding: 0; }}
        body {{
            background-color: var(--bg-base);
            color: var(--text-main);
            font-family: var(--font-stack);
            line-height: 1.6;
            min-height: 100vh;
            padding: 32px 20px 60px;
            display: flex;
            flex-direction: column;
            align-items: center;
        }}
        .container {{
            width: 100%;
            max-width: 1120px;
        }}
        header {{
            display: flex;
            justify-content: space-between;
            align-items: center;
            padding-bottom: 24px;
            border-bottom: 1px solid var(--border-subtle);
            margin-bottom: 36px;
        }}
        .brand {{
            display: flex;
            align-items: center;
            gap: 14px;
            text-decoration: none;
            color: var(--text-main);
        }}
        .brand-logo {{
            width: 44px;
            height: 44px;
            display: flex;
            align-items: center;
            justify-content: center;
        }}
        .brand-logo svg {{
            width: 100%;
            height: 100%;
        }}
        .brand-text h1 {{
            font-size: 20px;
            font-weight: 700;
            letter-spacing: -0.02em;
        }}
        .brand-text p {{
            font-size: 12px;
            color: var(--text-muted);
        }}
        .status-badge {{
            display: inline-flex;
            align-items: center;
            gap: 8px;
            background: var(--bg-surface-elevated);
            border: 1px solid var(--border-bright);
            padding: 6px 14px;
            border-radius: 9999px;
            font-size: 13px;
            font-weight: 500;
            color: var(--text-main);
        }}
        .pulse-dot {{
            width: 8px;
            height: 8px;
            background: var(--accent-emerald);
            border-radius: 50%;
            box-shadow: 0 0 10px var(--accent-emerald);
            animation: pulse 2s infinite ease-in-out;
        }}
        @keyframes pulse {{
            0%, 100% {{ opacity: 1; transform: scale(1); }}
            50% {{ opacity: 0.4; transform: scale(0.85); }}
        }}
        
        .hero {{
            text-align: center;
            padding: 24px 0 36px;
        }}
        .hero h2 {{
            font-size: 42px;
            font-weight: 800;
            letter-spacing: -0.03em;
            line-height: 1.15;
            margin-bottom: 16px;
            background: linear-gradient(135deg, #ffffff 0%, #cbd5e1 50%, #7dd3fc 100%);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
        }}
        .hero p.lead {{
            font-size: 17px;
            color: var(--text-muted);
            max-width: 740px;
            margin: 0 auto 24px;
        }}
        .motto-banner {{
            display: inline-flex;
            align-items: center;
            flex-wrap: wrap;
            justify-content: center;
            gap: 10px;
            font-size: 13px;
            color: var(--accent-silver);
            background: var(--bg-surface);
            border: 1px solid var(--border-subtle);
            padding: 8px 18px;
            border-radius: 8px;
        }}
        .motto-banner strong {{
            color: var(--accent-cyan);
        }}

        /* Mascot Visualizer Card */
        .stage-card {{
            background: var(--bg-surface);
            border: 1px solid var(--border-subtle);
            border-radius: 16px;
            padding: 36px 24px;
            margin-bottom: 36px;
            text-align: center;
            position: relative;
            overflow: hidden;
        }}
        .stage-card::before {{
            content: '';
            position: absolute;
            top: 0; left: 50%;
            transform: translateX(-50%);
            width: 380px;
            height: 220px;
            background: radial-gradient(ellipse at center, rgba(56, 189, 248, 0.14) 0%, transparent 70%);
            pointer-events: none;
        }}
        .mascot-display {{
            width: 220px;
            height: 220px;
            margin: 0 auto 20px;
            transition: transform 0.25s ease;
        }}
        .mascot-display:hover {{
            transform: scale(1.04);
        }}
        .mascot-display svg {{
            width: 100%;
            height: 100%;
            filter: drop-shadow(0 10px 25px rgba(0,0,0,0.5));
        }}
        .stage-meta {{
            font-size: 15px;
            font-weight: 600;
            color: var(--text-main);
            margin-bottom: 12px;
        }}
        .stage-submeta {{
            font-size: 13px;
            color: var(--text-dim);
            margin-bottom: 18px;
        }}
        .expression-picker {{
            display: flex;
            flex-wrap: wrap;
            justify-content: center;
            gap: 8px;
            max-width: 820px;
            margin: 0 auto;
        }}
        .expr-btn {{
            background: var(--bg-surface-elevated);
            color: var(--text-muted);
            border: 1px solid var(--border-subtle);
            padding: 6px 12px;
            border-radius: 8px;
            font-size: 12px;
            font-weight: 500;
            cursor: pointer;
            transition: all 0.15s ease;
            text-transform: capitalize;
        }}
        .expr-btn:hover {{
            border-color: var(--accent-cyan);
            color: var(--text-main);
            transform: translateY(-1px);
        }}
        .expr-btn.active {{
            background: #0284c7;
            border-color: var(--accent-cyan);
            color: #ffffff;
            box-shadow: 0 0 12px rgba(56, 189, 248, 0.35);
        }}

        /* Topology & Pillar Grid */
        .grid-3 {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
            gap: 20px;
            margin-bottom: 36px;
        }}
        .card {{
            background: var(--bg-surface);
            border: 1px solid var(--border-subtle);
            border-radius: 12px;
            padding: 24px;
        }}
        .card-header {{
            display: flex;
            align-items: center;
            gap: 10px;
            margin-bottom: 14px;
        }}
        .card-icon {{
            font-size: 22px;
            line-height: 1;
        }}
        .card h3 {{
            font-size: 16px;
            font-weight: 600;
            color: var(--text-main);
        }}
        .card p {{
            font-size: 13px;
            color: var(--text-muted);
            margin-bottom: 16px;
            line-height: 1.5;
        }}
        .card ul {{
            list-style: none;
            padding: 0;
        }}
        .card li {{
            font-size: 12.5px;
            color: var(--text-muted);
            padding: 7px 0;
            border-top: 1px solid rgba(255, 255, 255, 0.04);
            display: flex;
            justify-content: space-between;
            align-items: center;
            gap: 8px;
        }}
        .card li strong {{
            color: var(--text-main);
            font-weight: 500;
            text-align: right;
            word-break: break-word;
        }}

        /* Diagnostics & Endpoints */
        .code-snippet {{
            background: #030712;
            border: 1px solid var(--border-subtle);
            border-radius: 8px;
            padding: 12px 16px;
            font-family: var(--font-mono);
            font-size: 12.5px;
            color: #7dd3fc;
            overflow-x: auto;
            margin-bottom: 16px;
            line-height: 1.55;
        }}
        .endpoint-row {{
            display: flex;
            align-items: center;
            justify-content: space-between;
            padding: 10px 0;
            border-bottom: 1px solid var(--border-subtle);
            font-size: 13px;
            gap: 12px;
        }}
        .endpoint-row:last-child {{ border-bottom: none; }}
        .badge-method {{
            font-family: var(--font-mono);
            font-weight: 700;
            font-size: 11px;
            padding: 2px 7px;
            border-radius: 4px;
            margin-right: 8px;
            display: inline-block;
        }}
        .badge-get {{ background: #064e3b; color: #34d399; }}
        .badge-ws {{ background: #312e81; color: #a5b4fc; }}
        .badge-post {{ background: #78350f; color: #fcd34d; }}

        footer {{
            margin-top: 40px;
            padding-top: 24px;
            border-top: 1px solid var(--border-subtle);
            text-align: center;
            font-size: 13px;
            color: var(--text-dim);
        }}
        footer a {{
            color: var(--accent-cyan);
            text-decoration: none;
        }}
        footer a:hover {{ text-decoration: underline; }}
    </style>
</head>
<body>
    <div class="container">
        <header>
            <a href="/" class="brand">
                <div class="brand-logo">
                    {initial_svg}
                </div>
                <div class="brand-text">
                    <h1>NEXO</h1>
                    <p>Dedicated AI Appliance</p>
                </div>
            </a>
            <div class="status-badge">
                <span class="pulse-dot"></span>
                <span id="gatewayStatusText">Gateway Online</span>
            </div>
        </header>

        <section class="hero">
            <h2>Wujud Fisik Hermes Agent di Meja Kerja Anda</h2>
            <p class="lead">
                Nexo Dedicated Appliance menghadirkan antarmuka fisik otonom bagi asisten AI: layar visual maskot digital 3D yang hidup, pengeras suara alami berlatensi rendah (TTFA &lt; 800ms), deteksi suara on-device Silero VAD, kendali Smart Studio IoT terpadu, serta mikrofon hands-free dengan peredam gema hardware.
            </p>
            <div class="motto-banner">
                <span>Filosofi &amp; Rekayasa:</span>
                <strong>"Berkarya dengan hati @ 2026"</strong>
                <span>&bull;</span>
                <span>Kreator: <strong>Yanto Hariyono ({{YH Dev}})</strong></span>
            </div>
        </section>

        <!-- Interactive Mascot Section -->
        <div class="stage-card">
            <div class="mascot-display" id="mascotContainer">
                {initial_svg}
            </div>
            <div class="stage-meta">
                Mascot Visualizer: <span id="currentExprName" style="color: var(--accent-cyan);">Senang</span> (Nexo Silver)
            </div>
            <div class="stage-submeta">
                Tactile Touch Spring Physics &bull; Damped Harmonic Oscillator &bull; Real-time Gaze Tracking &bull; Ambient Sleep &bull; Pixel Shifting
            </div>
            <div class="expression-picker">
                {" ".join([f'<button class="expr-btn {"active" if e == "senang" else ""}" onclick="switchMascot(\'{e}\')">{e.replace("-", " ")}</button>' for e in AVAILABLE_EXPRESSIONS])}
            </div>
        </div>

        <!-- System Architecture & Technology Pillars Grid -->
        <div class="grid-3">
            <!-- 1. Hermes Core & Multi-Device -->
            <div class="card">
                <div class="card-header">
                    <span class="card-icon">🧠</span>
                    <h3>Pikiran &amp; Penalaran (Hermes Core)</h3>
                </div>
                <p>Gateway server mengunci sesi kanonis ke Hermes Agent CLI persisten untuk keutuhan memori kerja, dengan isolasi mandiri untuk multi-perangkat.</p>
                <ul>
                    <li><span>Hermes Engine</span><strong>Hermes Agent CLI Bridge</strong></li>
                    <li><span>Canonical Session</span><strong>nexo (Terminal Synced 🔒)</strong></li>
                    <li><span>Multi-Device</span><strong>nexo-&lt;device_id&gt; Isolated</strong></li>
                    <li><span>Compaction Guard</span><strong>Anti-Replay SQLite Filter</strong></li>
                    <li><span>Smart Briefing</span><strong>Sapaan Waktu WIB &amp; Pantun</strong></li>
                </ul>
            </div>

            <!-- 2. Pipelined Voice & Biometrics -->
            <div class="card">
                <div class="card-header">
                    <span class="card-icon">🔊</span>
                    <h3>Suara &amp; Biometrik (Pipelined Voice)</h3>
                </div>
                <p>Pipeline suara paralel memangkas jeda respon ke tingkat percakapan alami dengan dukungan biometrik vokal dan interupsi instan.</p>
                <ul>
                    <li><span>STT Engine</span><strong>Faster-Whisper (base, ID)</strong></li>
                    <li><span>TTS Engine</span><strong>Edge-TTS (id-ID-ArdiNeural)</strong></li>
                    <li><span>Latency (TTFA)</span><strong>&lt; 800 ms (Sentence-Chunked)</strong></li>
                    <li><span>Voice Barge-In</span><strong>Interupsi Bicara Alami</strong></li>
                    <li><span>Biometrik Vokal</span><strong>CAM++ ONNX (192-dim)</strong></li>
                    <li><span>Echo Guard</span><strong>600 ms Hardware AEC Safe</strong></li>
                </ul>
            </div>

            <!-- 3. Android Native Client -->
            <div class="card">
                <div class="card-header">
                    <span class="card-icon">📱</span>
                    <h3>Antarmuka Kios &amp; Mobile (Android)</h3>
                </div>
                <p>Antarmuka native zero-option yang ergonomis satu jempol, memadukan visual maskot 60 FPS dengan audio capture yang tangguh.</p>
                <ul>
                    <li><span>Target Client</span><strong>Android 8.0+ Native Kotlin</strong></li>
                    <li><span>Main Action</span><strong>Floating Circular Pill (56dp)</strong></li>
                    <li><span>Waveform</span><strong>LiveWaveformView (4-Bar Wave)</strong></li>
                    <li><span>Header Status</span><strong>Micro-Capsule (32dp Glass)</strong></li>
                    <li><span>VAD Engine</span><strong>Silero VAD v6.2.3 (On-Device)</strong></li>
                    <li><span>Background Live</span><strong>MediaSession &amp; Dynamic Island</strong></li>
                </ul>
            </div>

            <!-- 4. Smart Studio Lighting (WiZ) -->
            <div class="card">
                <div class="card-header">
                    <span class="card-icon">💡</span>
                    <h3>Smart Lighting (Philips WiZ)</h3>
                </div>
                <p>Pencahayaan studio pintar berkecepatan tinggi melalui protokol UDP lokal tanpa ketergantungan cloud pihak ketiga.</p>
                <ul>
                    <li><span>Protokol Kendali</span><strong>Stateless UDP Port 38899</strong></li>
                    <li><span>Kecepatan Eksekusi</span><strong>&lt; 20 ms (Zero Cloud Delay)</strong></li>
                    <li><span>Palet Warna</span><strong>16+ Warna &amp; 2200K–6500K</strong></li>
                    <li><span>Smart Scenes</span><strong>Mode Fokus &amp; Mode Santai</strong></li>
                    <li><span>Auto-Discovery</span><strong>UDP Broadcast Local Bridge</strong></li>
                </ul>
            </div>

            <!-- 5. Polytron AC & Power Telemetry -->
            <div class="card">
                <div class="card-header">
                    <span class="card-icon">❄️</span>
                    <h3>Smart HVAC &amp; Daya (Polytron AC)</h3>
                </div>
                <p>Integrasi reverse-engineered (REA) ke cloud Fira/Polytron untuk kendali pendingin udara dan pemantauan daya listrik aktif.</p>
                <ul>
                    <li><span>Unit Target</span><strong>Smart Neuva Pro (PAC-05/09VZI)</strong></li>
                    <li><span>Daya Aktif</span><strong>⚡ Live Watt Metering Real-Time</strong></li>
                    <li><span>Sensor Ruangan</span><strong>🌡️ Suhu Ruang &amp; 📈 kWh Harian</strong></li>
                    <li><span>Kontrol HVAC</span><strong>Suhu (16–32°C), Mode, Fan, Swing</strong></li>
                    <li><span>Otentikasi Aman</span><strong>Tantangan OTP WhatsApp Fira</strong></li>
                </ul>
            </div>

            <!-- 6. Network Security & Perimeter -->
            <div class="card">
                <div class="card-header">
                    <span class="card-icon">🛡️</span>
                    <h3>Keamanan &amp; Perimeter (Zero Trust)</h3>
                </div>
                <p>Koneksi end-to-end terenkripsi penuh melalui Cloudflare Zero Trust, isolasi kredensial lokal, dan otentikasi per perangkat.</p>
                <ul>
                    <li><span>Public Tunnel</span><strong>nexo.yhdev.my.id (Zero Trust)</strong></li>
                    <li><span>Origin Port</span><strong>127.0.0.1:8766 (Loopback Only)</strong></li>
                    <li><span>Device Registry</span><strong>HMAC-SHA256 (Mode 0600)</strong></li>
                    <li><span>Endpoint Guard</span><strong>verify_local_or_appliance</strong></li>
                    <li><span>Key Storage</span><strong>Android Keystore (Hardware AES)</strong></li>
                </ul>
            </div>
        </div>

        <!-- Endpoints & Terminal Quickstart -->
        <div class="card" style="margin-bottom: 36px;">
            <div class="card-header">
                <span class="card-icon">⚡</span>
                <h3>Endpoints &amp; Integrasi Sistem</h3>
            </div>
            
            <div class="endpoint-row">
                <div>
                    <span class="badge-method badge-get">GET</span>
                    <code>/health</code>
                </div>
                <span style="color: var(--accent-emerald);">Public &bull; Health Check &amp; Service Metadata</span>
            </div>
            <div class="endpoint-row">
                <div>
                    <span class="badge-method badge-ws">WSS</span>
                    <code>/ws/appliance</code>
                </div>
                <span style="color: var(--accent-cyan);">Protected &bull; Full-Duplex PCM16/MP3 Audio Gateway &amp; Cognitive Stream</span>
            </div>
            <div class="endpoint-row">
                <div>
                    <span class="badge-method badge-get">GET</span>
                    <code>/iot/status</code>
                </div>
                <span style="color: var(--accent-emerald);">Protected &bull; Telemetri Server Linux (CPU, RAM, Suhu) &amp; Status IoT</span>
            </div>
            <div class="endpoint-row">
                <div>
                    <span class="badge-method badge-post">POST</span>
                    <code>/iot/control</code>
                </div>
                <span style="color: var(--accent-amber);">Protected &bull; Kendali WiZ Lighting, Polytron AC, &amp; Smart Scene</span>
            </div>
            <div class="endpoint-row">
                <div>
                    <span class="badge-method badge-get">GET</span>
                    <code>/iot/ac/status</code>
                </div>
                <span style="color: var(--accent-cyan);">Protected &bull; Telemetri Daya Real-Time (⚡ Watt, 🌡️ Suhu, 📈 kWh) AC Polytron</span>
            </div>
            <div class="endpoint-row">
                <div>
                    <span class="badge-method badge-post">POST</span>
                    <code>/iot/ac/auth/request-otp</code>
                </div>
                <span style="color: var(--accent-amber);">Protected &bull; Request OTP WhatsApp Pairing Polytron Fira</span>
            </div>
            <div class="endpoint-row">
                <div>
                    <span class="badge-method badge-post">POST</span>
                    <code>/iot/ac/auth/verify-otp</code>
                </div>
                <span style="color: var(--accent-amber);">Protected &bull; Verifikasi OTP WhatsApp &amp; Registrasi Sesi Token</span>
            </div>
            <div class="endpoint-row">
                <div>
                    <span class="badge-method badge-get">GET</span>
                    <code>/session/*</code>
                </div>
                <span style="color: var(--text-dim);">Loopback Only &bull; Local Operator CLI &amp; Terminal Bridge</span>
            </div>

            <h4 style="font-size: 14px; margin: 24px 0 10px; color: var(--text-main);">Perintah Terminal Lokal (Nexo CLI &amp; IoT Subcommand):</h4>
            <div class="code-snippet"># 1. Monitoring &amp; Sesi Kognitif
nexo status            # Cek status server, bridge, dan appliance aktif
nexo watch             # Stream transkripsi kognitif real-time
nexo bridge            # Konsol interaktif 2-arah (terminal + suara)

# 2. Smart Studio &amp; Lampu Philips WiZ
nexo iot status        # Telemetri server Linux &amp; status perangkat IoT
nexo iot light_on      # Sakelar nyala lampu studio WiZ
nexo iot light_off     # Sakelar padam lampu studio WiZ
nexo iot color -v biru # Ganti warna lampu WiZ (16+ palet warna)
nexo iot dimming -v 80 # Atur kecerahan lampu 80 persen

# 3. AC Polytron Smart Neuva Pro &amp; Telemetri Daya
nexo ac status         # Cek daya listrik (⚡ Watt), suhu ruang, &amp; kWh
nexo ac on / off       # Sakelar daya pendingin ruangan
nexo ac temp -v 22     # Setel suhu target AC 22 derajat Celsius
nexo ac login &lt;no_wa&gt;  # Request pairing OTP WhatsApp Polytron Fira
nexo ac verify-otp &lt;no_wa&gt; &lt;kode&gt; # Verifikasi kode 6-digit OTP</div>
        </div>

        <footer>
            <p>
                <strong>Nexo Dedicated Appliance</strong> &bull; Dibuat oleh <strong>Yanto Hariyono ({{YH Dev}})</strong>
                &bull; <em>"Berkarya dengan hati @ 2026"</em>
            </p>
            <p style="margin-top: 6px;">
                <a href="/health">Health Check API</a> &bull;
                <a href="https://github.com/mrthx89/Nexo" target="_blank" rel="noopener">GitHub Repository</a>
            </p>
        </footer>
    </div>

    <script>
        async function switchMascot(expr) {{
            const container = document.getElementById('mascotContainer');
            const nameEl = document.getElementById('currentExprName');
            try {{
                const res = await fetch('/mascot/' + expr + '.svg');
                if (res.ok) {{
                    const svgText = await res.text();
                    container.innerHTML = svgText;
                    nameEl.textContent = expr.charAt(0).toUpperCase() + expr.slice(1).replace('-', ' ');
                    
                    document.querySelectorAll('.expr-btn').forEach(btn => {{
                        if (btn.textContent.trim().toLowerCase() === expr.replace('-', ' ')) {{
                            btn.classList.add('active');
                        }} else {{
                            btn.classList.remove('active');
                        }}
                    }});
                }}
            }} catch (err) {{
                console.error('Failed to load mascot SVG:', err);
            }}
        }}

        // Verify gateway health dynamically
        window.addEventListener('DOMContentLoaded', async () => {{
            const statusEl = document.getElementById('gatewayStatusText');
            try {{
                const res = await fetch('/health');
                if (res.ok) {{
                    const data = await res.json();
                    if (data.status === 'healthy') {{
                        statusEl.textContent = 'Gateway Online (' + (data.appliance || 'Nexo') + ')';
                    }}
                }}
            }} catch (_) {{
                // Keep default text if offline or unreachable
            }}
        }});
    </script>
</body>
</html>"""
    return html
