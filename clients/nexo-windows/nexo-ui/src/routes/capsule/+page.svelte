<script lang="ts">
  import { onMount } from 'svelte';
  import { nexo } from '$lib/stores/nexo';
  import { STATE_LABELS } from '$lib/backend/types';

  onMount(() => {
    nexo.init();
  });

  $: bars = ($nexo.telemetry.frequencyBands?.length ? $nexo.telemetry.frequencyBands : [0, 0, 0, 0]).slice(0, 4);
  $: color =
    $nexo.state === 'LISTENING'
      ? '#00e676'
      : $nexo.state === 'ANSWERING'
        ? '#00e5ff'
        : $nexo.state === 'PROCESSING'
          ? '#ffb300'
          : $nexo.state === 'PAUSED'
            ? '#7a8aa8'
            : $nexo.state === 'CONNECTING'
              ? '#ffb300'
              : '#00e5ff';
  $: micLabel = $nexo.micEnabled && $nexo.state !== 'PAUSED' ? 'Mic aktif' : 'Mic nonaktif';

  async function toggleMain() {
    try {
      const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
      const main = await WebviewWindow.getByLabel('main');
      if (main) {
        const visible = await main.isVisible();
        if (visible) {
          await main.hide();
        } else {
          await main.show();
          await main.unminimize();
          await main.setFocus();
        }
      }
    } catch {
      // browser/dev fallback
      window.open('/', '_blank');
    }
  }

  async function hideCapsule() {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().hide();
    } catch {
      window.close();
    }
  }
</script>

<div class="wrap">
  <button
    class="island"
    style="--accent:{color}"
    on:click={toggleMain}
    title="Klik untuk tampil/sembunyikan jendela utama Nexo"
  >
    <div class="dot" class:pulse={$nexo.state !== 'IDLE' && $nexo.state !== 'PAUSED'}></div>
    <img class="mini" src={`/mascots/mascot_${$nexo.expression || 'netral'}.png`} alt="nexo" />
    <div class="text">
      <div class="label">{STATE_LABELS[$nexo.state]}</div>
      <div class="sub">{$nexo.connected ? micLabel : 'Menghubungkan…'}</div>
    </div>
    <div class="eq" aria-hidden="true">
      {#each bars as b, i}
        <span
          class="bar"
          style="height:{Math.max(12, Math.round(b * 100))}%; animation-delay:{i * 0.08}s"
        ></span>
      {/each}
    </div>
  </button>
  <button class="hide" on:click={hideCapsule} title="Sembunyikan capsule (tetap di tray)">✕</button>
</div>

<style>
  :global(html), :global(body) { background: transparent !important; margin: 0; overflow: hidden; }
  .wrap { height: 100vh; display: flex; align-items: center; justify-content: center; gap: 8px; background: transparent; padding: 8px; }
  .island {
    display: inline-flex; align-items: center; gap: 10px;
    padding: 8px 14px 8px 10px; border-radius: 999px;
    background: rgba(12, 18, 34, 0.92);
    border: 1px solid rgba(0,229,255,0.22);
    box-shadow: 0 10px 40px rgba(0,0,0,0.45), 0 0 0 1px rgba(255,255,255,0.03) inset;
    color: #e6eefc; cursor: pointer;
    backdrop-filter: blur(14px);
  }
  .dot { width: 10px; height: 10px; border-radius: 50%; background: var(--accent); box-shadow: 0 0 10px var(--accent); }
  .dot.pulse { animation: pulse 1.4s ease-in-out infinite; }
  @keyframes pulse { 0%,100% { transform: scale(1); opacity: 1; } 50% { transform: scale(1.4); opacity: 0.7; } }
  .mini { width: 28px; height: 28px; object-fit: contain; }
  .text { display: flex; flex-direction: column; line-height: 1.1; text-align: left; }
  .label { font-size: 13px; font-weight: 700; white-space: nowrap; }
  .sub { font-size: 10px; color: #7a8aa8; }
  .eq { display: inline-flex; align-items: flex-end; gap: 3px; height: 18px; margin-left: 4px; }
  .bar { width: 3px; border-radius: 3px; background: var(--accent); animation: bounce 0.9s ease-in-out infinite; min-height: 3px; }
  @keyframes bounce { 0%,100% { transform: scaleY(0.5); } 50% { transform: scaleY(1.2); } }
  .hide {
    width: 28px; height: 28px; border-radius: 50%; border: 1px solid rgba(255,255,255,0.12);
    background: rgba(18,26,46,0.85); color: #7a8aa8; cursor: pointer;
  }
  .hide:hover { color: #fff; }
</style>
