<script lang="ts">
  import { afterUpdate } from 'svelte';
  import type { TranscriptEntry } from '../backend/types';

  export let transcript: TranscriptEntry[] = [];
  let listEl: HTMLDivElement;
  let copiedId: string | null = null;

  afterUpdate(() => {
    if (listEl) listEl.scrollTop = listEl.scrollHeight;
  });

  async function copyText(text: string, id: string) {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      const ta = document.createElement('textarea');
      ta.value = text; document.body.appendChild(ta); ta.select();
      try { document.execCommand('copy'); } catch {}
      document.body.removeChild(ta);
    }
    copiedId = id;
    setTimeout(() => { if (copiedId === id) copiedId = null; }, 1200);
  }

  function fmt(ts: number) {
    try { return new Date(ts).toLocaleTimeString('id-ID', { hour: '2-digit', minute: '2-digit' }); } catch { return ''; }
  }
</script>

<div class="panel">
  <div class="header">
    <div class="title">Percakapan</div>
    <button class="copy-all" on:click={() => copyText(transcript.map(t => `[${t.role}] ${t.text}`).join('\n'), '__all')} title="Salin semua transkrip">
      {copiedId === '__all' ? 'Tersalin' : 'Salin semua'}
    </button>
  </div>
  <div class="list" bind:this={listEl}>
    {#if transcript.length === 0}
      <div class="empty">Belum ada transkrip.</div>
    {:else}
      {#each transcript as item (item.id)}
        <div class="msg {item.role}">
          <div class="meta">
            <span class="role">{item.role === 'user' ? 'Anda' : item.role === 'assistant' ? 'Nexo' : 'Sistem'}</span>
            <span class="time">{fmt(item.timestamp)}</span>
            <button class="copy" on:click={() => copyText(item.text, item.id)} title="Salin">{copiedId === item.id ? '✓' : 'Salin'}</button>
          </div>
          <div class="text">{item.text}</div>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .panel {
    display: flex; flex-direction: column; height: 100%;
    background: rgba(18,26,46,0.72);
    border: 1px solid rgba(255,255,255,0.06);
    border-radius: 18px;
    overflow: hidden;
    backdrop-filter: blur(10px);
  }
  .header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 12px 14px;
    border-bottom: 1px solid rgba(255,255,255,0.06);
  }
  .title { font-weight: 700; font-size: 14px; letter-spacing: .2px; }
  .copy-all, .copy {
    background: rgba(0,229,255,0.08); color: #00e5ff; border: 1px solid rgba(0,229,255,0.2);
    border-radius: 8px; padding: 4px 8px; font-size: 11px; cursor: pointer;
  }
  .copy-all:hover, .copy:hover { background: rgba(0,229,255,0.14); }
  .list { flex: 1; overflow: auto; padding: 12px; display: flex; flex-direction: column; gap: 10px; }
  .empty { color: #7a8aa8; font-size: 13px; text-align: center; padding: 20px 0; }
  .msg { border-radius: 14px; padding: 10px 12px; border: 1px solid rgba(255,255,255,0.05); }
  .msg.user { background: rgba(0,229,255,0.08); align-self: stretch; }
  .msg.assistant { background: rgba(0,230,118,0.06); }
  .msg.system { background: rgba(255,179,0,0.06); }
  .meta { display:flex; align-items:center; gap:8px; margin-bottom: 4px; color: #7a8aa8; font-size: 11px; }
  .role { font-weight: 700; color: #cfe3ff; }
  .time { margin-left: auto; }
  .text { font-size: 13px; line-height: 1.45; white-space: pre-wrap; word-break: break-word; }
</style>
