// Backend boundary for Nexo Windows UI (Phase 2).
// This interface is the contract the real nexo-core/Tauri backend must satisfy.
// UI and mock implementations depend only on this surface.

export type NexoState =
  | 'CONNECTING'
  | 'IDLE'
  | 'LISTENING'
  | 'PROCESSING'
  | 'ANSWERING'
  | 'PAUSED';

export type MascotExpressionId =
  | 'netral'
  | 'senang'
  | 'sangat_senang'
  | 'tertawa'
  | 'bingung'
  | 'mengantuk'
  | 'mengedip'
  | 'terkejut'
  | 'cinta'
  | 'jahil'
  | 'keren'
  | 'malu'
  | 'marah'
  | 'menangis'
  | 'sedih'
  | 'takut'
  | 'sleeping';

export interface VisualTelemetry {
  amplitude: number; // 0..1
  frequencyBands: number[]; // e.g. 4 bars
}

export interface TranscriptEntry {
  id: string;
  role: 'user' | 'assistant' | 'system';
  text: string;
  timestamp: number;
  final: boolean;
}

export interface BackendSnapshot {
  state: NexoState;
  expression: MascotExpressionId;
  telemetry: VisualTelemetry;
  transcript: TranscriptEntry[];
  micEnabled: boolean;
  connected: boolean;
  statusText: string;
  serverUrl: string;
  deviceId: string;
  hasToken: boolean;
}

export interface BackendSettings {
  serverUrl: string;
  token: string; // placeholder/redacted in UI; never logged
  deviceId?: string;
}

export interface NexoBackend {
  getSnapshot(): Promise<BackendSnapshot>;
  subscribe(listener: (snapshot: BackendSnapshot) => void): () => void;
  setMicEnabled(enabled: boolean): Promise<void>;
  togglePause(): Promise<void>;
  updateSettings(settings: BackendSettings): Promise<void>;
  openSettings?(): Promise<void>;
  quit?(): Promise<void>;
}

export const STATE_LABELS: Record<NexoState, string> = {
  CONNECTING: 'Menghubungkan',
  IDLE: 'Siaga',
  LISTENING: 'Menyimak',
  PROCESSING: 'Memproses',
  ANSWERING: 'Menjawab',
  PAUSED: 'Paused'
};

export const ALL_EXPRESSIONS: MascotExpressionId[] = [
  'netral', 'senang', 'sangat_senang', 'tertawa', 'bingung', 'mengantuk',
  'mengedip', 'terkejut', 'cinta', 'jahil', 'keren', 'malu', 'marah',
  'menangis', 'sedih', 'takut', 'sleeping'
];
