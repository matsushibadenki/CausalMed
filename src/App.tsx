import { useRef, useState, useEffect, useMemo } from 'react';
import { Layers, Database, FlaskConical, ChartNoAxesColumnIncreasing, Upload, Download, Network, Circle, PanelRight, ExternalLink } from 'lucide-react';
import { EvidenceExplorer } from './components/EvidenceExplorer';
import { Inspector } from './components/Inspector';
import { HypothesisDialog } from './components/HypothesisDialog';
import { Datasets,Hypotheses,BenchmarkView } from './components/ResearchViews';
import { ErrorBanner,IconButton,Info } from './components/ui';
import { native,useResearch,refresh,selectEdge,setLocale,setDock,importFile,setHypothesisStatus,runBenchmark,openGraph } from './lib/bridge';
import { errorMessage,localeLabels,translator } from './lib/i18n';
import type { Dock,Edge,Locale,View } from './lib/types';
import { invoke } from '@tauri-apps/api/core';

const navigation=[{id:'evidence',icon:Layers},{id:'datasets',icon:Database},{id:'hypotheses',icon:FlaskConical},{id:'benchmark',icon:ChartNoAxesColumnIncreasing}] as const;
const floating=new URLSearchParams(window.location.search).get('panel')==='inspector';

export default function App() {
  const {document,error:loadError}=useResearch();
  const [view,setView]=useState<View>('evidence');
  const [error,setError]=useState<unknown>(null);const [busy,setBusy]=useState(false);
  const [draftEdge,setDraftEdge]=useState<Edge|null>(null);const [dragging,setDragging]=useState(false);
  const upload=useRef<HTMLInputElement>(null);
  const locale=document?.workspace.locale??'ja';
  const t=useMemo(()=>translator(locale),[locale]);
  useEffect(()=>{window.document.documentElement.lang={en:'en',ja:'ja',zh:'zh-CN'}[locale];},[locale]);
  async function act(operation:()=>Promise<unknown>) {setError(null);setBusy(true);try {await operation();}catch(error){setError(error);}finally{setBusy(false);}}
  const onSelect=(id:string)=>void act(()=>selectEdge(id));
  const onDock=(dock:Dock)=>void act(()=>setDock(dock));
  if(!document) return <main className="loading-screen"><Network size={30}/><h1>CausalMed</h1><p>{loadError?errorMessage(loadError,locale):t('loading')}</p>{loadError?<button className="button" onClick={()=>void refresh()}>{t('retry')}</button>:null}</main>;
  const inspector=<Inspector document={document} t={t} onDock={onDock} onSelect={onSelect} onCreate={setDraftEdge} native={native} onDragChange={setDragging}/>;
  const modal=draftEdge?<HypothesisDialog edge={draftEdge} locale={locale} t={t} onClose={()=>setDraftEdge(null)}/>:null;
  const banner=error||loadError?<ErrorBanner message={errorMessage(error||loadError,locale)} closeLabel={t('close')} dismiss={()=>{setError(null);if(loadError)void refresh();}}/>:null;
  if(floating) return <div className="floating-shell">{banner}{inspector}{modal}</div>;
  const dock=document.workspace.inspectorDock;
  return <div className="app-shell" aria-busy={busy}>
    <aside className="sidebar">
      <div className="brand"><Network size={24} strokeWidth={2}/><span>CausalMed</span></div>
      <div className="project-label"><span className="project-initial">IP</span><span>{t('research')}</span></div>
      <nav aria-label="CausalMed">{navigation.map(({id,icon:Icon})=><button key={id} className={view===id?'active':''} aria-current={view===id?'page':undefined} title={t(id)} onClick={()=>setView(id)}><Icon size={19} strokeWidth={1.7}/><span>{t(id)}</span>{id==='hypotheses'&&document.hypotheses.length?<small>{document.hypotheses.length}</small>:null}</button>)}</nav>
      <div className="workspace-status"><span className="status-dot"/><span>{t(native?'local':'preview')}</span>{!native?<Info text={t('previewHelp')}/>:null}</div>
    </aside>
    <header className="topbar"><div className="breadcrumb"><span>{t('research')}</span><span>/</span><strong>{t(view)}</strong></div><div className="topbar-actions">{view!=='datasets'?<span className="demo-label"><Circle size={7} fill="currentColor"/>{t(document.sources.every(s=>s.synthetic)?'synthetic':'mixed')}</span>:null}<select aria-label="Language / 言語 / 语言" value={locale} onChange={event=>void act(()=>setLocale(event.target.value as Locale))}>{Object.entries(localeLabels).map(([code,label])=><option key={code} value={code}>{label}</option>)}</select><div className="toolbar-divider"/><IconButton label={native?t('export'):t('previewHelp')} disabled={!native||busy} onClick={()=>void act(()=>invoke('export_workspace'))}><Download size={18}/></IconButton><button className="button import-button" disabled={!native||busy} title={native?t('importHelp'):t('previewHelp')} onClick={()=>upload.current?.click()}><Upload size={17}/><span>{t('import')}</span></button></div></header>
    <div className={`workspace dock-${dock} ${view==='evidence'?'with-inspector':''}`}>
      <main className="main-content">{banner}
        {view==='evidence'?<EvidenceExplorer document={document} t={t} onSelect={onSelect} native={native} onGraph={()=>void act(openGraph)}/>:null}
        {view==='datasets'?<Datasets document={document} t={t}/>:null}
        {view==='hypotheses'?<Hypotheses document={document} t={t} busy={busy} onExplore={()=>setView('evidence')} onStatus={(id,status)=>void act(()=>setHypothesisStatus(id,status))}/>:null}
        {view==='benchmark'?<BenchmarkView document={document} t={t} native={native} busy={busy} onRun={()=>void act(runBenchmark)}/>:null}
        {view==='evidence'&&dock==='floating'?<div className="detached-notice"><ExternalLink size={17}/>{t('panelDetached')}<button className="button" onClick={()=>onDock('right')}><PanelRight size={16}/>{t('restorePanel')}</button></div>:null}
      </main>
      {view==='evidence'&&dock!=='floating'?inspector:null}
      {dragging?<div className="dock-targets">{(['right','bottom'] as const).map(target=><div key={target} className={`dock-target ${target}`} onDragOver={e=>e.preventDefault()} onDrop={e=>{e.preventDefault();setDragging(false);onDock(target);}}>{t(target==='right'?'dockRight':'dockBottom')}</div>)}</div>:null}
    </div>
    <footer className="statusbar"><span>{document.edges.length} {t('records')}<span className="status-separator">·</span>{document.sources.length} {t('sources')}</span><span>{busy?t('busy'):native?`${t('revision')} ${document.revision}`:t('preview')}<span className="status-separator">·</span>{t('researchOnly')}</span></footer>
    <input ref={upload} type="file" accept=".json,application/json" hidden onChange={event=>{const file=event.target.files?.[0];event.target.value='';if(file)void act(()=>importFile(file));}}/>
    {modal}
  </div>;
}
