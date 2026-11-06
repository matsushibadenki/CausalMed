import { BulkBar } from './BulkBar';
import { useBulkSelection, selectionLimit, type BatchResult } from '../lib/bulkSelection';
import { bulkText } from '../lib/bulkI18n';
import { usePagedList, type HistoryRow } from '../lib/browse';
import { browseText } from '../lib/browseI18n';
import { ListControls, ListPager } from './ListControls';
import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { History, Check, ArrowLeftRight, Trash2, Undo2 } from 'lucide-react';
import { refreshH5ad, type H5adState, type QcOptions } from '../lib/h5ad';
import { trashText } from '../lib/trashI18n';
import { historyText } from '../lib/historyI18n';
import type { Locale } from '../lib/types';
import { Info, IconButton } from './ui';
export function QcHistory({locale,job,active,sourceSha256=''}:{locale:Locale;job:H5adState;active:boolean;sourceSha256?:string}) {
 const t=historyText[locale],tt=trashText[locale];
 const list=usePagedList<HistoryRow>('query_qc_history',job.projectKey,`${job.id}:${job.status}:${job.report?.selection?.id}`,sourceSha256);
 const bulk=useBulkSelection(`${job.projectKey}:history:${list.query.trash}`);
 const [outcome,setOutcome]=useState<{scope:string;result:BatchResult}|null>(null);
 const bx=bulkText[locale];
 const runs=list.data?.items??[],bt=browseText[locale];
 const [a,setA]=useState<HistoryRow|null>(null),[b,setB]=useState<HistoryRow|null>(null),[error,setError]=useState(''),[pending,setPending]=useState(false);
 const left=a?.id??'',right=b?.id??'';
 const comparison=job.comparison?.leftId===left && job.comparison?.rightId===right ? job.comparison : null;
 async function action(command:string,args:Record<string,string|boolean>){setPending(true);setError('');try{await invoke(command,{...args,projectKey:job.projectKey});await refreshH5ad(true);if(command==='set_trash'){setA(null);setB(null);bulk.remove(bulk.scope,[String(args.id)]);}list.reload();}catch(e){setError(String(e));}finally{setPending(false);}}
 async function applyBatch(){const owner=bulk.scope;setPending(true);setError('');setOutcome(null);try{const result=await invoke<BatchResult>('set_trash_batch',{projectKey:job.projectKey,kind:'history',ids:[...bulk.ids],restore:list.query.trash});bulk.remove(owner,result.succeeded);setA(null);setB(null);setOutcome({scope:owner,result});}catch(e){setError(String(e));}finally{await Promise.allSettled([refreshH5ad(true)]);list.reload();setPending(false);}}
 const fields: [keyof QcOptions,keyof typeof t][]=[['matrix','matrix'],['donorColumn','donor'],['geneColumn','geneColumn'],['mitoPrefix','mitoPrefix'],['minGenes','minGenes'],['maxGenes','maxGenes'],['minCounts','minCounts'],['maxCounts','maxCounts'],['maxMitoPct','maxMitoPct']];
 return <section className="qc-history"><header><History size={17}/><h3>{t.title}</h3><span>{list.data?.unfilteredTotal??'—'}</span><Info text={t.help}/></header>
 {error||list.error?<p role="alert">{bx[(error||list.error) as keyof typeof bx]??tt[(error||list.error) as keyof typeof tt]??bt[(error||list.error) as keyof typeof bt]??t.error}</p>:null}
 <ListControls locale={locale} query={list.query} change={patch=>{list.change(patch);if(patch.trash!==undefined){setA(null);setB(null);}}} kind="history" disabled={pending}/>
 <BulkBar locale={locale} selection={bulk} visible={runs.map(r=>r.id)} eligible={runs.map(r=>r.id)} trash={list.query.trash} disabled={active||pending} onApply={()=>void applyBatch()}/>
 {outcome?.scope===bulk.scope?<small className="bulk-outcome" role="status">{bx.completed}: {outcome.result.succeeded.length} · {bx.failed}: {outcome.result.failed.length}</small>:null}
 {outcome?.scope===bulk.scope&&outcome.result.failed.length?<details className="bulk-failures"><summary>{bx.failed}</summary>{outcome.result.failed.map(f=><p key={f.id}><code>{f.id}</code> · {tt[f.error as keyof typeof tt]??t.error}</p>)}</details>:null}
 <div className="qc-history-list">{runs.map(r=><div className="qc-history-row" key={r.id}><input type="checkbox" aria-label={`${bx.selectItem}: ${r.name} ${r.id}`} checked={bulk.ids.has(r.id)} disabled={active||pending||(!bulk.ids.has(r.id)&&bulk.ids.size>=selectionLimit)} onChange={()=>bulk.toggle(r.id)}/><div><strong>{r.name}</strong><small>{new Date(r.createdAt).toLocaleString(locale)} · {r.id.slice(0,8)}</small><span>{r.options.donorColumn||'—'} · {t.retained} {r.retained.toLocaleString()} · {t.excluded} {r.excluded.toLocaleString()}</span><div className="qc-retention-track"><span style={{width:`${100*r.retained/r.cells}%`}}/></div></div><div className="qc-history-row-actions">{!list.query.trash?<><button className="button" aria-pressed={left===r.id} disabled={active||pending} onClick={()=>setA(r)}>{t.left}</button><button className="button" aria-pressed={right===r.id} disabled={active||pending} onClick={()=>setB(r)}>{t.right}</button><button className="button" disabled={active||pending||r.id===job.report?.selection?.id} onClick={()=>void action('select_qc_history',{id:r.id})}>{r.id===job.report?.selection?.id?<Check size={14}/>:null}{r.id===job.report?.selection?.id?t.current:t.select}</button></>:null}<IconButton label={list.query.trash?tt.restore:tt.move} disabled={active||pending} onClick={()=>void action('set_trash',{kind:'history',id:r.id,restore:list.query.trash})}>{list.query.trash?<Undo2 size={15}/>:<Trash2 size={15}/>}</IconButton></div></div>)}{!list.data&&!list.error?<small role="status">{bt.loading}</small>:list.data&&!runs.length?<small>{list.data.unfilteredTotal===0?(list.query.trash?tt.empty:t.empty):bt.noResults}</small>:null}</div>
 <ListPager locale={locale} data={list.data} change={list.change} disabled={pending}/>
 <div className="qc-pinned">{a?<button type="button" title={bt.unpin} onClick={()=>setA(null)}>{t.left}: {a.name} · {a.id.slice(0,8)} ×</button>:null}{b?<button type="button" title={bt.unpin} onClick={()=>setB(null)}>{t.right}: {b.name} · {b.id.slice(0,8)} ×</button>:null}</div>
 {(list.data?.unfilteredTotal??0)>1||a||b?<><div className="qc-history-pickers"><button className="button" disabled={!a||!b||active||pending} onClick={()=>void action('compare_qc_history',{left,right})}><ArrowLeftRight size={14}/>{t.compare}</button></div>
 {a&&b?<div className="qc-comparison"><table><thead><tr><th>{t.conditions}</th><th>{t.left}</th><th>{t.right}</th></tr></thead><tbody>{fields.map(([key,name])=><tr key={key} className={a.options[key]!==b.options[key]?'qc-changed':''}><th>{t[name]}</th><td>{String(a.options[key]??'—')||'—'}</td><td>{String(b.options[key]??'—')||'—'}</td></tr>)}{(['retained','excluded'] as const).map(key=><tr key={key}><th>{t[key]}</th><td>{a[key].toLocaleString()}</td><td>{b[key].toLocaleString()}</td></tr>)}</tbody></table>
 {comparison?comparison.comparable?<><div className="qc-transition-bar" role="img" aria-label={`${t.bothRetained}: ${comparison.bothRetained}; ${t.removed}: ${comparison.removed}; ${t.added}: ${comparison.added}; ${t.bothExcluded}: ${comparison.bothExcluded}`}>{(['bothRetained','removed','added','bothExcluded'] as const).map(key=><span key={key} className={key} style={{flex:comparison[key]??0}}/>)}</div><div className="qc-transition-grid">{(['bothRetained','removed','added','bothExcluded'] as const).map(key=><div key={key} className={key}><b>{comparison[key]?.toLocaleString()}</b><span>{t[key]}</span></div>)}</div></>:<p>{t.different}</p>:null}</div>:null}</>:null}</section>;
}
