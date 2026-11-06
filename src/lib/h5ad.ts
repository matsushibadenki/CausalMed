import { invoke, isTauri } from '@tauri-apps/api/core';
import { useSyncExternalStore } from 'react';
export type QcOptions = { matrix:string; geneColumn:string; mitoPrefix:string; donorColumn:string; minGenes:number; maxGenes:number|null; minCounts:number; maxCounts:number|null; maxMitoPct:number|null };
export type Histogram = { counts:number[];edges:number[];log:boolean;median:number;min:number;max:number;lower:number|null;upper:number|null };
export type QcReport = { version:number;status:string;options:QcOptions;features:number;mitoFeatures:number;cells:number;retained:number|null;excluded:number|null;zeroCells:number|null;invalidValues:{nonfinite:number;negative:number;fractional:number};failures:Record<string,number>;metrics:Partial<Record<'counts'|'genes'|'mito',Histogram>>;donor:{column:string;distinct:number;missing:number;missingRetained:number|null;otherCells:number;groups:{label:string;cells:number;retained:number|null}[]} };
export type QcSelection = {id:string;createdAt:string;sha256:string;manifestSha256:string;retained:number;excluded:number};
export type ExportReceipt = {kind:string;selectionId:string;fileName:string;sha256:string;bytes:number;retained:number;excluded:number};
export type H5adReport = {
  cohortOrigins?:{definitionId:string;definitionSha256:string;sourceSha256:string;parentQcId:string;retained:number}[]; selection?:QcSelection; name: string; cells: number; genes: number; bytes?: number; sha256: string;
  duplicateCellIds: number; duplicateGeneIds: number; obsColumns: string[];
  anndataVersion?: string; matrices?: string[]; geneColumns?: string[]; rawGeneColumns?: string[]; qc?:QcReport;
  groups: Record<string, { complete?:boolean; distinct: number; missing: number; other?: number; counts: { label: string; count: number }[] }>;
};
export type ResourceLimits={memoryMib:number;cpuSeconds:number;threads:number};
export type H5adState = {
  runtime:{kind:string;pythonVersion:string|null;anndataVersion:string|null;error:string|null};
  resources:{residentBytes:number;peakResidentBytes:number;cpuMillis:number;samples:number};executionLimits:ResourceLimits|null;resourceSettings:{version:number;revision:number;limits:ResourceLimits};resourceSettingsInvalid:boolean;resourceMonitorSupported:boolean;
  projectKey:string;sourceLinked:boolean;sourceName:string|null;
  comparison:{leftId:string;rightId:string;comparable:boolean;bothRetained?:number;bothExcluded?:number;removed?:number;added?:number}|null;
  id: number; status: string; percent: number; phase: string; elapsedMs: number;
  timeoutSeconds: number; operation:string;lastExport:ExportReceipt|null;canAnalyze:boolean; error: string | null; report: H5adReport | null;
};
const initial: H5adState = {runtime:{kind:"unavailable",pythonVersion:null,anndataVersion:null,error:null},resources:{residentBytes:0,peakResidentBytes:0,cpuMillis:0,samples:0},executionLimits:null,resourceSettings:{version:1,revision:0,limits:{memoryMib:2048,cpuSeconds:300,threads:2}},resourceSettingsInvalid:false,resourceMonitorSupported:true, projectKey:'',sourceLinked:false,sourceName:null, comparison:null, id: 0, status: 'idle', percent: 0, phase: 'runtime', elapsedMs: 0, timeoutSeconds: 120, operation:'analysis',lastExport:null,canAnalyze:false, error: null, report: null };
let snapshot = initial;
const subscribers = new Set<() => void>();
let timer: ReturnType<typeof setTimeout> | undefined;
let request: Promise<void> | undefined;
export const workerActive = (status: string) => ['selecting', 'running', 'cancelling'].includes(status);
export function refreshH5ad(force=false): Promise<void> {
  if (!isTauri()) return Promise.resolve();
  if (request) return force ? request.then(()=>refreshH5ad()) : request;
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

export async function analyzeH5ad(options:QcOptions,sha256:string,timeoutSeconds:number) {
  await invoke('analyze_h5ad',{options,sha256,timeoutSeconds}); await refreshH5ad();
}

export async function exportH5adSelection(selectionId:string,kind:'export_qc'|'export_h5ad',timeoutSeconds:number) {
  await invoke('export_h5ad_selection',{selectionId,kind,timeoutSeconds}); await refreshH5ad();
}
