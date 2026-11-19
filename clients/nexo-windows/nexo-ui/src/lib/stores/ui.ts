import { writable } from 'svelte/store';

export const settingsOpen = writable(false);
export const capsuleVisible = writable(true);
export const capsuleOnMinimize = writable(true);
export const autostartEnabled = writable(false);
export const demoMode = writable(true); // simulation banner/helper
