const invoke = window.__TAURI__?.core?.invoke;
const listen = window.__TAURI__?.event?.listen;

const EXPRESSIONS = new Set([
  "netral", "senang", "sangat_senang", "tertawa", "terkejut",
  "bingung", "sedih", "menangis", "marah", "takut", "malu",
  "cinta", "mengedip", "mengantuk", "jahil", "keren", "sleeping"
]);

const EXPRESSION_ALIASES = {
  "sangat senang": "sangat_senang",
  "tidur": "sleeping"
};

const els = {
  statusCapsule: document.getElementById("statusCapsule"),
  statusLed: document.getElementById("statusLed"),
  statusLabel: document.getElementById("statusLabel"),
  mascotStage: document.getElementById("mascotStage"),
  mascotAura: document.getElementById("mascotAura"),
  mascotImg: document.getElementById("mascotImg"),
  voiceMood: document.getElementById("voiceMood"),
  replyText: document.getElementById("replyText"),
  replyRole: document.getElementById("replyRole"),
  btnCopyReply: document.getElementById("btnCopyReply"),
  btnToggleVoice: document.getElementById("btnToggleVoice"),
  micIcon: document.getElementById("micIcon"),
  waveCanvas: document.getElementById("waveCanvas"),
  btnModeToggle: document.getElementById("btnModeToggle"),
  btnFinishUtterance: document.getElementById("btnFinishUtterance"),
  btnManualTalk: document.getElementById("btnManualTalk"),
  btnOpenSetup: document.getElementById("btnOpenSetup"),
  btnOpenVoiceprint: document.getElementById("btnOpenVoiceprint"),
  btnOpenIot: document.getElementById("btnOpenIot"),
  modalSetup: document.getElementById("modalSetup"),
  modalVoiceprint: document.getElementById("modalVoiceprint"),
  modalIot: document.getElementById("modalIot"),
  formSetup: document.getElementById("formSetup"),
  inputDeviceId: document.getElementById("inputDeviceId"),
  btnCopyDeviceId: document.getElementById("btnCopyDeviceId"),
  inputServerUrl: document.getElementById("inputServerUrl"),
  inputToken: document.getElementById("inputToken"),
  tokenStatusHint: document.getElementById("tokenStatusHint"),
  inputVadModel: document.getElementById("inputVadModel"),
  setupFeedback: document.getElementById("setupFeedback"),
  btnClearToken: document.getElementById("btnClearToken"),
  tabEnroll: document.getElementById("tabEnroll"),
  tabDiagnostic: document.getElementById("tabDiagnostic"),
  vpSessionState: document.getElementById("vpSessionState"),
  vpSessionDetail: document.getElementById("vpSessionDetail"),
  vpProfileMeta: document.getElementById("vpProfileMeta"),
  vpEnrolledLabel: document.getElementById("vpEnrolledLabel"),
  vpSamplesBar: document.getElementById("vpSamplesBar"),
  btnVpRecord: document.getElementById("btnVpRecord"),
  btnVpFinish: document.getElementById("btnVpFinish"),
  btnVpSave: document.getElementById("btnVpSave"),
  btnVpPlayDiag: document.getElementById("btnVpPlayDiag"),
  btnVpDeleteDiag: document.getElementById("btnVpDeleteDiag"),
  btnVpUnenroll: document.getElementById("btnVpUnenroll"),
  btnVpResume: document.getElementById("btnVpResume"),
  tabWiz: document.getElementById("tabWiz"),
  tabAc: document.getElementById("tabAc"),
  paneWiz: document.getElementById("paneWiz"),
  paneAc: document.getElementById("paneAc"),
  btnScanWiz: document.getElementById("btnScanWiz"),
  wizListContainer: document.getElementById("wizListContainer"),
  acContent: document.getElementById("acContent")
};

let snapshot = null;
let lastFinalReply = "";
let lastFocusedTrigger = null;
let voiceMoodTimer = null;
let wavePhase = 0;

