import { BulkBar } from './BulkBar';
import { useBulkSelection, selectionLimit, type BatchResult } from '../lib/bulkSelection';
import { bulkText } from '../lib/bulkI18n';
import { usePagedList, type ProjectRow } from '../lib/browse';
import { browseText } from '../lib/browseI18n';
import { ListControls, ListPager } from './ListControls';
import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Check, Folder, Link, Link2Off, Plus, Pencil, X, ArrowRightLeft, Trash2, Undo2 } from 'lucide-react';
import { refresh, useResearch } from '../lib/bridge';
import { refreshH5ad, useH5ad, workerActive } from '../lib/h5ad';
import { trashText } from '../lib/trashI18n';
import { projectText } from '../lib/projectI18n';
import type { Locale } from '../lib/types';
import { IconButton, Info } from './ui';
export function ProjectBrowser({locale,onClose,onSwitch}:{locale:Locale;onClose:()=>void;onSwitch:()=>void}){
 const t=projectText[locale],job=useH5ad();const dialog=useRef<HTMLDialogElement>(null);
 const [name,setName]=useState(''),[rename,setRename]=useState(''),[pending,setPending]=useState(false),[error,setError]=useState('');
 useEffect(()=>{dialog.current?.showModal();dialog.current?.querySelector<HTMLHeadingElement>('h2')?.focus();return()=>dialog.current?.close();},[]);
 const {document}=useResearch();
 const list=usePagedList<ProjectRow>('query_projects',job.projectKey,`${job.status}:${job.id}:${document?.revision}`);
 const bulk=useBulkSelection(`${job.projectKey}:projects:${list.query.trash}`);
 const [outcome,setOutcome]=useState<{scope:string;result:BatchResult}|null>(null);
 const b=bulkText[locale];
 const rows=list.data?.items??[];const tt=trashText[locale];const bt=browseText[locale];
 useEffect(()=>{setRename(document?.projectName??'');},[job.projectKey,document?.projectName]);
 async function run(command:string,args:Record<string,string|boolean>,switching:boolean){setPending(true);setError('');try{await invoke(command,args);await Promise.all([refresh(),refreshH5ad(true)]);if(switching){onSwitch();onClose();}else{if(command==='set_trash')bulk.remove(bulk.scope,[String(args.id)]);list.reload();}}catch(e){setError(String(e));}finally{setPending(false);}}
 async function applyBatch(){const owner=bulk.scope;setPending(true);setError('');setOutcome(null);try{const result=await invoke<BatchResult>('set_trash_batch',{projectKey:job.projectKey,kind:'project',ids:[...bulk.ids],restore:list.query.trash});bulk.remove(owner,result.succeeded);setOutcome({scope:owner,result});}catch(e){setError(String(e));}finally{await Promise.allSettled([refresh(),refreshH5ad(true)]);list.reload();setPending(false);}}
 const disabled=pending||workerActive(job.status);
 return <dialog className="project-browser" ref={dialog} onCancel={e=>{e.preventDefault();if(!pending)onClose();}} aria-labelledby="projects-title"><header><Folder size={19}/><h2 id="projects-title" tabIndex={-1}>{t.projects}</h2><Info text={t.switchHelp}/><IconButton label={t.close} disabled={pending} onClick={onClose}><X size={17}/></IconButton></header>
 {error||list.error?<p className="form-error" role="alert">{b[(error||list.error) as keyof typeof b]??tt[(error||list.error) as keyof typeof tt]??bt[(error||list.error) as keyof typeof bt]??t[(error||list.error) as keyof typeof t]??t.storage_error}</p>:null}
 <ListControls locale={locale} query={list.query} change={list.change} kind="projects" disabled={pending}/>
 <BulkBar locale={locale} selection={bulk} visible={rows.map(r=>r.id)} eligible={rows.filter(r=>!r.active).map(r=>r.id)} trash={list.query.trash} disabled={disabled} onApply={()=>void applyBatch()}/>
 {outcome?.scope===bulk.scope?<p className="bulk-outcome" role="status">{b.completed}: {outcome.result.succeeded.length} · {b.failed}: {outcome.result.failed.length}</p>:null}
 {outcome?.scope===bulk.scope&&outcome.result.failed.length?<details className="bulk-failures"><summary>{b.failed}</summary>{outcome.result.failed.map(f=><p key={f.id}><code>{f.id}</code> · {tt[f.error as keyof typeof tt]??t[f.error as keyof typeof t]??t.storage_error}</p>)}</details>:null}
 <div className="project-browser-list" aria-busy={pending||list.loading}>{rows.map(row=><article key={row.id} className={row.active?'selected-project':''}><div className="project-row-heading"><input type="checkbox" aria-label={`${b.selectItem}: ${row.name} ${row.id}`} checked={bulk.ids.has(row.id)} disabled={disabled||row.active||(!bulk.ids.has(row.id)&&bulk.ids.size>=selectionLimit)} onChange={()=>bulk.toggle(row.id)}/><h3>{row.name}</h3><code>{row.id==='local'?t.originalProject:row.id}</code>{!list.query.trash?<button className="button" disabled={disabled||row.active} onClick={()=>void run('switch_project',{id:row.id},true)}>{row.active?<Check size={14}/>:<ArrowRightLeft size={14}/>} {row.active?t.activeProject:t.switchProject}</button>:null}<IconButton label={list.query.trash?tt.restore:row.active?tt.active_project:tt.move} disabled={disabled||row.active} onClick={()=>void run('set_trash',{kind:'project',id:row.id,projectKey:job.projectKey,restore:list.query.trash},false)}>{list.query.trash?<Undo2 size={15}/>:<Trash2 size={15}/>}</IconButton></div><div className="project-row-details"><span>{t.historyCount}: <b>{row.historyCount}</b></span><span>{t.retained}: <b>{row.retained?.toLocaleString()??'—'}</b></span><span>{t.excluded}: <b>{row.excluded?.toLocaleString()??'—'}</b></span><span>{t.donorColumn}: <code>{row.donorColumn||'—'}</code></span><span>{row.sourceLinked?<Link size={12}/>:<Link2Off size={12}/>} {row.sourceLinked?t.linked:t.missing} {row.sourceName??''}</span></div>{row.selectedId?<small>{t.selectedResult}: <code>{row.selectedId.slice(0,8)}</code></small>:null}</article>)}{!list.data&&!list.error?<p role="status">{bt.loading}</p>:list.data&&!rows.length?<p>{list.query.trash&&list.data.unfilteredTotal===0?tt.empty:bt.noResults}</p>:null}</div>
 <ListPager locale={locale} data={list.data} change={list.change} disabled={pending}/>
 {!list.query.trash?<><form onSubmit={e=>{e.preventDefault();void run('create_project',{name},true);}}><label>{t.newProject}<input required maxLength={120} value={name} disabled={disabled} onChange={e=>setName(e.target.value)} placeholder={t.projectName}/></label><button className="button" disabled={disabled||!name.trim()}><Plus size={15}/>{t.createProject}</button></form>
 <form onSubmit={e=>{e.preventDefault();void run('rename_project',{name:rename},false);}}><label>{t.renameProject}<input required maxLength={120} value={rename} disabled={disabled} onChange={e=>setRename(e.target.value)}/></label><button className="button" disabled={disabled||!rename.trim()}><Pencil size={14}/>{t.saveName}</button></form></>:null}
 </dialog>;
}
