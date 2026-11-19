/// <reference types="vite/client" />

declare namespace NodeJS {
  interface ProcessEnv {
    TAURI_DEV_HOST?: string;
  }
}