function mapExpression(expr) {
  if (!expr) return "netral";
  const key = expr.toLowerCase().trim();
  const mapped = EXPRESSION_ALIASES[key] || key;
  return EXPRESSIONS.has(mapped) ? mapped : "netral";
}

function stateLedColor(state, voiceEnabled) {
  if (state === "ERROR") return "#FF5252";
  if (!voiceEnabled) return "#8892B0";
  switch (state) {
    case "LISTENING":
      return "#00E676";
    case "THINKING":
    case "DISCONNECTED":
      return "#FFB300";
    case "SPEAKING":
      return "#2979FF";
    default:
      return "#00E5FF";
  }
}

function stateShortLabel(state, voiceEnabled) {
  if (state === "ERROR") return "Gangguan";
  if (!voiceEnabled) return "Dijeda";
  switch (state) {
    case "LISTENING":
      return "Mendengarkan";
    case "THINKING":
      return "Memahami";
    case "SPEAKING":
      return "Menjawab";
    case "DISCONNECTED":
      return "Menghubungkan";
    default:
      return "Siaga";
  }
}

function drawWaveform(amplitude) {
  const ctx = els.waveCanvas.getContext("2d");
  if (!ctx) return;
  const w = els.waveCanvas.width;
  const h = els.waveCanvas.height;
  ctx.clearRect(0, 0, w, h);
  wavePhase += 0.28;
  const bars = 4;
  const barW = 5;
  const gap = 5;
  const startX = (w - (bars * barW + (bars - 1) * gap)) / 2;
  const level = Math.max(0.18, Math.min(1, amplitude * 2.2));
  ctx.fillStyle = "#000000";
  for (let i = 0; i < bars; i++) {
    const wave = 0.55 + 0.45 * Math.sin(wavePhase + i * 1.15);
    const barH = Math.max(6, Math.round(h * 0.72 * level * wave));
    const x = startX + i * (barW + gap);
    const y = Math.round((h - barH) / 2);
    ctx.beginPath();
    ctx.roundRect(x, y, barW, barH, 3);
    ctx.fill();
  }
}

function renderSnapshot(s) {
  if (!s) return;
  const prevEnabled = snapshot?.voice_enabled;
  snapshot = s;

  const expr = s.voice_enabled
    ? mapExpression(s.expression)
    : (s.expression === "sleeping" || s.expression === "tidur" ? "sleeping" : "menangis");
  const svgFile = expr === "sangat_senang" ? "sangat-senang" : expr;
  const nextSrc = `assets/svg/${svgFile}.svg`;
  if (!els.mascotImg.src.endsWith(nextSrc)) {
    els.mascotImg.src = nextSrc;
    els.mascotImg.alt = `Maskot NEXO Silver ekspresi ${expr}`;
    els.mascotStage.setAttribute("aria-label", `Maskot Nexo ekspresi ${expr}`);
  }
  els.mascotStage.classList.toggle("is-sleeping", expr === "sleeping");

  els.statusLed.style.background = stateLedColor(s.state, s.voice_enabled);
  els.statusLabel.textContent = stateShortLabel(s.state, s.voice_enabled);

  if (prevEnabled !== undefined && prevEnabled !== s.voice_enabled) {
    els.voiceMood.textContent = s.voice_enabled
      ? "Mikrofon selalu aktif hingga Anda menekan Berhenti."
      : "Mikrofon dijeda.";
    els.voiceMood.hidden = false;
    clearTimeout(voiceMoodTimer);
    voiceMoodTimer = setTimeout(() => {
      els.voiceMood.hidden = true;
    }, 6000);
  }

  els.btnToggleVoice.classList.toggle("is-active", s.voice_enabled);
  els.btnToggleVoice.setAttribute("aria-pressed", String(s.voice_enabled));
  els.btnToggleVoice.setAttribute(
    "aria-label",
    s.voice_enabled ? "Berhenti mikrofon" : "Mulai mikrofon"
  );
  els.micIcon.hidden = s.voice_enabled;
  els.waveCanvas.hidden = !s.voice_enabled;
  if (s.voice_enabled) {
    drawWaveform(s.amplitude || 0);
  }

  const auto = Boolean(s.automatic_mode);
  els.btnModeToggle.textContent = s.vad_available
    ? (auto ? "Mode: Auto" : "Mode: Manual")
    : "Mode: Manual (VAD off)";
  els.btnModeToggle.setAttribute("aria-pressed", String(auto));
  els.btnModeToggle.disabled = !s.vad_available;

  els.btnFinishUtterance.hidden = !(s.voice_enabled && s.can_finish);
  els.btnManualTalk.hidden = !(s.voice_enabled && !auto && !s.can_finish);

  if (s.last_reply && s.last_reply !== lastFinalReply) {
    lastFinalReply = s.last_reply;
  }
  if (lastFinalReply && s.state !== "ERROR" && s.state !== "DISCONNECTED") {
    els.replyText.textContent = lastFinalReply;
    els.replyRole.textContent = "Jawaban Nexo";
  } else {
    els.replyText.textContent = s.message || "Dijeda";
    els.replyRole.textContent = stateShortLabel(s.state, s.voice_enabled);
  }
  els.btnCopyReply.disabled = !lastFinalReply;

  const scale = 1 + Math.min(0.28, (s.amplitude || 0) * 0.9);
  els.mascotAura.style.transform = `scale(${scale.toFixed(3)})`;
  els.mascotAura.style.opacity = s.voice_enabled ? "0.75" : "0.25";

  if (!els.modalVoiceprint.hidden) {
    renderVoiceprintSession(s.recording);
  }
}

