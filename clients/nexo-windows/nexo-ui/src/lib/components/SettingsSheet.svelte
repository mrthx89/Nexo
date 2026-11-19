<script lang="ts">
  import { createEventDispatcher, onMount } from 'svelte';
  import { nexo } from '../stores/nexo';
  import { autostartEnabled, capsuleOnMinimize } from '../stores/ui';

  export let open = false;
  export let serverUrl = '';
  export let deviceId = '';
  export let hasToken = false;

  let token = '';
  let url = serverUrl;
  let saving = false;
  let showToken = false;
  let autostartBusy = false;
  let capsuleBusy = false;
  let tauriAvailable = false;
  const dispatch = createEventDispatcher();

  async function refreshTauriState() {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      tauriAvailable = true;
      try {
        const ui = await invoke<{ capsuleOnMinimize: boolean }>('app_get_ui_settings');
        capsuleOnMinimize.set(!!ui.capsuleOnMinimize);
      } catch {}
      try {
        const { isEnabled } = await import('@tauri-apps/plugin-autostart');
        autostartEnabled.set(await isEnabled());
      } catch {}
    } catch {
      tauriAvailable = false;
    }
  }

  $: if (open) {
    url = serverUrl;
    token = '';
    refreshTauriState();
  }

  async function save() {
    saving = true;
    try {
      await nexo.updateSettings({
        serverUrl: url,
        token: token || (hasToken ? '[REDACTED]' : ''),
        deviceId
      });
      dispatch('close');
    } finally {
      saving = false;
    }
  }

  async function toggleAutostart() {
    if (!tauriAvailable) return;
    autostartBusy = true;
    try {
      const { enable, disable, isEnabled } = await import('@tauri-apps/plugin-autostart');
      const enabled = await isEnabled();
      if (enabled) {
        await disable();
        autostartEnabled.set(false);
      } else {
        await enable();
        autostartEnabled.set(true);
      }
    } catch (e) {
      console.error('autostart toggle failed', e);
    } finally {
      autostartBusy = false;
    }
  }

  async function toggleCapsuleOnMinimize() {
    const next = !$capsuleOnMinimize;
    capsuleBusy = true;
    capsuleOnMinimize.set(next);
    if (tauriAvailable) {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        await invoke('app_set_capsule_on_minimize', { enabled: next });
      } catch (e) {
        console.error('capsule preference save failed', e);
      }
    }
    capsuleBusy = false;
  }

  async function requestQuit() {
    if (!tauriAvailable) return;
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('app_quit');
    } catch {}
  }
</script>

