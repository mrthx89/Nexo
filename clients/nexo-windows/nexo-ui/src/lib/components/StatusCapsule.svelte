<script lang="ts">
  import { STATE_LABELS, type NexoState, type VisualTelemetry } from '../backend/types';

  export let state: NexoState = 'CONNECTING';
  export let telemetry: VisualTelemetry = { amplitude: 0, frequencyBands: [0, 0, 0, 0] };
  export let compact = false;
  export let onToggle: (() => void) | null = null;

  $: label = STATE_LABELS[state] ?? state;
  $: bars = (telemetry.frequencyBands?.length ? telemetry.frequencyBands : [telemetry.amplitude, telemetry.amplitude, telemetry.amplitude, telemetry.amplitude]).slice(0, 4);
  $: active = state === 'LISTENING' || state === 'ANSWERING' || state === 'PROCESSING';
  $: color = state === 'LISTENING' ? '#00e676' : state === 'ANSWERING' ? '#00e5ff' : state === 'PROCESSING' ? '#ffb300' : state === 'PAUSED' ? '#7a8aa8' : state === 'CONNECTING' ? '#ffb300' : '#00e5ff';
  $: dotGlow = state === 'LISTENING' ? 'shadow-glowGreen' : 'shadow-glow';
</script>

<div class="capsule {compact ? 'compact' : ''}" style="--accent:{color}">
  <div class="dot {dotGlow}" class:pulse={active}></div>
  <div class="label">{label}</div>
  <div class="eq" aria-hidden="true">
    {#each bars as b, i}
      <span class="bar" style="height:{Math.max(10, Math.round(b * 100))}%; animation-delay:{i * 0.08}s"></span>
    {/each}
  </div>
  {#if onToggle}
    <button class="hide-btn" on:click={onToggle} title="Sembunyikan/Tampilkan">···</button>
  {/if}
</div>

<style>
  .capsule {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    padding: 8px 14px;
    border-radius: 999px;
    background: rgba(18, 26, 46, 0.82);
    border: 1px solid rgba(0,229,255,0.18);
    backdrop-filter: blur(12px);
    box-shadow: 0 8px 30px rgba(0,0,0,0.35), 0 0 0 1px rgba(255,255,255,0.03) inset;
    color: #e6eefc;
    min-width: 220px;
  }
  .capsule.compact { padding: 6px 12px; gap: 8px; min-width: 180px; }
  .dot {
    width: 10px; height: 10px; border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 10px var(--accent);
    flex: 0 0 auto;
  }
  .dot.pulse { animation: pulse 1.4s ease-in-out infinite; }
  @keyframes pulse { 0%,100%{ transform: scale(1); opacity: 1;} 50%{ transform: scale(1.35); opacity: .7;} }
  .label { font-size: 13px; font-weight: 600; letter-spacing: .2px; white-space: nowrap; }
  .eq { display: inline-flex; align-items: flex-end; gap: 3px; height: 16px; margin-left: 2px; }
  .bar {
    width: 3px; border-radius: 3px; background: var(--accent); opacity: .95;
    animation: bounce 0.9s ease-in-out infinite;
    min-height: 3px;
  }
  @keyframes bounce { 0%,100%{ transform: scaleY(.55);} 50%{ transform: scaleY(1.15);} }
  .hide-btn {
    margin-left: 4px; background: transparent; border: none; color: #7a8aa8; cursor: pointer; font-size: 16px; line-height: 1; padding: 0 2px;
  }
  .hide-btn:hover { color: #e6eefc; }
</style>