function renderVoiceprintSession(rec) {
  if (!rec) return;
  els.vpSessionState.textContent = rec.state || "CLOSED";
  els.vpSessionDetail.textContent = rec.message || "";
  const isDiag = Boolean(rec.diagnostic);
  els.tabEnroll.classList.toggle("active", !isDiag);
  els.tabDiagnostic.classList.toggle("active", isDiag);
  els.tabEnroll.setAttribute("aria-selected", String(!isDiag));
  els.tabDiagnostic.setAttribute("aria-selected", String(isDiag));

  if (!isDiag && rec.voiceprint_available !== null) {
    els.vpProfileMeta.hidden = false;
    els.vpEnrolledLabel.textContent = rec.is_enrolled ? "Terdaftar" : "Belum terdaftar";
  } else {
    els.vpProfileMeta.hidden = true;
  }

  if (!isDiag && rec.required_samples > 0) {
    els.vpSamplesBar.hidden = false;
    els.vpSamplesBar.textContent = `Kemajuan rekaman: ${rec.sample_count} / ${rec.required_samples + 1} (termasuk 1 holdout)`;
  } else {
    els.vpSamplesBar.hidden = true;
  }

  const recording = rec.state === "RECORDING";
  els.btnVpRecord.hidden = recording;
  els.btnVpRecord.disabled = !rec.ready || rec.state === "UPLOADING" || rec.state === "SAVING";
  els.btnVpRecord.textContent = isDiag ? "Rekam Klip Uji" : "Rekam Sampel";
  els.btnVpFinish.hidden = !recording;
  els.btnVpSave.hidden = !(
    !isDiag &&
    rec.ready &&
    rec.required_samples >= 2 &&
    rec.sample_count > rec.required_samples &&
    rec.state !== "SAVED"
  );
  els.btnVpPlayDiag.hidden = !(isDiag && rec.diagnostic_ready);
  els.btnVpDeleteDiag.hidden = !(isDiag && rec.diagnostic_ready);
  els.btnVpUnenroll.hidden = !(!isDiag && rec.ready && rec.is_enrolled);
}

function openModal(modal, triggerEl) {
  lastFocusedTrigger = triggerEl || document.activeElement;
  modal.hidden = false;
  const focusable = modal.querySelector("button, input, select");
  if (focusable) focusable.focus();
}

function closeModal(modal) {
  if (modal.hidden) return;
  modal.hidden = true;
  if (modal === els.modalVoiceprint && invoke) {
    invoke("close_recording_sheet", { resume: false }).catch(() => {});
  }
  if (lastFocusedTrigger && typeof lastFocusedTrigger.focus === "function") {
    lastFocusedTrigger.focus();
  }
}

