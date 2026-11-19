<script lang="ts">
  import type { VisualTelemetry } from '../backend/types';

  export let active = false;
  export let enabled = true;
  export let telemetry: VisualTelemetry = { amplitude: 0, frequencyBands: [0, 0, 0, 0] };
  export let onClick: () => void = () => {};

  $: bars = (telemetry.frequencyBands?.length ? telemetry.frequencyBands : [0.2, 0.4, 0.3, 0.2]).slice(0, 4);
  $: color = active && enabled ? '#00e676' : '#7a8aa8';
  $: glow = active && enabled;
</script>

<button class="mic" class:active={active && enabled} class:glow on:click={onClick} aria-label="Mic toggle" title={active ? 'Mic aktif (klik untuk pause)' : 'Mic nonaktif (klik untuk aktifkan)'}>
  <div class="wave" aria-hidden="true">
    {#each bars as b, i}
      <span class="bar" style="height:{enabled ? Math.max(18, Math.round(b * 100)) : 18}%; animation-delay:{i * 0.07}s; background:{color}"></span>
    {/each}
  </div>
  <div class="icon">
    {#if active && enabled}
      <svg viewBox="0 0 24 24" width="26" height="26" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12 14a3 3 0 0 0 3-3V5a3 3 0 1 0-6 0v6a3 3 0 0 0 3 3z"/>
        <path d="M19 11a7 7 0 0 1-14 0"/>
        <path d="M12 18v3"/>
      </svg>
    {:else}
      <svg viewBox="0 0 24 24" width="26" height="26" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M9 5a3 3 0 0 1 6 0v3"/>
        <path d="M15 11a3 3 0 0 1-6 0V9"/>
        <path d="M19 11a7 7 0 0 1-11.3 5.5"/>
        <path d="M5 11c0 1.3.35 2.5.97 3.54"/>
        <path d="M12 18v3"/>
        <path d="M3 3l18 18"/>
      </svg>
    {/if}
  </div>
  <div class="label">{active && enabled ? 'Mikrofon aktif' : 'Mikrofon pause'}</div>
</button>

<style>
  .mic {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 8px;
    width: 148px; height: 148px;
    border-radius: 24px;
    border: 1px solid rgba(0,230,118,0.25);
    background: linear-gradient(180deg, rgba(18,26,46,0.95), rgba(12,18,34,0.95));
    color: #7a8aa8;
    cursor: pointer;
    transition: all 180ms ease;
    box-shadow: 0 10px 30px rgba(0,0,0,0.35);
  }
  .mic.active { color: #00e676; border-color: rgba(0,230,118,0.55); }
  .mic.glow { box-shadow: 0 0 0 1px rgba(0,230,118,0.15) inset, 0 0 28px rgba(0,230,118,0.22), 0 10px 30px rgba(0,0,0,0.35); }
  .mic:hover { transform: translateY(-1px); }
  .wave { display: inline-flex; align-items: flex-end; gap: 5px; height: 34px; }
  .bar {
    width: 6px; border-radius: 6px;
    min-height: 6px;
    animation: wave 0.8s ease-in-out infinite;
    opacity: .95;
  }
  @keyframes wave { 0%,100%{ transform: scaleY(.5);} 50%{ transform: scaleY(1.15);} }
  .icon { display:flex; align-items:center; justify-content:center; }
  .label { font-size: 12px; font-weight: 600; letter-spacing: .2px; }
</style>
