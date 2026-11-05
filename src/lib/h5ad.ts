import { invoke, isTauri } from '@tauri-apps/api/core';
import { useSyncExternalStore } from 'react';
export type H5adReport = {
  name: string; cells: number; genes: number; bytes?: number; sha256: string;
  duplicateCellIds: number; duplicateGeneIds: number; obsColumns: string[];
  anndataVersion?: string;
  groups: Record<string, { distinct: number; missing: number; other?: number; counts: { label: string; count: number }[] }>;
};
export type H5adState = {
  id: number; status: string; percent: number; phase: string; elapsedMs: number;
  timeoutSeconds: number; error: string | null; report: H5adReport | null;
};
const initial: H5adState = { id: 0, status: 'idle', percent: 0, phase: 'runtime', elapsedMs: 0, timeoutSeconds: 120, error: null, report: null };
let snapshot = initial;
const subscribers = new Set<() => void>();
let timer: ReturnType<typeof setTimeout> | undefined;
let request: Promise<void> | undefined;
export const workerActive = (status: string) => ['selecting', 'running', 'cancelling'].includes(status);
export function refreshH5ad(): Promise<void> {
  if (!isTauri()) return Promise.resolve();
  if (request) return request;
  request = invoke<H5adState>('get_h5ad_state').then(value => {
    snapshot = value; subscribers.forEach(fn => fn());
  }).finally(() => { request = undefined; });
  return request;
}
let pollVersion = 0;
function poll(version: number) {
  void refreshH5ad().catch(() => {}).finally(() => {
    if (subscribers.size && version === pollVersion) timer = setTimeout(() => poll(version), workerActive(snapshot.status) ? 150 : 1000);
  });
}
function subscribe(fn: () => void) {
  subscribers.add(fn);
  if (subscribers.size === 1) poll(++pollVersion);
  return () => {
    subscribers.delete(fn);
    if (!subscribers.size) { ++pollVersion; clearTimeout(timer); }
  };
}
export function useH5ad() { return useSyncExternalStore(subscribe, () => snapshot); }
export async function startH5ad(timeoutSeconds: number) {
  await invoke('import_h5ad', { timeoutSeconds }); await refreshH5ad();
}
export async function cancelH5ad() { await invoke('cancel_h5ad'); await refreshH5ad(); }
