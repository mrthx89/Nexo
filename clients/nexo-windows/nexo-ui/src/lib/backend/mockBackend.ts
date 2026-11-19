import type {
  BackendSnapshot,
  BackendSettings,
  MascotExpressionId,
  NexoBackend,
  NexoState,
  TranscriptEntry,
  VisualTelemetry
} from './types';

const STATES: NexoState[] = ['CONNECTING', 'IDLE', 'LISTENING', 'PROCESSING', 'ANSWERING', 'PAUSED'];

function clamp01(v: number) { return Math.max(0, Math.min(1, v)); }

function exprForState(state: NexoState): MascotExpressionId {
  switch (state) {
    case 'CONNECTING': return 'bingung';
    case 'IDLE': return 'netral';
    case 'LISTENING': return 'netral';
    case 'PROCESSING': return 'bingung';
    case 'ANSWERING': return 'senang';
    case 'PAUSED': return 'mengantuk';
    default: return 'netral';
  }
}

export class MockNexoBackend implements NexoBackend {
  private listeners = new Set<(s: BackendSnapshot) => void>();
  private timer: ReturnType<typeof setInterval> | null = null;
  private tick = 0;
  private stateIndex = 0;
  private autoCycle = true;
  private transcript: TranscriptEntry[] = [
    { id: 't1', role: 'assistant', text: 'Halo, saya Nexo. Mode simulasi UI aktif (mock backend).', timestamp: Date.now() - 15000, final: true },
    { id: 't2', role: 'user', text: 'Nexo, tampilkan status kiosk.', timestamp: Date.now() - 9000, final: true },
    { id: 't3', role: 'assistant', text: 'Kiosk siap. Capsule status dan panel percakapan tersambung ke state machine.', timestamp: Date.now() - 4000, final: true }
  ];
  private snapshot: BackendSnapshot = {
    state: 'CONNECTING',
    expression: 'bingung',
    telemetry: { amplitude: 0.05, frequencyBands: [0.1, 0.15, 0.12, 0.08] },
    transcript: this.transcript,
    micEnabled: true,
    connected: false,
    statusText: 'Menghubungkan ke layanan (simulasi)…',
    serverUrl: 'wss://nexo.yhdev.my.id/ws/appliance',
    deviceId: 'nexo-windows-mock',
    hasToken: false
  };

  constructor() {
    this.start();
  }

  private start() {
    if (this.timer) return;
    this.timer = setInterval(() => this.step(), 900);
  }

  private step() {
    this.tick++;
    if (this.autoCycle && this.tick % 5 === 0) {
      this.stateIndex = (this.stateIndex + 1) % STATES.length;
    }
    const state = STATES[this.stateIndex];
    const connected = state !== 'CONNECTING';
    const baseAmp = state === 'LISTENING' ? 0.35 : state === 'ANSWERING' ? 0.55 : state === 'PROCESSING' ? 0.18 : 0.06;
    const phase = this.tick * 0.9;
    const telemetry: VisualTelemetry = {
      amplitude: clamp01(baseAmp + Math.sin(phase) * 0.18),
      frequencyBands: [0, 1, 2, 3].map((i) => clamp01(baseAmp * (0.5 + 0.15 * i) + Math.abs(Math.sin(phase + i * 0.8)) * 0.35))
    };
    if (state === 'LISTENING' && this.tick % 10 === 0) {
      this.pushTranscript('user', 'Contoh input suara (simulasi).');
    }
    if (state === 'ANSWERING' && this.tick % 10 === 5) {
      this.pushTranscript('assistant', 'Respons simulasi: state machine UI aktif dan ekspresi mengikuti status.');
    }
    this.snapshot = {
      ...this.snapshot,
      state,
      connected,
      expression: exprForState(state),
      telemetry,
      statusText: connected ? `Status: ${state}` : 'Menghubungkan ke layanan (simulasi)…',
      transcript: [...this.transcript]
    };
    this.emit();
  }

  private pushTranscript(role: TranscriptEntry['role'], text: string) {
    const entry: TranscriptEntry = { id: `m${Date.now()}${Math.random()}`, role, text, timestamp: Date.now(), final: true };
    this.transcript = [...this.transcript.slice(-49), entry];
  }

  private emit() {
    for (const l of this.listeners) l({ ...this.snapshot, transcript: [...this.snapshot.transcript] });
  }

  async getSnapshot(): Promise<BackendSnapshot> {
    return { ...this.snapshot, transcript: [...this.snapshot.transcript] };
  }

  subscribe(listener: (snapshot: BackendSnapshot) => void): () => void {
    this.listeners.add(listener);
    listener({ ...this.snapshot, transcript: [...this.snapshot.transcript] });
    return () => this.listeners.delete(listener);
  }

  async setMicEnabled(enabled: boolean): Promise<void> {
    this.snapshot = { ...this.snapshot, micEnabled: enabled };
    if (!enabled) {
      this.autoCycle = false;
      this.stateIndex = STATES.indexOf('PAUSED');
    } else if (this.snapshot.state === 'PAUSED') {
      this.autoCycle = true;
      this.stateIndex = STATES.indexOf('IDLE');
    }
    this.emit();
  }

  async togglePause(): Promise<void> {
    const paused = this.snapshot.state === 'PAUSED';
    await this.setMicEnabled(paused);
  }

  async updateSettings(settings: BackendSettings): Promise<void> {
    this.snapshot = {
      ...this.snapshot,
      serverUrl: settings.serverUrl || this.snapshot.serverUrl,
      deviceId: settings.deviceId || this.snapshot.deviceId,
      hasToken: !!settings.token && settings.token !== '[REDACTED]'
    };
    this.pushTranscript('system', 'Pengaturan diperbarui (token/URL) — diterapkan ke backend saat integrasi nyata.');
    this.emit();
  }
}