document.querySelectorAll("[data-close-modal]").forEach((btn) => {
  btn.addEventListener("click", () => {
    const id = btn.getAttribute("data-close-modal");
    closeModal(document.getElementById(id));
  });
});

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    [els.modalSetup, els.modalVoiceprint, els.modalIot].forEach((m) => closeModal(m));
  }
});

// Mascot gaze + wake interaction
function activateMascotStage() {
  if (els.mascotStage.classList.contains("is-sleeping")) {
    els.mascotStage.classList.remove("is-sleeping");
    if (snapshot) {
      renderSnapshot({ ...snapshot, expression: "senang" });
    }
    return;
  }
  els.btnToggleVoice.click();
}

els.mascotStage.addEventListener("click", activateMascotStage);
els.mascotStage.addEventListener("keydown", (e) => {
  if (e.key === "Enter" || e.key === " ") {
    e.preventDefault();
    activateMascotStage();
  }
});

document.addEventListener("mousemove", (e) => {
  if (!snapshot || snapshot.expression === "sleeping" || window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    return;
  }
  const rect = els.mascotStage.getBoundingClientRect();
  const dx = ((e.clientX - (rect.left + rect.width / 2)) / Math.max(window.innerWidth, 1)) * 10;
  const dy = ((e.clientY - (rect.top + rect.height / 2)) / Math.max(window.innerHeight, 1)) * 8;
  els.mascotImg.style.transform = `translate(${dx.toFixed(1)}px, ${dy.toFixed(1)}px)`;
});

// Core Voice Triggers
els.btnToggleVoice.addEventListener("click", async () => {
  if (!invoke) return;
  const enabled = Boolean(snapshot?.voice_enabled);
  await invoke(enabled ? "stop_voice" : "start_voice").catch((err) => {
    els.replyText.textContent = String(err);
  });
});

els.btnModeToggle.addEventListener("click", async () => {
  if (!invoke || !snapshot) return;
  await invoke("set_automatic_mode", { enabled: !snapshot.automatic_mode });
});

els.btnFinishUtterance.addEventListener("click", () => {
  if (invoke) invoke("finish_utterance").catch(() => {});
});

els.btnManualTalk.addEventListener("click", () => {
  if (invoke) invoke("start_manual_utterance").catch(() => {});
});

els.btnCopyReply.addEventListener("click", async () => {
  if (!lastFinalReply) return;
  try {
    await navigator.clipboard.writeText(lastFinalReply);
    const old = els.btnCopyReply.textContent;
    els.btnCopyReply.textContent = "Tersalin";
    setTimeout(() => {
      els.btnCopyReply.textContent = old;
    }, 1500);
  } catch (_) {}
});

// Setup Credentials
function showSetupModal(trigger) {
  els.setupFeedback.hidden = true;
  if (snapshot) {
    els.inputDeviceId.value = snapshot.device_id || "";
    els.inputServerUrl.value = snapshot.server_url || "";
    els.tokenStatusHint.textContent = snapshot.has_token
      ? "Status token: tersimpan aman di DPAPI (kosongkan jika tidak mengubah)."
      : "Status token: belum terpasang. Masukkan bearer token untuk menghubungkan.";
  }
  openModal(els.modalSetup, trigger);
}

els.statusCapsule.addEventListener("click", () => showSetupModal(els.statusCapsule));
els.btnOpenSetup.addEventListener("click", () => showSetupModal(els.btnOpenSetup));

els.btnCopyDeviceId.addEventListener("click", async () => {
  if (!els.inputDeviceId.value) return;
  await navigator.clipboard.writeText(els.inputDeviceId.value).catch(() => {});
  els.btnCopyDeviceId.textContent = "Disalin";
  setTimeout(() => {
    els.btnCopyDeviceId.textContent = "Salin ID";
  }, 1400);
});

