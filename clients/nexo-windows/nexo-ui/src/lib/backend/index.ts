import { MockNexoBackend } from './mockBackend';
import type { BackendSettings, BackendSnapshot, NexoBackend } from './types';

class TauriNexoBackend implements NexoBackend {
  private inner = new MockNexoBackend();
  private tauriReady: Promise<boolean>;

  constructor() {
    this.tauriReady = this.tryInit();
  }

  private async tryInit(): Promise<boolean> {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('backend_get_snapshot');
      return true;
    } catch {
      return false;
    }
  }

  async getSnapshot(): Promise<BackendSnapshot> {
    if (await this.tauriReady) {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        return (await invoke('backend_get_snapshot')) as BackendSnapshot;
      } catch { /* fall through */ }
    }
    return this.inner.getSnapshot();
  }

  subscribe(listener: (snapshot: BackendSnapshot) => void): () => void {
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    (async () => {
      if (await this.tauriReady) {
        try {
          const { listen } = await import('@tauri-apps/api/event');
          const u = await listen<BackendSnapshot>('nexo://snapshot', (e) => listener(e.payload));
          if (cancelled) u(); else unlisten = u;
          // seed
          try { listener(await this.getSnapshot()); } catch {}
          return;
        } catch { /* fall through */ }
      }
      unlisten = this.inner.subscribe(listener);
    })();
    return () => { cancelled = true; if (unlisten) unlisten(); };
  }

  async setMicEnabled(enabled: boolean): Promise<void> {
    if (await this.tauriReady) {
      try { const { invoke } = await import('@tauri-apps/api/core'); await invoke('backend_set_mic', { enabled }); return; } catch {}
    }
    return this.inner.setMicEnabled(enabled);
  }

  async togglePause(): Promise<void> {
    if (await this.tauriReady) {
      try { const { invoke } = await import('@tauri-apps/api/core'); await invoke('backend_toggle_pause'); return; } catch {}
    }
    return this.inner.togglePause();
  }

  async updateSettings(settings: BackendSettings): Promise<void> {
    if (await this.tauriReady) {
      try { const { invoke } = await import('@tauri-apps/api/core'); await invoke('backend_update_settings', { settings }); return; } catch {}
    }
    return this.inner.updateSettings(settings);
  }
}

let _backend: NexoBackend | null = null;
export function getBackend(): NexoBackend {
  if (!_backend) _backend = new TauriNexoBackend();
  return _backend;
}

export * from './types';
export { MockNexoBackend };
