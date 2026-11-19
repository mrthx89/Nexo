<script lang="ts">
  import { onMount } from 'svelte';
  import { mascotSrc } from '../assets/mascots';
  import type { MascotExpressionId, NexoState, VisualTelemetry } from '../backend/types';

  export let expression: MascotExpressionId = 'netral';
  export let state: NexoState = 'IDLE';
  export let telemetry: VisualTelemetry = { amplitude: 0, frequencyBands: [0, 0, 0, 0] };

  let tick = 0;
  let shiftX = 0, shiftY = 0;
  let gazeX = 0, gazeY = 0;

  onMount(() => {
    const t = setInterval(() => {
      tick = (tick + 1) % 100000;
      // burn-in pixel shift every ~210s in concept; here subtle continuous micro-shift cycle
      const phase = Math.floor(tick / 180) % 8;
      const offsets = [[0,0],[1,0],[1,1],[0,1],[-1,1],[-1,-1],[1,-1],[1,0]];
      [shiftX, shiftY] = offsets[phase];
      // subtle saccade-like drift
      gazeX = Math.sin(tick * 0.07) * 2.2;
      gazeY = Math.cos(tick * 0.05) * 1.6;
    }, 1000);
    return () => clearInterval(t);
  });

  $: src = mascotSrc[expression] || mascotSrc.netral;
  $: amp = Math.max(0, Math.min(1, telemetry.amplitude || 0));
  $: speaking = state === 'ANSWERING';
  $: listening = state === 'LISTENING';
  $: glow = speaking ? 'rgba(0,229,255,0.55)' : listening ? 'rgba(0,230,118,0.45)' : 'rgba(0,229,255,0.22)';
  $: scale = 1 + (speaking ? amp * 0.06 : listening ? amp * 0.03 : 0);
  $: bob = speaking ? Math.sin(tick * 0.35) * (2 + amp * 6) : Math.sin(tick * 0.12) * 2.5;
</script>

<div class="stage">
  <div class="aura" style="--glow:{glow}"></div>
  <div class="mascot-wrap" style="transform: translate({shiftX + gazeX}px, {shiftY + gazeY + bob}px) scale({scale});">
    <img class="mascot" {src} alt="Nexo {expression}" draggable="false" />
    <div class="eye-glow left" class:on={speaking || listening}></div>
    <div class="eye-glow right" class:on={speaking || listening}></div>
  </div>
  <div class="expr-tag">{expression}</div>
</div>

<style>
  .stage {
    position: relative;
    width: 100%;
    height: 100%;
    min-height: 320px;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }
  .aura {
    position: absolute;
    width: min(70vmin, 520px);
    height: min(70vmin, 520px);
    border-radius: 50%;
    background: radial-gradient(circle, var(--glow) 0%, rgba(0,229,255,0.06) 45%, rgba(0,0,0,0) 72%);
    filter: blur(8px);
    transition: background 400ms ease;
  }
  .mascot-wrap {
    position: relative;
    width: min(58vmin, 420px);
    height: min(58vmin, 420px);
    transition: transform 280ms ease;
    will-change: transform;
  }
  .mascot {
    width: 100%; height: 100%; object-fit: contain;
    user-select: none;
    filter: drop-shadow(0 18px 40px rgba(0,0,0,0.45));
  }
  .eye-glow {
    position: absolute;
    width: 12%; height: 10%;
    top: 42%;
    border-radius: 50%;
    background: radial-gradient(circle, rgba(0,229,255,0.0) 0%, rgba(0,229,255,0.0) 100%);
    pointer-events: none;
    transition: all 220ms ease;
    mix-blend-mode: screen;
  }
  .eye-glow.left { left: 32%; }
  .eye-glow.right { right: 32%; }
  .eye-glow.on {
    background: radial-gradient(circle, rgba(0,229,255,0.85) 0%, rgba(0,229,255,0.15) 55%, rgba(0,229,255,0) 100%);
    filter: blur(2px);
  }
  .expr-tag {
    position: absolute;
    bottom: 14px;
    left: 50%; transform: translateX(-50%);
    font-size: 12px; color: #7a8aa8;
    background: rgba(18,26,46,0.55);
    border: 1px solid rgba(255,255,255,0.05);
    padding: 4px 10px; border-radius: 999px;
    text-transform: lowercase;
  }
</style>
