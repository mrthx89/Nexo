import { writable, derived } from 'svelte/store';
import { getBackend } from '../backend';
import type { BackendSnapshot } from '../backend/types';

const initial: BackendSnapshot = {
  state: 'CONNECTING',
  expression: 'bingung',
  telemetry: { amplitude: 0, frequencyBands: [0, 0, 0, 0] },
  transcript: [],
  micEnabled: true,
  connected: false,
  statusText: 'Menghubungkan…',
  serverUrl: 'wss://nexo.yhdev.my.id/ws/appliance',
  deviceId: 'nexo-windows',
  hasToken: false
};

function createNexoStore() {
  const { subscribe, set } = writable<BackendSnapshot>(initial);
  let unsub: (() => void) | null = null;

  function init() {
    if (unsub) return;
    const backend = getBackend();
    unsub = backend.subscribe((snap) => set(snap));
  }

  return {
    subscribe,
    init,
    async setMicEnabled(enabled: boolean) { await getBackend().setMicEnabled(enabled); },
    async togglePause() { await getBackend().togglePause(); },
    async updateSettings(settings: { serverUrl: string; token: string; deviceId?: string }) {
      await getBackend().updateSettings(settings);
    }
  };
}

export const nexo = createNexoStore();
export const telemetry = derived(nexo, ($n) => $n.telemetry);
export const transcript = derived(nexo, ($n) => $n.transcript);