els.formSetup.addEventListener("submit", async (e) => {
  e.preventDefault();
  if (!invoke) return;
  const serverUrl = els.inputServerUrl.value.trim();
  const token = els.inputToken.value.trim() || null;
  const vadModel = els.inputVadModel.value.trim() || null;
  if (!serverUrl.startsWith("wss://")) {
    els.setupFeedback.textContent = "Endpoint wajib memakai protokol wss://.";
    els.setupFeedback.className = "state-banner is-error";
    els.setupFeedback.hidden = false;
    return;
  }
  try {
    await invoke("save_credentials", { serverUrl, token, vadModel });
    els.inputToken.value = "";
    closeModal(els.modalSetup);
  } catch (err) {
    els.setupFeedback.textContent = String(err);
    els.setupFeedback.className = "state-banner is-error";
    els.setupFeedback.hidden = false;
  }
});

els.btnClearToken.addEventListener("click", async () => {
  if (!invoke) return;
  await invoke("clear_credentials").catch(() => {});
  els.setupFeedback.textContent = "Token perangkat berhasil dihapus.";
  els.setupFeedback.className = "state-banner";
  els.setupFeedback.hidden = false;
});

// Voiceprint & Diagnostic Modal
els.btnOpenVoiceprint.addEventListener("click", async () => {
  openModal(els.modalVoiceprint, els.btnOpenVoiceprint);
  if (invoke) {
    await invoke("open_recording_sheet", { diagnosticOnly: false }).catch(() => {});
  }
});

els.tabEnroll.addEventListener("click", () => {
  if (invoke) invoke("open_recording_sheet", { diagnosticOnly: false }).catch(() => {});
});
els.tabDiagnostic.addEventListener("click", () => {
  if (invoke) invoke("open_recording_sheet", { diagnosticOnly: true }).catch(() => {});
});

els.btnVpRecord.addEventListener("click", () => {
  if (invoke) invoke("start_enrollment_or_diagnostic").catch(() => {});
});
els.btnVpFinish.addEventListener("click", () => {
  if (invoke) invoke("finish_utterance").catch(() => {});
});
els.btnVpSave.addEventListener("click", () => {
  if (invoke) invoke("save_enrollment").catch(() => {});
});
els.btnVpUnenroll.addEventListener("click", () => {
  if (invoke) invoke("unenroll_voiceprint").catch(() => {});
});
els.btnVpPlayDiag.addEventListener("click", () => {
  if (invoke) invoke("play_diagnostic").catch(() => {});
});
els.btnVpDeleteDiag.addEventListener("click", () => {
  if (invoke) invoke("delete_diagnostic").catch(() => {});
});
els.btnVpResume.addEventListener("click", async () => {
  if (invoke) await invoke("close_recording_sheet", { resume: true }).catch(() => {});
  els.modalVoiceprint.hidden = true;
});