{#if open}
  <div
    class="backdrop"
    on:click={() => dispatch('close')}
    on:keydown={(e) => e.key === 'Escape' && dispatch('close')}
    role="button"
    tabindex="0"
  ></div>
  <div class="sheet" role="dialog" aria-label="Pengaturan">
    <div class="handle"></div>
    <div class="head">
      <h2>Pengaturan</h2>
      <button class="x" on:click={() => dispatch('close')}>✕</button>
    </div>
    <div class="body">
      <label>
        <span>Endpoint WSS</span>
        <input
          type="text"
          bind:value={url}
          placeholder="wss://nexo.yhdev.my.id/ws/appliance"
          spellcheck="false"
        />
      </label>

      <label>
        <span>Token Bearer {hasToken ? '(tersimpan)' : '(belum diset)'}</span>
        <div class="token-row">
          {#if showToken}
            <input
              type="text"
              bind:value={token}
              placeholder={hasToken ? 'Kosongkan untuk mempertahankan token tersimpan' : 'Masukkan token bearer'}
              spellcheck="false"
              autocomplete="off"
            />
          {:else}
            <input
              type="password"
              bind:value={token}
              placeholder={hasToken ? 'Kosongkan untuk mempertahankan token tersimpan' : 'Masukkan token bearer'}
              autocomplete="off"
            />
          {/if}
          <button class="ghost" on:click={() => (showToken = !showToken)}
            >{showToken ? 'Sembunyikan' : 'Tampil'}</button
          >
        </div>
        <small
          >Token tidak dicatat di log UI. Untuk integrasi nyata, token diteruskan ke backend
          nexo-core secara aman.</small
        >
      </label>

      <div class="meta">Device ID: <code>{deviceId}</code></div>

      <div class="section">
        <div class="section-title">Perilaku tray & jendela (Phase 3)</div>

        <div class="toggle-row">
          <div class="toggle-text">
            <div class="toggle-label">Mulai otomatis saat login</div>
            <div class="toggle-desc">
              Aktifkan autostart Windows (plugin Tauri autostart). Nexo dapat mulai diminimalkan ke tray
              dengan argumen <code>--minimized</code>.
            </div>
          </div>
          <button
            class="switch"
            class:on={$autostartEnabled}
            disabled={!tauriAvailable || autostartBusy}
            on:click={toggleAutostart}
            aria-pressed={$autostartEnabled}
          >
            <span class="knob"></span>
          </button>
        </div>

        <div class="toggle-row">
          <div class="toggle-text">
            <div class="toggle-label">Tampilkan capsule saat minimize/ke tray</div>
            <div class="toggle-desc">
              Saat jendela utama diminimize atau disembunyikan ke tray, jendela capsule/notch always-on-top
              tetap tampil (Dynamic Island). Matikan jika ingin capsule ikut tersembunyi.
            </div>
          </div>
          <button
            class="switch"
            class:on={$capsuleOnMinimize}
            disabled={capsuleBusy}
            on:click={toggleCapsuleOnMinimize}
            aria-pressed={$capsuleOnMinimize}
          >
            <span class="knob"></span>
          </button>
        </div>

        <div class="toggle-row quit-row">
          <div class="toggle-text">
            <div class="toggle-label">Keluar dari Nexo</div>
            <div class="toggle-desc">
              Menutup aplikasi dengan konfirmasi, melepas mikrofon, dan menutup koneksi WS dengan bersih
              (saat integrasi nexo-core nyata).
            </div>
          </div>
          <button class="danger" disabled={!tauriAvailable} on:click={requestQuit}>Keluar…</button>
        </div>

        {#if !tauriAvailable}
          <div class="note">Beberapa opsi (autostart/quit native/persistensi tray) aktif saat berjalan di Tauri (Windows).</div>
        {/if}
      </div>
    </div>
    <div class="foot">
      <button class="secondary" on:click={() => dispatch('close')}>Batal</button>
      <button class="primary" on:click={save} disabled={saving}>{saving ? 'Menyimpan…' : 'Simpan'}</button>
    </div>
  </div>
{/if}

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(5,8,16,0.55); backdrop-filter: blur(2px); z-index: 40; }
  .sheet {
    position: fixed; left: 50%; bottom: 0; transform: translateX(-50%);
    width: min(760px, 100vw); max-height: 88vh; overflow: auto;
    background: #121a2e; border: 1px solid rgba(255,255,255,0.08);
    border-bottom: none; border-radius: 20px 20px 0 0;
    box-shadow: 0 -20px 60px rgba(0,0,0,0.5); z-index: 50;
  }
  .handle { width: 48px; height: 5px; border-radius: 999px; background: rgba(255,255,255,0.18); margin: 10px auto 0; }
  .head { display:flex; align-items:center; justify-content:space-between; padding: 10px 18px 6px; }
  h2 { margin: 0; font-size: 18px; }
  .x { background: transparent; border: none; color: #7a8aa8; font-size: 18px; cursor: pointer; }
  .body { padding: 8px 18px 18px; display:flex; flex-direction:column; gap: 14px; }
  label { display:flex; flex-direction:column; gap: 6px; font-size: 13px; color: #cfe3ff; }
  label > span { font-weight: 600; }
  input {
    background: #0b1020; color: #e6eefc; border: 1px solid rgba(0,229,255,0.18);
    border-radius: 12px; padding: 10px 12px; font-size: 14px; outline: none; width: 100%;
  }
  input:focus { border-color: rgba(0,229,255,0.5); box-shadow: 0 0 0 3px rgba(0,229,255,0.12); }
  .token-row { display:flex; gap: 8px; align-items:center; }
  .ghost { background: rgba(255,255,255,0.06); color:#cfe3ff; border:1px solid rgba(255,255,255,0.1); border-radius:10px; padding: 9px 10px; cursor:pointer; white-space:nowrap; }
  small { color: #7a8aa8; line-height: 1.35; }
  .meta { color:#7a8aa8; font-size:12px; }
  code { color:#00e5ff; }
  .section { margin-top: 4px; padding: 14px; border-radius: 14px; background: rgba(11,16,32,0.55); border: 1px solid rgba(255,255,255,0.06); display:flex; flex-direction:column; gap: 12px; }
  .section-title { font-weight: 700; font-size: 13px; color: #e6eefc; }
  .toggle-row { display:flex; align-items:center; justify-content:space-between; gap: 14px; }
  .toggle-text { flex: 1; }
  .toggle-label { font-size: 13px; font-weight: 600; color: #e6eefc; margin-bottom: 2px; }
  .toggle-desc { font-size: 12px; color: #7a8aa8; line-height: 1.4; }
  .switch { width: 46px; height: 26px; border-radius: 999px; background: rgba(255,255,255,0.12); border: 1px solid rgba(255,255,255,0.12); position: relative; cursor: pointer; flex: 0 0 auto; transition: all 160ms ease; }
  .switch .knob { position: absolute; top: 2px; left: 2px; width: 20px; height: 20px; border-radius: 50%; background: #cfe3ff; transition: transform 160ms ease; }
  .switch.on { background: rgba(0,230,118,0.25); border-color: rgba(0,230,118,0.5); }
  .switch.on .knob { transform: translateX(20px); background: #00e676; }
  .switch:disabled { opacity: .5; cursor: default; }
  .danger { background: rgba(255,82,82,0.12); color: #ff8a80; border: 1px solid rgba(255,82,82,0.35); border-radius: 10px; padding: 8px 12px; font-weight: 600; cursor: pointer; }
  .danger:disabled { opacity: .5; cursor: default; }
  .note { font-size: 12px; color: #ffb300; }
  .quit-row { border-top: 1px solid rgba(255,255,255,0.06); padding-top: 10px; margin-top: 2px; }
  .foot { display:flex; gap: 10px; justify-content:flex-end; padding: 12px 18px 18px; border-top: 1px solid rgba(255,255,255,0.06); }
  .primary, .secondary { border-radius: 12px; padding: 10px 14px; font-weight: 600; cursor: pointer; border: 1px solid transparent; }
  .primary { background: #00e5ff; color: #041018; }
  .primary:disabled { opacity: .6; cursor: default; }
  .secondary { background: rgba(255,255,255,0.06); color: #e6eefc; border-color: rgba(255,255,255,0.1); }
</style>
