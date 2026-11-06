import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useSyncExternalStore } from 'react';
import fixture from '../../fixtures/evidence.synthetic.json';
import type { ResearchDocument, Edge, Locale, Dock, NewHypothesis, HypothesisStatus } from './types';

export const native = isTauri();
const preview: ResearchDocument = {
  schemaVersion:1,revision:0,projectName:'IPF',sources:fixture.sources,edges:fixture.edges as Edge[],
  evaluationPlan:{spec:{featureId:"",featureColumn:"_index"},measurementEvidence:"",reviewedFor:null},evaluations:[],cohortDefinitions:[],cohorts:[],datasets:[],imports:[],hypotheses:[],benchmarks:[],activity:[],
  workspace:{ locale:'ja',selectedEdgeId:'E-001',inspectorDock:'right' },
};
let snapshot: { document: ResearchDocument | null; error: unknown } = { document:native ? null : preview, error:null };
const subscribers = new Set<() => void>();
function notify() { subscribers.forEach(fn=>fn()); }
function accept(document: ResearchDocument) {
  if (!snapshot.document || document.revision >= snapshot.document.revision) { snapshot = { document,error:null }; notify(); }
}
export async function refresh() {
  if (!native) return;
  try { accept(await invoke<ResearchDocument>('get_document')); }
  catch (error) { snapshot={...snapshot,error}; notify(); }
}
let started=false;
export function initialize() {
  if (started || !native) return; started=true;
  // Subscribe before requesting the initial snapshot; revision checks reject stale replies.
  void listen<number>('document-changed',()=>void refresh()).then(()=>refresh()).catch(error=>{ snapshot={...snapshot,error};notify(); });
}
export function useResearch() { return useSyncExternalStore(fn=>{subscribers.add(fn);return()=>subscribers.delete(fn);},()=>snapshot); }
async function mutate(command:string,args?:Record<string,unknown>) {
  if (!native) throw new Error('desktop_required');
  const document=await invoke<ResearchDocument>(command,args);accept(document);return document;
}
export async function selectEdge(id:string) {
  if (native) return mutate('select_edge',{id});
  // Browser-only view projection, never persisted or used for research operations.
  if (snapshot.document) accept({...snapshot.document,workspace:{...snapshot.document.workspace,selectedEdgeId:id}});
}
export async function setLocale(locale:Locale) {
  if(native) return mutate('set_locale',{locale});
  if(snapshot.document) accept({...snapshot.document,workspace:{...snapshot.document.workspace,locale}});
}
export const setDock=(dock:Dock)=>mutate('set_inspector_dock',{dock});
export const createHypothesis=(input:NewHypothesis)=>mutate('create_hypothesis',{input});
export const setHypothesisStatus=(id:string,status:HypothesisStatus)=>mutate('set_hypothesis_status',{id,status});
export const runBenchmark=()=>mutate('run_benchmark');
export const openGraph=()=>invoke<void>('open_graph');
export async function importFile(file:File) {
  if(file.size>2*1024*1024) throw new Error('import_too_large');
  return mutate('import_evidence',{json:await file.text()});
}