// Smart Home IoT Modal
function renderWizBulbs(bulbs) {
  els.wizListContainer.replaceChildren();
  if (!bulbs || bulbs.length === 0) {
    const empty = document.createElement("div");
    empty.className = "state-banner";
    empty.textContent = "Belum ada lampu WiZ ditemukan. Pastikan PC berada di jaringan WiFi/LAN yang sama dengan lampu, lalu tekan Pindai Jaringan Lokal.";
    els.wizListContainer.appendChild(empty);
    return;
  }
  bulbs.forEach((bulb) => {
    const card = document.createElement("div");
    card.className = "device-card";

    const head = document.createElement("div");
    head.className = "device-head";
    const info = document.createElement("div");
    const title = document.createElement("div");
    title.className = "device-title";
    title.textContent = bulb.name;
    const sub = document.createElement("div");
    sub.className = "device-sub";
    sub.textContent = `${bulb.ip} · MAC ${bulb.mac || "-"}`;
    info.append(title, sub);

    const powerBtn = document.createElement("button");
    powerBtn.type = "button";
    powerBtn.className = bulb.state ? "primary-btn" : "secondary-btn";
    powerBtn.textContent = bulb.state ? "Nyala" : "Mati";
    powerBtn.addEventListener("click", async () => {
      if (!invoke) return;
      const nextState = !bulb.state;
      await invoke("control_wiz_bulb", {
        params: { ip: bulb.ip, state: nextState, dimming: null, temp: null, r: null, g: null, b: null, scene_id: null }
      });
      bulb.state = nextState;
      powerBtn.className = nextState ? "primary-btn" : "secondary-btn";
      powerBtn.textContent = nextState ? "Nyala" : "Mati";
    });
    head.append(info, powerBtn);

    const sliderRow = document.createElement("div");
    sliderRow.className = "slider-row";
    const dimLabel = document.createElement("span");
    dimLabel.textContent = `Kecerahan ${bulb.dimming}%`;
    const slider = document.createElement("input");
    slider.type = "range";
    slider.min = "10";
    slider.max = "100";
    slider.value = String(bulb.dimming || 100);
    slider.setAttribute("aria-label", `Kecerahan ${bulb.name}`);
    slider.addEventListener("change", async () => {
      if (!invoke) return;
      const val = Number(slider.value);
      dimLabel.textContent = `Kecerahan ${val}%`;
      await invoke("control_wiz_bulb", {
        params: { ip: bulb.ip, state: true, dimming: val, temp: null, r: null, g: null, b: null, scene_id: null }
      });
    });
    sliderRow.append(dimLabel, slider);
    card.append(head, sliderRow);
    els.wizListContainer.appendChild(card);
  });
}

async function loadAcTab() {
  els.acContent.replaceChildren();
  const loading = document.createElement("div");
  loading.className = "state-banner";
  loading.textContent = "Memuat status Polytron AC dari server...";
  els.acContent.appendChild(loading);
  if (!invoke) return;

  try {
    const res = await invoke("ac_status", { serial: null });
    els.acContent.replaceChildren();
    if (!res.authenticated) {
      const box = document.createElement("div");
      box.className = "device-card";
      const info = document.createElement("p");
      info.className = "status-box-desc";
      info.textContent = "Akun Polytron Smart Home belum tertaut. Masukkan nomor WhatsApp terdaftar untuk meminta OTP.";
      const phoneInput = document.createElement("input");
      phoneInput.type = "tel";
      phoneInput.placeholder = "0812xxxxxxxx";
      const reqBtn = document.createElement("button");
      reqBtn.type = "button";
      reqBtn.className = "primary-btn";
      reqBtn.textContent = "Minta OTP WhatsApp";
      reqBtn.addEventListener("click", async () => {
        if (!phoneInput.value.trim()) return;
        await invoke("ac_request_otp", { phone: phoneInput.value.trim() });
        info.textContent = "OTP terkirim ke WhatsApp. Masukkan kode verifikasi lalu tekan Verifikasi.";
        const otpInput = document.createElement("input");
        otpInput.type = "text";
        otpInput.placeholder = "Kode OTP";
        const verBtn = document.createElement("button");
        verBtn.type = "button";
        verBtn.className = "primary-btn";
        verBtn.textContent = "Verifikasi OTP";
        verBtn.addEventListener("click", async () => {
          await invoke("ac_verify_otp", { phone: phoneInput.value.trim(), otpCode: otpInput.value.trim() });
          loadAcTab();
        });
        box.append(otpInput, verBtn);
      });
      box.append(info, phoneInput, reqBtn);
      els.acContent.appendChild(box);
      return;
    }

    const st = res.state || {};
    const card = document.createElement("div");
    card.className = "device-card";
    const summary = document.createElement("div");
    summary.className = "status-box-title";
    summary.textContent = `Polytron AC (${res.default_serial || "terhubung"}) · Suhu target ${st.temp ?? 24} C`;
    const btnRow = document.createElement("div");
    btnRow.className = "modal-actions wrap-actions";
    const controls = [
      { label: "Nyalakan AC", action: "ac_on" },
      { label: "Matikan AC", action: "ac_off" },
      { label: "22 C", action: "ac_temp", extra: { temp: 22 } },
      { label: "24 C", action: "ac_temp", extra: { temp: 24 } },
      { label: "26 C", action: "ac_temp", extra: { temp: 26 } }
    ];
    controls.forEach((c) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "secondary-btn";
      b.textContent = c.label;
      b.addEventListener("click", async () => {
        await invoke("iot_control", { payload: { action: c.action, ...(c.extra || {}) } });
        loadAcTab();
      });
      btnRow.appendChild(b);
    });
    card.append(summary, btnRow);
    els.acContent.appendChild(card);
  } catch (err) {
    els.acContent.replaceChildren();
    const errBox = document.createElement("div");
    errBox.className = "state-banner is-error";
    errBox.textContent = `Gagal memuat AC dari server: ${err}. Pastikan token perangkat sudah tersimpan.`;
    els.acContent.appendChild(errBox);
  }
}

