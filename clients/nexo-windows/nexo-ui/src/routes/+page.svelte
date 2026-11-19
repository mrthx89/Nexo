<script lang="ts">
  import { onMount } from 'svelte';
  import { nexo } from '$lib/stores/nexo';
  import { settingsOpen, capsuleVisible } from '$lib/stores/ui';
  import { STATE_LABELS } from '$lib/backend/types';
  import StatusCapsule from '$lib/components/StatusCapsule.svelte';
  import MascotStage from '$lib/components/MascotStage.svelte';
  import ConversationPanel from '$lib/components/ConversationPanel.svelte';
  import MicButton from '$lib/components/MicButton.svelte';
  import SettingsSheet from '$lib/components/SettingsSheet.svelte';

  onMount(() => { nexo.init(); });

  async function toggleMic() {
    const paused = $nexo.state === 'PAUSED' || !$nexo.micEnabled;
    await nexo.setMicEnabled(paused ? true : false);
  }

  async function openCapsuleWindow() {
    try {
      const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
      const existing = await WebviewWindow.getByLabel('capsule');
      if (existing) { await existing.show(); await existing.setFocus(); }
    } catch {
      // browser/dev fallback: open route
      window.open('/capsule', '_blank', 'width=420,height=120');
    }
    capsuleVisible.set(true);
  }
</script>

<div class="kiosk">
  <div class="topbar">
    <div class="left">
      <div class="brand">Nexo <span class="sub">Windows Kiosk</span></div>
    </div>
    <div class="center">
      {#if $capsuleVisible}
        <StatusCapsule state={$nexo.state} telemetry={$nexo.telemetry} onToggle={() => capsuleVisible.set(false)} />
      {:else}
        <button class="restore" on:click={() => capsuleVisible.set(true)}>Tampilkan capsule</button>
      {/if}
    </div>
    <div class="right">
      <button class="pill" on:click={openCapsuleWindow} title="Buka jendela capsule/notch always-on-top">Capsule window</button>
      <button class="pill" on:click={() => settingsOpen.set(true)}>Settings</button>
      <div class="conn" class:ok={$nexo.connected} class:bad={!$nexo.connected}>{$nexo.connected ? 'Terhubung' : 'Simulasi/Offline'}</div>
    </div>
  </div>

  <div class="main">
    <div class="stage-pane">
      <MascotStage expression={$nexo.expression} state={$nexo.state} telemetry={$nexo.telemetry} />
      <div class="state-big">
        <div class="state-label">{STATE_LABELS[$nexo.state]}</div>
        <div class="state-sub">{$nexo.statusText}</div>
      </div>
    </div>
    <div class="side-pane">
      <div class="controls">
        <MicButton active={$nexo.micEnabled && $nexo.state !== 'PAUSED'} enabled={$nexo.micEnabled} telemetry={$nexo.telemetry} onClick={toggleMic} />
        <div class="hints">
          <div class="hint"><b>State machine:</b> {$nexo.state}</div>
          <div class="hint"><b>Ekspresi:</b> {$nexo.expression}</div>
          <div class="hint"><b>Mode:</b> UI simulasi via interface backend (mock) — siap ditempel ke nexo-core/Tauri command.</div>
        </div>
      </div>
      <div class="conversation">
        <ConversationPanel transcript={$nexo.transcript} />
      </div>
    </div>
  </div>

  <SettingsSheet
    open={$settingsOpen}
    serverUrl={$nexo.serverUrl}
    deviceId={$nexo.deviceId}
    hasToken={$nexo.hasToken}
    on:close={() => settingsOpen.set(false)}
  />
</div>

<style>
  .kiosk { height: 100vh; display: flex; flex-direction: column; background: radial-gradient(1200px 600px at 20% -10%, rgba(0,229,255,0.10), transparent 60%), radial-gradient(900px 500px at 100% 0%, rgba(0,230,118,0.08), transparent 55%), #0b1020; }
  .topbar {
    display: grid; grid-template-columns: 1fr auto 1fr; align-items: center;
    gap: 12px; padding: 14px 18px 8px;
  }
  .left { justify-self: start; }
  .center { justify-self: center; min-height: 40px; display:flex; align-items:center; }
  .right { justify-self: end; display:flex; align-items:center; gap: 8px; }
  .brand { font-weight: 800; font-size: 18px; letter-spacing: .3px; }
  .brand .sub { font-weight: 600; font-size: 12px; color: #7a8aa8; margin-left: 6px; }
  .pill, .restore {
    background: rgba(18,26,46,0.8); color: #cfe3ff; border: 1px solid rgba(255,255,255,0.1);
    border-radius: 999px; padding: 7px 12px; font-size: 12px; cursor: pointer;
  }
  .pill:hover, .restore:hover { border-color: rgba(0,229,255,0.35); color: #fff; }
  .conn { font-size: 11px; padding: 6px 10px; border-radius: 999px; border: 1px solid rgba(255,255,255,0.1); }
  .conn.ok { color: #00e676; border-color: rgba(0,230,118,0.3); background: rgba(0,230,118,0.06); }
  .conn.bad { color: #ffb300; border-color: rgba(255,179,0,0.3); background: rgba(255,179,0,0.06); }
  .main { flex: 1; display: grid; grid-template-columns: 1.2fr 1fr; gap: 16px; padding: 8px 18px 18px; min-height: 0; }
  .stage-pane {
    position: relative; display:flex; flex-direction:column; min-height: 0;
    background: rgba(18,26,46,0.35); border: 1px solid rgba(255,255,255,0.05);
    border-radius: 22px; overflow: hidden;
  }
  .state-big { position: absolute; left: 18px; bottom: 16px; }
  .state-label { font-size: 28px; font-weight: 800; letter-spacing: .3px; }
  .state-sub { font-size: 13px; color: #7a8aa8; margin-top: 2px; max-width: 46ch; }
  .side-pane { display:flex; flex-direction:column; gap: 14px; min-height: 0; }
  .controls {
    display:flex; gap: 14px; align-items: center;
    background: rgba(18,26,46,0.55); border: 1px solid rgba(255,255,255,0.06);
    border-radius: 18px; padding: 14px;
  }
  .hints { display:flex; flex-direction:column; gap: 6px; font-size: 12px; color: #cfe3ff; }
  .hint b { color: #fff; }
  .conversation { flex: 1; min-height: 0; }
  @media (max-width: 980px) {
    .main { grid-template-columns: 1fr; }
    .topbar { grid-template-columns: 1fr; justify-items: start; }
    .center, .right { justify-self: start; }
  }
</style>
