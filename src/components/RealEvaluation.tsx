import { resourceText } from '../lib/resourceI18n';
import { useEffect, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { BarChart3, Check, Play, Save, Square } from 'lucide-react';
import { refresh } from '../lib/bridge';
import { cancelH5ad, refreshH5ad, useH5ad, workerActive } from '../lib/h5ad';
import { h5adText } from '../lib/h5adI18n';
import { evaluationText } from '../lib/evaluationI18n';
import { cohortText } from '../lib/cohortI18n';
import { datasetText } from '../lib/datasetI18n';
import { exportText } from '../lib/exportI18n';
import type { ResearchDocument } from '../lib/types';
import { Info, IconButton } from './ui';

export function RealEvaluation({document}:{document:ResearchDocument}) {
 const job=useH5ad();
 return <EvaluationEditor key={job.projectKey} document={document}/>;
}
function EvaluationEditor({document}:{document:ResearchDocument}) {
 const job=useH5ad(), locale=document.workspace.locale, t=evaluationText[locale], ct=cohortText[locale], dt=datasetText[locale], et=exportText[locale];
 const plan=document.evaluationPlan, planKey=JSON.stringify(plan);
 const [draft,setDraft]=useState(plan),[reviewed,setReviewed]=useState(!!plan.reviewedFor),[busy,setBusy]=useState(''),[error,setError]=useState(''),[timeout,setTimeoutSeconds]=useState(120),[selected,setSelected]=useState(''),[source,setSource]=useState(''),[signature,setSignature]=useState('');
 useEffect(()=>{setDraft(plan);setReviewed(!!plan.reviewedFor);},[planKey]);
 useEffect(()=>{let live=true;setSignature('');if(isTauri())void invoke<string>('evaluation_context_sha',{projectKey:job.projectKey}).then(s=>{if(live)setSignature(s);}).catch(()=>{});return()=>{live=false;};},[document.revision,job.projectKey,job.status]);
 const dirty=JSON.stringify(draft)!==JSON.stringify(plan)||reviewed!==!!plan.reviewedFor;
 const active=workerActive(job.status), disabled=!!busy||active||!isTauri();
 const reports=[...document.evaluations].reverse(), report=reports.find(r=>r.id===selected)??reports[0];
 const inspection=report?.kind==='inspection'?report:reports.find(r=>r.id===report?.inspectionId);
 const rows=inspection?.outcome?.inputs??[], row=rows.find(r=>r.sourceSha256===source)??rows.find(r=>r.donors.length>0)??rows[0], input=report?.context.inputs.find(i=>i.sourceSha256===row?.sourceSha256);
 function message(code:string){return resourceText[locale][code as keyof typeof resourceText.en]??t[code as keyof typeof t]??ct[code as keyof typeof ct]??dt[code as keyof typeof dt]??et[code as keyof typeof et]??t.error;}
 async function action(command:string,args:Record<string,unknown>={}){
  setBusy(command);setError('');try{await invoke(command,{projectKey:job.projectKey,expectedRevision:document.revision,...args});if(command==='inspect_real_outcome')await refreshH5ad(true);else await Promise.all([refresh(),refreshH5ad(true)]);setSelected('');}catch(e){setError(String(e));}finally{setBusy('');}
 }
 return <section className="dataset-registry real-evaluation"><header><BarChart3 size={18}/><h2>{t.title}</h2><Info text={t.help}/><span>{t.model}</span></header>
 <form className="evaluation-form" onSubmit={e=>{e.preventDefault();void action('set_evaluation_plan',{spec:draft.spec,evidence:draft.measurementEvidence,reviewed});}}>
 <label>{t.feature}<input aria-label={t.feature} value={draft.spec.featureId} maxLength={200} disabled={disabled} placeholder="ISG15" onChange={e=>{setReviewed(false);setDraft({...draft,spec:{...draft.spec,featureId:e.target.value}});}}/></label>
 <label>{t.column}<input aria-label={t.column} value={draft.spec.featureColumn} maxLength={200} disabled={disabled} onChange={e=>{setReviewed(false);setDraft({...draft,spec:{...draft.spec,featureColumn:e.target.value}});}}/></label>
 <label className="evaluation-evidence">{t.evidence}<input value={draft.measurementEvidence} maxLength={2000} disabled={disabled} onChange={e=>setDraft({...draft,measurementEvidence:e.target.value})}/></label>
 <label className="evaluation-review"><input type="checkbox" checked={reviewed} disabled={disabled} onChange={e=>setReviewed(e.target.checked)}/>{t.review}{plan.reviewedFor===signature&&signature?<Check size={14}/>:null}</label>
 <button className="button" disabled={disabled||!draft.spec.featureId.trim()||!draft.spec.featureColumn.trim()||reviewed&&!draft.measurementEvidence.trim()}><Save size={14}/>{t.save}</button>
 </form>
 {dirty?<small>{t.dirty}</small>:null}
 <div className="evaluation-actions"><select aria-label={t.timeout} value={timeout} disabled={disabled} onChange={e=>setTimeoutSeconds(Number(e.target.value))}>{[1,30,120,600,3600].map(v=><option key={v} value={v}>{v} s</option>)}</select><button className="button" disabled={disabled||dirty||!plan.spec.featureId||document.evaluations.length>=8} onClick={()=>void action('inspect_real_outcome',{timeoutSeconds:timeout})}><BarChart3 size={14}/>{t.inspect}</button><button className="button primary" disabled={disabled||dirty||!plan.spec.featureId||document.evaluations.length>=8} onClick={()=>void action('run_real_baseline')}><Play size={14}/>{busy==='run_real_baseline'?t.checking:t.run}</button></div>
 {active&&job.operation==='inspect_outcome'?<div className="definition-progress" role="status"><span>{t.working} {job.percent}%</span><progress value={job.percent} max={100}/><IconButton label={t.cancel} disabled={job.status==='cancelling'} onClick={()=>void cancelH5ad().catch(e=>setError(String(e)))}><Square size={13}/></IconButton></div>:null}
 {job.operation==='inspect_outcome'&&(job.status==='cancelled'||job.status==='timed_out')?<p role="status">{h5adText[locale][job.status]}</p>:null}
 {error?<p role="alert">{message(error)}</p>:null}{job.operation==='inspect_outcome'&&job.error?<p role="alert">{message(job.error)}</p>:null}
 {!document.cohorts.length?<small>{t.empty}</small>:null}
 {report?<><label className="evaluation-history">{t.history}<select value={report.id} onChange={e=>{setSelected(e.target.value);setSource('');}}>{reports.map(r=><option key={r.id} value={r.id}>{r.kind==='inspection'?t.observation:t.baseline} · {new Date(r.createdAt).toLocaleString(locale)} · {r.context.spec.featureId}</option>)}</select></label>
 <div className="evaluation-snapshot"><span>{t.saved} · {report.context.spec.featureId}</span><span>{signature&&signature===report.contextSha256?t.current:t.stale}</span></div>
 {report.kind==='baseline'?<div className="evaluation-score"><strong>{report.abstained?t.abstained:`${t.baseline}: ${report.mae?.toFixed(4)}`}</strong>{report.controlMean!==null?<span>M0 = {report.controlMean.toFixed(4)} · {ct.trainDonors}: {report.trainDonors} · {ct.testDonors}: {report.testDonors}</span>:null}</div>:null}
 {report.reasons.length?<ul className="evaluation-reasons">{report.reasons.map((r,i)=><li key={i}>{message(r.code)}{r.sourceSha256?<small> · {report.context.inputs.find(s=>s.sourceSha256===r.sourceSha256)?.sourceName||r.sourceSha256.slice(0,8)}</small>:null}</li>)}</ul>:null}
 {row?<><select className="evaluation-source" aria-label={t.observation} value={row.sourceSha256} onChange={e=>setSource(e.target.value)}>{rows.map(r=><option key={r.sourceSha256} value={r.sourceSha256}>{report.context.inputs.find(i=>i.sourceSha256===r.sourceSha256)?.sourceName} · {r.sourceSha256.slice(0,8)}</option>)}</select><div className="evaluation-chart" aria-label={t.observation}>{row.donors.map(d=>{const mapping=input?.cohort.settings.mappings.find(m=>m.label===d.label);return <div className="evaluation-donor" key={d.label}><div><strong>{d.label}</strong><small>{mapping?.canonicalId?`${mapping.canonicalId} · `:''}{mapping?.reviewed?ct.confirmed:ct.draft}</small></div><div className="evaluation-bar" role="img" aria-label={`${d.label}: ${d.mean.toFixed(4)} log1p CP10K, ${d.cells} ${t.cells}`}><span style={{width:`${100*d.mean/Math.log1p(10000)}%`}}/></div><span>{d.mean.toFixed(4)}<small>{d.cells.toLocaleString()} {t.cells}</small></span></div>;})}</div><small>{t.observation} · log1p CP10K · {row.matrix} · {row.featureCount.toLocaleString()}</small></>:null}
 <details><summary>{t.saved} · SHA-256</summary><code>{report.contextSha256}</code>{row?.featureSpaceSha256?<code>{row.featureSpaceSha256}</code>:null}{report.inspectionId?<code>{report.inspectionId}</code>:null}<time>{report.createdAt}</time>{report.measurementEvidence?<p>{report.measurementEvidence}</p>:null}</details></>:null}
 </section>;
}