els.btnOpenIot.addEventListener("click", async () => {
  openModal(els.modalIot, els.btnOpenIot);
  if (invoke) {
    const cached = await invoke("load_cached_bulbs").catch(() => []);
    renderWizBulbs(cached);
  }
});

els.tabWiz.addEventListener("click", () => {
  els.tabWiz.classList.add("active");
  els.tabAc.classList.remove("active");
  els.tabWiz.setAttribute("aria-selected", "true");
  els.tabAc.setAttribute("aria-selected", "false");
  els.paneWiz.hidden = false;
  els.paneAc.hidden = true;
});

els.tabAc.addEventListener("click", () => {
  els.tabAc.classList.add("active");
  els.tabWiz.classList.remove("active");
  els.tabAc.setAttribute("aria-selected", "true");
  els.tabWiz.setAttribute("aria-selected", "false");
  els.paneAc.hidden = false;
  els.paneWiz.hidden = true;
  loadAcTab();
});

els.btnScanWiz.addEventListener("click", async () => {
  els.wizListContainer.innerHTML = `<div class="state-banner">Memindai lampu WiZ melalui UDP port 38899...</div>`;
  if (!invoke) return;
  try {
    const bulbs = await invoke("scan_wiz_bulbs");
    renderWizBulbs(bulbs);
  } catch (err) {
    els.wizListContainer.innerHTML = `<div class="state-banner is-error">Pemindaian gagal: ${err}. Coba pindai ulang.</div>`;
  }
});

document.querySelectorAll(".scene-trigger").forEach((btn) => {
  btn.addEventListener("click", () => {
    const action = btn.getAttribute("data-scene");
    if (invoke && action) {
      invoke("iot_control", { payload: { action } }).catch(() => {});
    }
  });
});

// Bootstrap
async function init() {
  if (!invoke) {
    els.replyText.textContent = "Preview UI lokal (Tauri runtime tidak aktif). Jalankan melalui aplikasi Nexo Desktop.";
    return;
  }
  const initial = await invoke("get_snapshot").catch(() => null);
  if (initial) renderSnapshot(initial);
  if (listen) {
    await listen("nexo-event", (event) => {
      const ev = event.payload;
      const data = ev?.payload;
      if (!ev || !data) return;
      if (ev.type === "snapshot") {
        renderSnapshot(data);
      } else if (ev.type === "telemetry") {
        if (snapshot) {
          snapshot.amplitude = data.amplitude;
          const scale = 1 + Math.min(0.28, data.amplitude * 0.9);
          els.mascotAura.style.transform = `scale(${scale.toFixed(3)})`;
          if (snapshot.voice_enabled) drawWaveform(data.amplitude);
        }
      } else if (ev.type === "transcription") {
        if (data.role === "assistant" && data.is_final && data.text) {
          lastFinalReply = data.text;
          els.replyText.textContent = data.text;
          els.replyRole.textContent = "Jawaban Nexo";
          els.btnCopyReply.disabled = false;
        } else if (data.role === "user" && data.text) {
          els.replyText.textContent = data.text;
          els.replyRole.textContent = "Anda";
        } else if (data.role === "system" && data.text) {
          els.replyText.textContent = data.text;
          els.replyRole.textContent = "Info sistem";
        }
      }
    });
  }
}

init();
